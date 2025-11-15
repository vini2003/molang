use crate::ast::{BinaryOp, ControlFlowExpr, Expr, Program, Statement};
use crate::eval::{QualifiedName, RuntimeContext, Value};
use crate::ir::BuiltinFunction;
use indexmap::IndexMap;
use thiserror::Error;

const MAX_LOOP_ITERATIONS: usize = 1024;

/// Statement interpreter used whenever expressions include features not supported by
/// the JIT (loops, arrays, strings, flow control, etc.).
#[derive(Default)]
pub struct Executor;

impl Executor {
    /// Runs a Molang program against the provided runtime context.
    pub fn execute(
        &mut self,
        program: &Program,
        ctx: &mut RuntimeContext,
    ) -> Result<Value, ExecError> {
        for statement in &program.statements {
            match self.exec_statement(statement, ctx, 0)? {
                ControlSignal::None => {}
                ControlSignal::Break | ControlSignal::Continue => {
                    return Err(ExecError::FlowOutsideLoop);
                }
                ControlSignal::Return(value) => return Ok(value),
            }
        }
        Ok(Value::number(0.0))
    }

    /// Executes a single statement, tracking the current loop depth to validate
    /// `break`/`continue` usage.
    fn exec_statement(
        &mut self,
        statement: &Statement,
        ctx: &mut RuntimeContext,
        loop_depth: usize,
    ) -> Result<ControlSignal, ExecError> {
        match statement {
            Statement::Expr(expr) => {
                let (_, signal) = self.eval_expr(expr, ctx)?;
                self.validate_signal(loop_depth, &signal)?;
                Ok(signal)
            }
            Statement::Assignment { target, value } => {
                let (result, signal) = self.eval_expr(value, ctx)?;
                if matches!(signal, ControlSignal::None) {
                    ctx.set_value_for_path(target, result);
                } else {
                    self.validate_signal(loop_depth, &signal)?;
                }
                Ok(signal)
            }
            Statement::Block(statements) => {
                for stmt in statements {
                    let signal = self.exec_statement(stmt, ctx, loop_depth)?;
                    if !matches!(signal, ControlSignal::None) {
                        return Ok(signal);
                    }
                }
                Ok(ControlSignal::None)
            }
            Statement::Loop { count, body } => self.exec_loop(count, body, ctx, loop_depth),
            Statement::ForEach {
                variable,
                collection,
                body,
            } => self.exec_for_each(variable, collection, body, ctx, loop_depth),
            Statement::Return(value) => {
                if let Some(expr) = value {
                    let (result, signal) = self.eval_expr(expr, ctx)?;
                    if matches!(signal, ControlSignal::None) {
                        Ok(ControlSignal::Return(result))
                    } else {
                        Ok(signal)
                    }
                } else {
                    Ok(ControlSignal::Return(Value::number(0.0)))
                }
            }
        }
    }

    /// Executes a `loop(count, ...)` call, respecting the global iteration limit.
    fn exec_loop(
        &mut self,
        count_expr: &Expr,
        body: &Statement,
        ctx: &mut RuntimeContext,
        loop_depth: usize,
    ) -> Result<ControlSignal, ExecError> {
        let (count_value, signal) = self.eval_expr(count_expr, ctx)?;
        if !matches!(signal, ControlSignal::None) {
            return Ok(signal);
        }
        let iterations = clamp_loop_count(count_value.as_number());

        for _ in 0..iterations {
            match self.exec_statement(body, ctx, loop_depth + 1)? {
                ControlSignal::None => {}
                ControlSignal::Continue => continue,
                ControlSignal::Break => break,
                ControlSignal::Return(value) => return Ok(ControlSignal::Return(value)),
            }
        }

        Ok(ControlSignal::None)
    }

    /// Executes `for_each(variable, collection, ...)`, binding each Array element to `variable`.
    fn exec_for_each(
        &mut self,
        variable: &[String],
        collection: &Expr,
        body: &Statement,
        ctx: &mut RuntimeContext,
        loop_depth: usize,
    ) -> Result<ControlSignal, ExecError> {
        let (collection_value, signal) = self.eval_expr(collection, ctx)?;
        if !matches!(signal, ControlSignal::None) {
            return Ok(signal);
        }

        let array = match collection_value {
            Value::Array(values) => values,
            _ => return Ok(ControlSignal::None),
        };
        let variable_name = QualifiedName::from_parts(variable);

        for element in array {
            ctx.set_value_with_name(variable_name.clone(), element);
            match self.exec_statement(body, ctx, loop_depth + 1)? {
                ControlSignal::None => {}
                ControlSignal::Continue => continue,
                ControlSignal::Break => break,
                ControlSignal::Return(value) => return Ok(ControlSignal::Return(value)),
            }
        }

        Ok(ControlSignal::None)
    }

