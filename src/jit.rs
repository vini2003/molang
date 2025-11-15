use crate::ast::{BinaryOp, UnaryOp};
use crate::builtins;
use crate::eval::{QualifiedName, RuntimeContext};
use crate::ir::{BuiltinFunction, FunctionRef, IrExpr};
use cranelift::prelude::*;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{FuncId, Linkage, Module};
use std::collections::HashMap;
use thiserror::Error;

pub struct CompiledExpression {
    module: JITModule,
    func_id: FuncId,
    slot_names: Vec<QualifiedName>,
}

impl CompiledExpression {
    pub fn evaluate(&self, ctx: &RuntimeContext) -> Result<f64, JitError> {
        let mut slots = Vec::with_capacity(self.slot_names.len());
        for name in &self.slot_names {
            let value = ctx
                .get_number(name)
                .ok_or_else(|| JitError::UnknownVariable {
                    name: name.to_string(),
                })?;
            slots.push(value);
        }

        let func = unsafe {
            let raw = self.module.get_finalized_function(self.func_id);
            std::mem::transmute::<*const u8, extern "C" fn(*const f64) -> f64>(raw)
        };
        Ok(func(slots.as_ptr()))
    }
}

pub fn compile_expression(expr: &IrExpr) -> Result<CompiledExpression, JitError> {
    let mut builder = JITBuilder::new(cranelift_module::default_libcall_names())?;
    register_builtin_symbols(&mut builder);
    let mut module = JITModule::new(builder);
    let mut ctx = module.make_context();
    ctx.func
        .signature
        .params
        .push(AbiParam::new(module.target_config().pointer_type()));
    ctx.func.signature.returns.push(AbiParam::new(types::F64));

    let mut func_ctx = FunctionBuilderContext::new();
    let slot_names = {
        let mut builder = FunctionBuilder::new(&mut ctx.func, &mut func_ctx);
        let entry = builder.create_block();
        builder.append_block_params_for_function_params(entry);
        builder.switch_to_block(entry);
        builder.seal_block(entry);

        let values_ptr = builder.block_params(entry)[0];
        let (result, slot_names) = {
            let mut translator = Translator::new(&mut builder, &mut module, values_ptr);
            let value = translator.translate(expr)?;
            (value, translator.into_slot_names())
        };
        builder.ins().return_(&[result]);
        builder.finalize();
        slot_names
    };

    let func_id = module.declare_function("molang_expr", Linkage::Export, &ctx.func.signature)?;
    module.define_function(func_id, &mut ctx)?;
    module.clear_context(&mut ctx);
    module.finalize_definitions()?;

    Ok(CompiledExpression {
        module,
        func_id,
        slot_names,
    })
}

struct Translator<'a, 'b> {
    builder: &'a mut FunctionBuilder<'b>,
    module: &'a mut JITModule,
    values_ptr: Value,
    slot_names: Vec<QualifiedName>,
    slot_map: HashMap<QualifiedName, usize>,
    builtin_funcs: HashMap<BuiltinFunction, FuncId>,
}

impl<'a, 'b> Translator<'a, 'b> {
    fn new(
        builder: &'a mut FunctionBuilder<'b>,
        module: &'a mut JITModule,
        values_ptr: Value,
    ) -> Self {
        Self {
            builder,
            module,
            values_ptr,
            slot_names: Vec::new(),
            slot_map: HashMap::new(),
            builtin_funcs: HashMap::new(),
        }
    }

