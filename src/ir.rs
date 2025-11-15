use crate::ast::{BinaryOp, Expr, UnaryOp};
use thiserror::Error;

#[derive(Debug, Clone)]
pub enum IrExpr {
    Constant(f64),
    Path(Vec<String>),
    Unary {
        op: UnaryOp,
        expr: Box<IrExpr>,
    },
    Binary {
        op: BinaryOp,
        left: Box<IrExpr>,
        right: Box<IrExpr>,
    },
    Conditional {
        condition: Box<IrExpr>,
        then_branch: Box<IrExpr>,
        else_branch: Option<Box<IrExpr>>,
    },
    Call {
        function: FunctionRef,
        args: Vec<IrExpr>,
    },
}

#[derive(Debug, Clone, Copy)]
pub enum FunctionRef {
    Builtin(BuiltinFunction),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuiltinFunction {
    MathCos,
    MathSin,
    MathAbs,
    MathRandom,
    MathRandomInteger,
    MathClamp,
    MathSqrt,
    MathFloor,
    MathCeil,
    MathRound,
    MathTrunc,
}

impl BuiltinFunction {
    pub fn from_path(path: &[String]) -> Option<Self> {
        match path {
            [ns, func] if ns == "math" => match func.as_str() {
                "cos" => Some(BuiltinFunction::MathCos),
                "sin" => Some(BuiltinFunction::MathSin),
                "abs" => Some(BuiltinFunction::MathAbs),
                "random" => Some(BuiltinFunction::MathRandom),
                "random_integer" => Some(BuiltinFunction::MathRandomInteger),
                "clamp" => Some(BuiltinFunction::MathClamp),
                "sqrt" => Some(BuiltinFunction::MathSqrt),
                "floor" => Some(BuiltinFunction::MathFloor),
                "ceil" => Some(BuiltinFunction::MathCeil),
                "round" => Some(BuiltinFunction::MathRound),
                "trunc" => Some(BuiltinFunction::MathTrunc),
                _ => None,
            },
            _ => None,
        }
    }

    pub fn arity(self) -> usize {
        match self {
            BuiltinFunction::MathRandom | BuiltinFunction::MathRandomInteger => 2,
            BuiltinFunction::MathClamp => 3,
            _ => 1,
        }
    }

    pub fn symbol_name(self) -> &'static str {
        match self {
            BuiltinFunction::MathCos => "molang_builtin_cos",
            BuiltinFunction::MathSin => "molang_builtin_sin",
            BuiltinFunction::MathAbs => "molang_builtin_abs",
            BuiltinFunction::MathRandom => "molang_builtin_random",
            BuiltinFunction::MathRandomInteger => "molang_builtin_random_integer",
            BuiltinFunction::MathClamp => "molang_builtin_clamp",
            BuiltinFunction::MathSqrt => "molang_builtin_sqrt",
            BuiltinFunction::MathFloor => "molang_builtin_floor",
            BuiltinFunction::MathCeil => "molang_builtin_ceil",
            BuiltinFunction::MathRound => "molang_builtin_round",
            BuiltinFunction::MathTrunc => "molang_builtin_trunc",
        }
    }

    pub fn evaluate(self, args: &[f64]) -> f64 {
        match self {
            BuiltinFunction::MathCos => args.first().copied().unwrap_or(0.0).cos(),
            BuiltinFunction::MathSin => args.first().copied().unwrap_or(0.0).sin(),
            BuiltinFunction::MathAbs => args.first().copied().unwrap_or(0.0).abs(),
            BuiltinFunction::MathRandom => crate::builtins::math_random(
                args.get(0).copied().unwrap_or(0.0),
                args.get(1).copied().unwrap_or(1.0),
            ),
            BuiltinFunction::MathRandomInteger => crate::builtins::math_random_integer(
                args.get(0).copied().unwrap_or(0.0),
                args.get(1).copied().unwrap_or(1.0),
            ),
            BuiltinFunction::MathClamp => crate::builtins::math_clamp(
                args.get(0).copied().unwrap_or(0.0),
                args.get(1).copied().unwrap_or(0.0),
                args.get(2).copied().unwrap_or(0.0),
            ),
            BuiltinFunction::MathSqrt => args.first().copied().unwrap_or(0.0).sqrt(),
            BuiltinFunction::MathFloor => args.first().copied().unwrap_or(0.0).floor(),
            BuiltinFunction::MathCeil => args.first().copied().unwrap_or(0.0).ceil(),
            BuiltinFunction::MathRound => args.first().copied().unwrap_or(0.0).round(),
            BuiltinFunction::MathTrunc => args.first().copied().unwrap_or(0.0).trunc(),
        }
    }
}