    /// Evaluates an expression tree, returning the computed `Value` and any control signal.
    fn eval_expr(
        &mut self,
        expr: &Expr,
        ctx: &RuntimeContext,
    ) -> Result<(Value, ControlSignal), ExecError> {
        match expr {
            Expr::Number(value) => Ok((Value::number(*value), ControlSignal::None)),
            Expr::Path(parts) => Ok((
                ctx.get_value_for_path(parts).unwrap_or(Value::Null),
                ControlSignal::None,
            )),
            Expr::Unary { op, expr } => {
                let (value, signal) = self.eval_expr(expr, ctx)?;
                if !matches!(signal, ControlSignal::None) {
                    return Ok((value, signal));
                }
                let result = match op {
                    crate::ast::UnaryOp::Plus => Value::number(value.as_number()),
                    crate::ast::UnaryOp::Minus => Value::number(-value.as_number()),
                    crate::ast::UnaryOp::Not => {
                        Value::number(if value.truthy() { 0.0 } else { 1.0 })
                    }
                };
                Ok((result, ControlSignal::None))
            }
            Expr::Binary { op, left, right } => self.eval_binary(op, left, right, ctx),
            Expr::Conditional {
                condition,
                then_branch,
                else_branch,
            } => {
                let (condition_value, signal) = self.eval_expr(condition, ctx)?;
                if !matches!(signal, ControlSignal::None) {
                    return Ok((condition_value, signal));
                }
                if condition_value.truthy() {
                    self.eval_expr(then_branch, ctx)
                } else if let Some(branch) = else_branch {
                    self.eval_expr(branch, ctx)
                } else {
                    Ok((Value::number(0.0), ControlSignal::None))
                }
            }
            Expr::Call { target, args } => self.eval_call(target, args, ctx),
            Expr::Flow(flow) => match flow {
                ControlFlowExpr::Break => Ok((Value::Null, ControlSignal::Break)),
                ControlFlowExpr::Continue => Ok((Value::Null, ControlSignal::Continue)),
            },
            Expr::Array(values) => {
                let mut evaluated = Vec::with_capacity(values.len());
                for value in values {
                    let (result, signal) = self.eval_expr(value, ctx)?;
                    if !matches!(signal, ControlSignal::None) {
                        return Ok((result, signal));
                    }
                    evaluated.push(result);
                }
                Ok((Value::array(evaluated), ControlSignal::None))
            }
            Expr::Struct(fields) => {
                let mut map = IndexMap::new();
                for (key, expr) in fields {
                    let (value, signal) = self.eval_expr(expr, ctx)?;
                    if !matches!(signal, ControlSignal::None) {
                        return Ok((value, signal));
                    }
                    map.insert(key.to_ascii_lowercase(), value);
                }
                Ok((Value::Struct(map), ControlSignal::None))
            }
            Expr::Index { target, index } => {
                let (collection, signal) = self.eval_expr(target, ctx)?;
                if !matches!(signal, ControlSignal::None) {
                    return Ok((collection, signal));
                }
                let (idx_value, signal) = self.eval_expr(index, ctx)?;
                if !matches!(signal, ControlSignal::None) {
                    return Ok((idx_value, signal));
                }
                let result = match collection {
                    Value::Array(values) => {
                        let idx = normalize_index(idx_value.as_number(), values.len());
                        values.get(idx).cloned().unwrap_or(Value::Null)
                    }
                    _ => Value::Null,
                };
                Ok((result, ControlSignal::None))
            }
            Expr::String(text) => Ok((Value::string(text), ControlSignal::None)),
        }
    }