    fn translate(&mut self, expr: &IrExpr) -> Result<Value, JitError> {
        match expr {
            IrExpr::Constant(value) => Ok(self.builder.ins().f64const(Ieee64::with_float(*value))),
            IrExpr::Path(parts) => self.load_variable(parts),
            IrExpr::Unary { op, expr } => {
                let value = self.translate(expr)?;
                Ok(match op {
                    UnaryOp::Plus => value,
                    UnaryOp::Minus => self.builder.ins().fneg(value),
                    UnaryOp::Not => {
                        let bool_val = self.bool_from_value(value);
                        let inverted = self.builder.ins().bnot(bool_val);
                        self.float_from_bool(inverted)
                    }
                })
            }
            IrExpr::Binary { op, left, right } => match op {
                BinaryOp::Add => {
                    let (l, r) = self.translate_pair(left, right)?;
                    Ok(self.builder.ins().fadd(l, r))
                }
                BinaryOp::Sub => {
                    let (l, r) = self.translate_pair(left, right)?;
                    Ok(self.builder.ins().fsub(l, r))
                }
                BinaryOp::Mul => {
                    let (l, r) = self.translate_pair(left, right)?;
                    Ok(self.builder.ins().fmul(l, r))
                }
                BinaryOp::Div => {
                    let (l, r) = self.translate_pair(left, right)?;
                    Ok(self.builder.ins().fdiv(l, r))
                }
                BinaryOp::Less => self.emit_comparison(FloatCC::LessThan, left, right),
                BinaryOp::LessEqual => self.emit_comparison(FloatCC::LessThanOrEqual, left, right),
                BinaryOp::Greater => self.emit_comparison(FloatCC::GreaterThan, left, right),
                BinaryOp::GreaterEqual => {
                    self.emit_comparison(FloatCC::GreaterThanOrEqual, left, right)
                }
                BinaryOp::Equal => self.emit_comparison(FloatCC::Equal, left, right),
                BinaryOp::NotEqual => self.emit_comparison(FloatCC::NotEqual, left, right),
                BinaryOp::And => self.emit_logical_and(left, right),
                BinaryOp::Or => self.emit_logical_or(left, right),
                BinaryOp::NullCoalesce => self.emit_null_coalesce(left, right),
            },
            IrExpr::Conditional {
                condition,
                then_branch,
                else_branch,
            } => self.emit_conditional(condition, then_branch, else_branch.as_deref()),
            IrExpr::Call { function, args } => self.emit_call(*function, args),
        }
    }

    fn translate_pair(
        &mut self,
        left: &IrExpr,
        right: &IrExpr,
    ) -> Result<(Value, Value), JitError> {
        let left_val = self.translate(left)?;
        let right_val = self.translate(right)?;
        Ok((left_val, right_val))
    }

    fn load_variable(&mut self, parts: &[String]) -> Result<Value, JitError> {
        let name = QualifiedName::from_parts(parts);
        let slot = if let Some(slot) = self.slot_map.get(&name) {
            *slot
        } else {
            let slot = self.slot_names.len();
            self.slot_names.push(name.clone());
            self.slot_map.insert(name.clone(), slot);
            slot
        };
        let offset = (slot * std::mem::size_of::<f64>()) as i32;
        let flags = MemFlags::new();
        Ok(self
            .builder
            .ins()
            .load(types::F64, flags, self.values_ptr, offset))
    }

    fn emit_call(&mut self, function: FunctionRef, args: &[IrExpr]) -> Result<Value, JitError> {
        match function {
            FunctionRef::Builtin(builtin) => {
                let arg_values = args
                    .iter()
                    .map(|arg| self.translate(arg))
                    .collect::<Result<Vec<_>, _>>()?;
                self.emit_builtin_call(builtin, &arg_values)
            }
        }
    }

    fn emit_comparison(
        &mut self,
        cond: FloatCC,
        left: &IrExpr,
        right: &IrExpr,
    ) -> Result<Value, JitError> {
        let (left_val, right_val) = self.translate_pair(left, right)?;
        let cmp = self.builder.ins().fcmp(cond, left_val, right_val);
        Ok(self.float_from_bool(cmp))
    }