#[derive(Default)]
pub struct IrBuilder;

impl IrBuilder {
    pub fn lower(&self, expr: &Expr) -> Result<IrExpr, LowerError> {
        self.lower_expr(expr)
    }

    fn lower_expr(&self, expr: &Expr) -> Result<IrExpr, LowerError> {
        match expr {
            Expr::Number(value) => Ok(IrExpr::Constant(*value)),
            Expr::Path(parts) => Ok(IrExpr::Path(parts.clone())),
            Expr::String(_) | Expr::Array(_) => Err(LowerError::UnsupportedLiteral),
            Expr::Unary { op, expr } => Ok(IrExpr::Unary {
                op: *op,
                expr: Box::new(self.lower_expr(expr)?),
            }),
            Expr::Binary { op, left, right } => Ok(IrExpr::Binary {
                op: *op,
                left: Box::new(self.lower_expr(left)?),
                right: Box::new(self.lower_expr(right)?),
            }),
            Expr::Conditional {
                condition,
                then_branch,
                else_branch,
            } => Ok(IrExpr::Conditional {
                condition: Box::new(self.lower_expr(condition)?),
                then_branch: Box::new(self.lower_expr(then_branch)?),
                else_branch: match else_branch {
                    Some(expr) => Some(Box::new(self.lower_expr(expr)?)),
                    None => None,
                },
            }),
            Expr::Call { target, args } => {
                let lowered_args = args
                    .iter()
                    .map(|arg| self.lower_expr(arg))
                    .collect::<Result<Vec<_>, _>>()?;
                let function = self.lower_call_target(target)?;
                self.validate_call(&function, lowered_args.len())?;
                Ok(IrExpr::Call {
                    function,
                    args: lowered_args,
                })
            }
            Expr::Flow(_) => Err(LowerError::UnsupportedControlFlow),
        }
    }

    fn lower_call_target(&self, target: &Expr) -> Result<FunctionRef, LowerError> {
        match target {
            Expr::Path(parts) => {
                if let Some(builtin) = BuiltinFunction::from_path(parts) {
                    Ok(FunctionRef::Builtin(builtin))
                } else {
                    Err(LowerError::UnknownFunction {
                        name: parts.join("."),
                    })
                }
            }
            other => Err(LowerError::UnsupportedCallTarget {
                description: format!("{other:?}"),
            }),
        }
    }

    fn validate_call(&self, function: &FunctionRef, arg_count: usize) -> Result<(), LowerError> {
        match function {
            FunctionRef::Builtin(builtin) => {
                let expected = builtin.arity();
                if expected != arg_count {
                    Err(LowerError::InvalidArgumentCount {
                        name: builtin.symbol_name().to_string(),
                        expected,
                        actual: arg_count,
                    })
                } else {
                    Ok(())
                }
            }
        }
    }
}

#[derive(Debug, Error)]
pub enum LowerError {
    #[error("unknown function `{name}`")]
    UnknownFunction { name: String },
    #[error("unsupported call target: {description}")]
    UnsupportedCallTarget { description: String },
    #[error("invalid argument count for `{name}`: expected {expected}, got {actual}")]
    InvalidArgumentCount {
        name: String,
        expected: usize,
        actual: usize,
    },
    #[error("control flow expressions cannot be lowered for JIT execution")]
    UnsupportedControlFlow,
    #[error("literal expressions are not supported by the JIT")]
    UnsupportedLiteral,
}