    /// Evaluates binary operators with Molang semantics.
    fn eval_binary(
        &mut self,
        op: &BinaryOp,
        left: &Expr,
        right: &Expr,
        ctx: &RuntimeContext,
    ) -> Result<(Value, ControlSignal), ExecError> {
        use BinaryOp::*;
        match op {
            And | Or | NullCoalesce => {
                let (left_value, signal) = self.eval_expr(left, ctx)?;
                if !matches!(signal, ControlSignal::None) {
                    return Ok((left_value, signal));
                }
                match op {
                    And => {
                        if !left_value.truthy() {
                            return Ok((Value::number(0.0), ControlSignal::None));
                        }
                        let (right_value, signal) = self.eval_expr(right, ctx)?;
                        if !matches!(signal, ControlSignal::None) {
                            return Ok((right_value, signal));
                        }
                        Ok((
                            Value::number(if right_value.truthy() { 1.0 } else { 0.0 }),
                            ControlSignal::None,
                        ))
                    }
                    Or => {
                        if left_value.truthy() {
                            return Ok((Value::number(1.0), ControlSignal::None));
                        }
                        self.eval_expr(right, ctx)
                    }
                    NullCoalesce => {
                        if is_null_like(&left_value) {
                            self.eval_expr(right, ctx)
                        } else {
                            Ok((left_value, ControlSignal::None))
                        }
                    }
                    _ => unreachable!(),
                }
            }
            Add | Sub | Mul | Div | Less | LessEqual | Greater | GreaterEqual | Equal
            | NotEqual => {
                let (left_value, signal) = self.eval_expr(left, ctx)?;
                if !matches!(signal, ControlSignal::None) {
                    return Ok((left_value, signal));
                }
                let (right_value, signal) = self.eval_expr(right, ctx)?;
                if !matches!(signal, ControlSignal::None) {
                    return Ok((right_value, signal));
                }
                let left_number = left_value.as_number();
                let right_number = right_value.as_number();
                let result = match op {
                    Add => Value::number(left_number + right_number),
                    Sub => Value::number(left_number - right_number),
                    Mul => Value::number(left_number * right_number),
                    Div => Value::number(left_number / right_number),
                    Less => Value::number(bool_to_float(left_number < right_number)),
                    LessEqual => Value::number(bool_to_float(left_number <= right_number)),
                    Greater => Value::number(bool_to_float(left_number > right_number)),
                    GreaterEqual => Value::number(bool_to_float(left_number >= right_number)),
                    Equal => Value::number(bool_to_float(float_equals(left_number, right_number))),
                    NotEqual => {
                        Value::number(bool_to_float(!float_equals(left_number, right_number)))
                    }
                    _ => unreachable!(),
                };
                Ok((result, ControlSignal::None))
            }
        }
    }

    /// Invokes a builtin math function. User-defined calls are unsupported.
    fn eval_call(
        &mut self,
        target: &Expr,
        args: &[Expr],
        ctx: &RuntimeContext,
    ) -> Result<(Value, ControlSignal), ExecError> {
        match target {
            Expr::Path(parts) => {
                if let Some(builtin) = BuiltinFunction::from_path(parts) {
                    let mut evaluated = Vec::with_capacity(args.len());
                    for arg in args {
                        let (value, signal) = self.eval_expr(arg, ctx)?;
                        if !matches!(signal, ControlSignal::None) {
                            return Ok((value, signal));
                        }
                        evaluated.push(value.as_number());
                    }
                    let result = Value::number(builtin.evaluate(&evaluated));
                    Ok((result, ControlSignal::None))
                } else {
                    Err(ExecError::UnknownFunction {
                        name: parts.join("."),
                    })
                }
            }
            _ => Err(ExecError::InvalidCallTarget),
        }
    }

    fn validate_signal(&self, loop_depth: usize, signal: &ControlSignal) -> Result<(), ExecError> {
        match signal {
            ControlSignal::Break | ControlSignal::Continue if loop_depth == 0 => {
                Err(ExecError::FlowOutsideLoop)
            }
            _ => Ok(()),
        }
    }
}

#[derive(Debug, Clone)]
enum ControlSignal {
    None,
    Break,
    Continue,
    Return(Value),
}

/// Matches Bedrock’s safety limit: loops of zero or negative count skip execution,
/// and any positive value is clamped to 1024 iterations.
fn clamp_loop_count(value: f64) -> usize {
    if value.is_nan() || value <= 0.0 {
        0
    } else {
        value.floor().min(MAX_LOOP_ITERATIONS as f64).max(0.0) as usize
    }
}

fn normalize_index(index: f64, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    if index.is_nan() {
        return 0;
    }
    let idx = index.floor() as i64;
    if idx < 0 {
        0
    } else {
        (idx as usize) % len
    }
}

fn bool_to_float(value: bool) -> f64 {
    if value {
        1.0
    } else {
        0.0
    }
}

fn float_equals(left: f64, right: f64) -> bool {
    (left - right).abs() < f64::EPSILON
}

fn is_null_like(value: &Value) -> bool {
    matches!(value, Value::Null)
}

#[derive(Debug, Error)]
pub enum ExecError {
    #[error("control flow outside of loop")]
    FlowOutsideLoop,
    #[error("unknown function `{name}`")]
    UnknownFunction { name: String },
    #[error("invalid call target")]
    InvalidCallTarget,
}