    fn emit_logical_and(&mut self, left: &IrExpr, right: &IrExpr) -> Result<Value, JitError> {
        let left_val = self.translate(left)?;
        let condition = self.bool_from_value(left_val);
        let then_block = self.builder.create_block();
        let else_block = self.builder.create_block();
        let merge_block = self.builder.create_block();
        let result_param = self.builder.append_block_param(merge_block, types::F64);

        self.builder
            .ins()
            .brif(condition, then_block, &[], else_block, &[]);

        self.builder.switch_to_block(then_block);
        let right_val = self.translate(right)?;
        let right_bool = self.bool_from_value(right_val);
        let right_float = self.float_from_bool(right_bool);
        self.builder.ins().jump(merge_block, &[right_float]);
        self.builder.seal_block(then_block);

        self.builder.switch_to_block(else_block);
        let zero = self.const_f64(0.0);
        self.builder.ins().jump(merge_block, &[zero]);
        self.builder.seal_block(else_block);

        self.builder.switch_to_block(merge_block);
        self.builder.seal_block(merge_block);
        Ok(result_param)
    }

    fn emit_logical_or(&mut self, left: &IrExpr, right: &IrExpr) -> Result<Value, JitError> {
        let left_val = self.translate(left)?;
        let condition = self.bool_from_value(left_val);
        let then_block = self.builder.create_block();
        let else_block = self.builder.create_block();
        let merge_block = self.builder.create_block();
        let result_param = self.builder.append_block_param(merge_block, types::F64);

        self.builder
            .ins()
            .brif(condition, then_block, &[], else_block, &[]);

        self.builder.switch_to_block(then_block);
        let one = self.const_f64(1.0);
        self.builder.ins().jump(merge_block, &[one]);
        self.builder.seal_block(then_block);

        self.builder.switch_to_block(else_block);
        let right_val = self.translate(right)?;
        let right_bool = self.bool_from_value(right_val);
        let right_float = self.float_from_bool(right_bool);
        self.builder.ins().jump(merge_block, &[right_float]);
        self.builder.seal_block(else_block);

        self.builder.switch_to_block(merge_block);
        self.builder.seal_block(merge_block);
        Ok(result_param)
    }

    fn emit_null_coalesce(&mut self, left: &IrExpr, right: &IrExpr) -> Result<Value, JitError> {
        let left_val = self.translate(left)?;
        let condition = self.bool_from_value(left_val);
        let then_block = self.builder.create_block();
        let else_block = self.builder.create_block();
        let merge_block = self.builder.create_block();
        let result_param = self.builder.append_block_param(merge_block, types::F64);

        self.builder
            .ins()
            .brif(condition, then_block, &[], else_block, &[]);

        self.builder.switch_to_block(then_block);
        self.builder.ins().jump(merge_block, &[left_val]);
        self.builder.seal_block(then_block);

        self.builder.switch_to_block(else_block);
        let right_val = self.translate(right)?;
        self.builder.ins().jump(merge_block, &[right_val]);
        self.builder.seal_block(else_block);

        self.builder.switch_to_block(merge_block);
        self.builder.seal_block(merge_block);
        Ok(result_param)
    }

    fn emit_conditional(
        &mut self,
        condition: &IrExpr,
        then_branch: &IrExpr,
        else_branch: Option<&IrExpr>,
    ) -> Result<Value, JitError> {
        let condition_value = self.translate(condition)?;
        let condition_bool = self.bool_from_value(condition_value);

        let then_block = self.builder.create_block();
        let else_block = self.builder.create_block();
        let merge_block = self.builder.create_block();
        let result_param = self.builder.append_block_param(merge_block, types::F64);

        self.builder
            .ins()
            .brif(condition_bool, then_block, &[], else_block, &[]);

        self.builder.switch_to_block(then_block);
        let then_value = self.translate(then_branch)?;
        self.builder.ins().jump(merge_block, &[then_value]);
        self.builder.seal_block(then_block);

        self.builder.switch_to_block(else_block);
        let else_value = match else_branch {
            Some(expr) => self.translate(expr)?,
            None => self.const_f64(0.0),
        };
        self.builder.ins().jump(merge_block, &[else_value]);
        self.builder.seal_block(else_block);

        self.builder.switch_to_block(merge_block);
        self.builder.seal_block(merge_block);
        Ok(result_param)
    }

    fn bool_from_value(&mut self, value: Value) -> Value {
        let zero = self.const_f64(0.0);
        self.builder.ins().fcmp(FloatCC::NotEqual, value, zero)
    }

    fn float_from_bool(&mut self, value: Value) -> Value {
        let int_value = self.builder.ins().uextend(types::I64, value);
        self.builder.ins().fcvt_from_sint(types::F64, int_value)
    }

    fn const_f64(&mut self, value: f64) -> Value {
        self.builder.ins().f64const(Ieee64::with_float(value))
    }

    fn emit_builtin_call(
        &mut self,
        builtin: BuiltinFunction,
        args: &[Value],
    ) -> Result<Value, JitError> {
        let func_id = self.ensure_builtin(builtin)?;
        let func_ref = self.module.declare_func_in_func(func_id, self.builder.func);
        let call = self.builder.ins().call(func_ref, args);
        let results = self.builder.inst_results(call);
        results
            .first()
            .copied()
            .ok_or(JitError::MissingReturnValue { function: builtin })
    }

    fn ensure_builtin(&mut self, builtin: BuiltinFunction) -> Result<FuncId, JitError> {
        if let Some(id) = self.builtin_funcs.get(&builtin) {
            return Ok(*id);
        }

        let mut sig = self.module.make_signature();
        for _ in 0..builtin.arity() {
            sig.params.push(AbiParam::new(types::F64));
        }
        sig.returns.push(AbiParam::new(types::F64));

        let func_id = self
            .module
            .declare_function(builtin.symbol_name(), Linkage::Import, &sig)?;
        self.builtin_funcs.insert(builtin, func_id);
        Ok(func_id)
    }

    fn into_slot_names(self) -> Vec<QualifiedName> {
        self.slot_names
    }
}

fn register_builtin_symbols(builder: &mut JITBuilder) {
    builder.symbol(
        BuiltinFunction::MathCos.symbol_name(),
        builtins::builtin_math_cos as *const u8,
    );
    builder.symbol(
        BuiltinFunction::MathSin.symbol_name(),
        builtins::builtin_math_sin as *const u8,
    );
    builder.symbol(
        BuiltinFunction::MathAbs.symbol_name(),
        builtins::builtin_math_abs as *const u8,
    );
    builder.symbol(
        BuiltinFunction::MathRandom.symbol_name(),
        builtins::builtin_math_random as *const u8,
    );
    builder.symbol(
        BuiltinFunction::MathRandomInteger.symbol_name(),
        builtins::builtin_math_random_integer as *const u8,
    );
    builder.symbol(
        BuiltinFunction::MathClamp.symbol_name(),
        builtins::builtin_math_clamp as *const u8,
    );
    builder.symbol(
        BuiltinFunction::MathSqrt.symbol_name(),
        builtins::builtin_math_sqrt as *const u8,
    );
    builder.symbol(
        BuiltinFunction::MathFloor.symbol_name(),
        builtins::builtin_math_floor as *const u8,
    );
    builder.symbol(
        BuiltinFunction::MathCeil.symbol_name(),
        builtins::builtin_math_ceil as *const u8,
    );
    builder.symbol(
        BuiltinFunction::MathRound.symbol_name(),
        builtins::builtin_math_round as *const u8,
    );
    builder.symbol(
        BuiltinFunction::MathTrunc.symbol_name(),
        builtins::builtin_math_trunc as *const u8,
    );
}

#[derive(Debug, Error)]
pub enum JitError {
    #[error(transparent)]
    Module(#[from] cranelift_module::ModuleError),
    #[error("missing return value from builtin {function:?}")]
    MissingReturnValue { function: BuiltinFunction },
    #[error("unknown variable `{name}`")]
    UnknownVariable { name: String },
}
