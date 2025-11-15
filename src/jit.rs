use crate::ast::{BinaryOp, UnaryOp};
use crate::builtins;
use crate::eval::{QualifiedName, RuntimeContext, Value as RuntimeValue};
use crate::ir::{BuiltinFunction, FunctionRef, IrExpr, IrProgram, IrStatement};
use cranelift::prelude::*;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{FuncId, Linkage, Module};
use std::collections::HashMap;
use std::{slice, str};
use thiserror::Error;

#[repr(C)]
pub struct RuntimeSlot {
    ptr: *const u8,
    len: usize,
}

pub struct CompiledExpression {
    module: JITModule,
    func_id: FuncId,
    _slot_data: Vec<Box<[u8]>>,
    slots: Vec<RuntimeSlot>,
}

impl CompiledExpression {
    pub fn evaluate(&self, ctx: &mut RuntimeContext) -> Result<f64, JitError> {
        let func = unsafe {
            let raw = self.module.get_finalized_function(self.func_id);
            std::mem::transmute::<
                *const u8,
                extern "C" fn(*mut RuntimeContext, *const RuntimeSlot) -> f64,
            >(raw)
        };
        Ok(func(ctx, self.slots.as_ptr()))
    }
}

pub fn compile_expression(expr: &IrExpr) -> Result<CompiledExpression, JitError> {
    let mut builder = JITBuilder::new(cranelift_module::default_libcall_names())?;
    register_builtin_symbols(&mut builder);
    register_runtime_symbols(&mut builder);
    let mut module = JITModule::new(builder);
    let mut ctx = module.make_context();
    let pointer_type = module.target_config().pointer_type();
    ctx.func.signature.params.push(AbiParam::new(pointer_type));
    ctx.func.signature.params.push(AbiParam::new(pointer_type));
    ctx.func.signature.returns.push(AbiParam::new(types::F64));

    let mut func_ctx = FunctionBuilderContext::new();
    let slot_names = {
        let mut builder = FunctionBuilder::new(&mut ctx.func, &mut func_ctx);
        let entry = builder.create_block();
        builder.append_block_params_for_function_params(entry);
        builder.switch_to_block(entry);
        builder.seal_block(entry);

        let runtime_ptr = builder.block_params(entry)[0];
        let slots_ptr = builder.block_params(entry)[1];
        let runtime_helpers = RuntimeHelpers::declare(&mut module)?;
        let mut translator = Translator::new(
            &mut builder,
            &mut module,
            runtime_ptr,
            slots_ptr,
            runtime_helpers,
        );
        let value = translator.translate(expr)?;
        let slots = translator.finish_expression(value);
        builder.finalize();
        slots
    };

    let func_id = module.declare_function("molang_expr", Linkage::Export, &ctx.func.signature)?;
    module.define_function(func_id, &mut ctx)?;
    module.clear_context(&mut ctx);
    module.finalize_definitions()?;

    let mut slot_data = Vec::with_capacity(slot_names.len());
    let mut slots = Vec::with_capacity(slot_names.len());
    for name in slot_names {
        let canonical = name.to_string();
        let bytes = canonical.into_bytes().into_boxed_slice();
        let len = bytes.len();
        slot_data.push(bytes);
        let stored_ptr = slot_data.last().unwrap().as_ptr();
        slots.push(RuntimeSlot {
            ptr: stored_ptr,
            len,
        });
    }

    Ok(CompiledExpression {
        module,
        func_id,
        _slot_data: slot_data,
        slots,
    })
}

pub fn compile_program(program: &IrProgram) -> Result<CompiledExpression, JitError> {
    let mut builder = JITBuilder::new(cranelift_module::default_libcall_names())?;
    register_builtin_symbols(&mut builder);
    register_runtime_symbols(&mut builder);
    let mut module = JITModule::new(builder);
    let mut ctx = module.make_context();
    let pointer_type = module.target_config().pointer_type();
    ctx.func.signature.params.push(AbiParam::new(pointer_type));
    ctx.func.signature.params.push(AbiParam::new(pointer_type));
    ctx.func.signature.returns.push(AbiParam::new(types::F64));

    let mut func_ctx = FunctionBuilderContext::new();
    let slot_names = {
        let mut builder = FunctionBuilder::new(&mut ctx.func, &mut func_ctx);
        let entry = builder.create_block();
        builder.append_block_params_for_function_params(entry);
        builder.switch_to_block(entry);
        builder.seal_block(entry);

        let runtime_ptr = builder.block_params(entry)[0];
        let slots_ptr = builder.block_params(entry)[1];
        let runtime_helpers = RuntimeHelpers::declare(&mut module)?;
        let translator = Translator::new(
            &mut builder,
            &mut module,
            runtime_ptr,
            slots_ptr,
            runtime_helpers,
        );
        let slots = translator.translate_program(program)?;
        builder.finalize();
        slots
    };

    let func_id = module.declare_function("molang_prog", Linkage::Export, &ctx.func.signature)?;
    module.define_function(func_id, &mut ctx)?;
    module.clear_context(&mut ctx);
    module.finalize_definitions()?;

    let mut slot_data = Vec::with_capacity(slot_names.len());
    let mut slots = Vec::with_capacity(slot_names.len());
    for name in slot_names {
        let canonical = name.to_string();
        let bytes = canonical.into_bytes().into_boxed_slice();
        let len = bytes.len();
        slot_data.push(bytes);
        let stored_ptr = slot_data.last().unwrap().as_ptr();
        slots.push(RuntimeSlot {
            ptr: stored_ptr,
            len,
        });
    }

    Ok(CompiledExpression {
        module,
        func_id,
        _slot_data: slot_data,
        slots,
    })
}

struct Translator<'a, 'b> {
    builder: &'a mut FunctionBuilder<'b>,
    module: &'a mut JITModule,
    runtime_ptr: Value,
    slots_ptr: Value,
    pointer_type: Type,
    pointer_bytes: i32,
    slot_names: Vec<QualifiedName>,
    slot_map: HashMap<QualifiedName, usize>,
    builtin_funcs: HashMap<BuiltinFunction, FuncId>,
    runtime_helpers: RuntimeHelpers,
    exit_block: Block,
    return_var: Variable,
}

impl<'a, 'b> Translator<'a, 'b> {
    fn new(
        builder: &'a mut FunctionBuilder<'b>,
        module: &'a mut JITModule,
        runtime_ptr: Value,
        slots_ptr: Value,
        runtime_helpers: RuntimeHelpers,
    ) -> Self {
        let pointer_type = module.target_config().pointer_type();
        let pointer_bytes = (module.target_config().pointer_bits() / 8) as i32;
        let exit_block = builder.create_block();
        let return_var = Variable::new(0);
        builder.declare_var(return_var, types::F64);
        let zero = builder.ins().f64const(Ieee64::with_float(0.0));
        builder.def_var(return_var, zero);
        Self {
            builder,
            module,
            runtime_ptr,
            slots_ptr,
            pointer_type,
            pointer_bytes,
            slot_names: Vec::new(),
            slot_map: HashMap::new(),
            builtin_funcs: HashMap::new(),
            runtime_helpers,
            exit_block,
            return_var,
        }
    }

    fn translate(&mut self, expr: &IrExpr) -> Result<Value, JitError> {
        match expr {
            IrExpr::Constant(value) => Ok(self.builder.ins().f64const(Ieee64::with_float(*value))),
            IrExpr::Path(parts) => self.load_variable(parts),
            IrExpr::String(_) => Err(JitError::UnsupportedExpression {
                feature: "string literal",
            }),
            IrExpr::Array(_) => Err(JitError::UnsupportedExpression {
                feature: "array literal",
            }),
            IrExpr::Struct(_) => Err(JitError::UnsupportedExpression {
                feature: "struct literal",
            }),
            IrExpr::Index { .. } => Err(JitError::UnsupportedExpression {
                feature: "indexing",
            }),
            IrExpr::Flow(_) => Err(JitError::UnsupportedExpression {
                feature: "control flow expression",
            }),
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
    fn finish_expression(self, result: Value) -> Vec<QualifiedName> {
        self.builder.ins().return_(&[result]);
        self.slot_names
    }

    fn translate_program(mut self, program: &IrProgram) -> Result<Vec<QualifiedName>, JitError> {
        for statement in &program.statements {
            self.translate_statement(statement)?;
        }
        if let Some(current) = self.builder.current_block() {
            if current != self.exit_block {
                self.builder.ins().jump(self.exit_block, &[]);
            }
        }
        self.builder.switch_to_block(self.exit_block);
        self.builder.seal_block(self.exit_block);
        let ret_val = self.builder.use_var(self.return_var);
        self.builder.ins().return_(&[ret_val]);
        Ok(self.slot_names)
    }

    fn translate_statement(&mut self, statement: &IrStatement) -> Result<(), JitError> {
        match statement {
            IrStatement::Assign { target, value } => {
                if let IrExpr::Path(source) = value {
                    self.copy_assignment(target, source)?;
                } else {
                    let val = self.translate(value)?;
                    self.store_number(target, val)?;
                }
            }
            IrStatement::Expr(expr) => {
                let _ = self.translate(expr)?;
            }
            IrStatement::Block(statements) => {
                for stmt in statements {
                    self.translate_statement(stmt)?;
                }
            }
            IrStatement::Return(expr) => {
                let value = match expr {
                    Some(expr) => self.translate(expr)?,
                    None => self.const_f64(0.0),
                };
                self.builder.def_var(self.return_var, value);
                self.builder.ins().jump(self.exit_block, &[]);
                let next = self.builder.create_block();
                self.builder.switch_to_block(next);
                self.builder.seal_block(next);
            }
            IrStatement::Loop { .. } => {
                return Err(JitError::UnsupportedStatement { feature: "loop" })
            }
            IrStatement::ForEach { .. } => {
                return Err(JitError::UnsupportedStatement {
                    feature: "for_each",
                })
            }
        }
        Ok(())
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
        let slot = self.ensure_slot(&name);
        let (ptr, len_value) = self.slot_pointer_components(slot);
        let func_ref = self
            .module
            .declare_func_in_func(self.runtime_helpers.get_number, self.builder.func);
        let call = self
            .builder
            .ins()
            .call(func_ref, &[self.runtime_ptr, ptr, len_value]);
        let results = self.builder.inst_results(call);
        Ok(results[0])
    }

    fn store_number(&mut self, parts: &[String], value: Value) -> Result<(), JitError> {
        let name = QualifiedName::from_parts(parts);
        let slot = self.ensure_slot(&name);
        let (ptr, len_value) = self.slot_pointer_components(slot);
        let func_ref = self
            .module
            .declare_func_in_func(self.runtime_helpers.set_number, self.builder.func);
        self.builder
            .ins()
            .call(func_ref, &[self.runtime_ptr, ptr, len_value, value]);
        Ok(())
    }

    fn copy_assignment(&mut self, target: &[String], source: &[String]) -> Result<(), JitError> {
        let dest_slot = self.ensure_slot_from_parts(target);
        let src_slot = self.ensure_slot_from_parts(source);
        self.clear_slot(dest_slot);
        self.copy_slot_value(dest_slot, src_slot);
        Ok(())
    }

    fn ensure_slot(&mut self, name: &QualifiedName) -> usize {
        if let Some(index) = self.slot_map.get(name) {
            *index
        } else {
            let index = self.slot_names.len();
            self.slot_names.push(name.clone());
            self.slot_map.insert(name.clone(), index);
            index
        }
    }

    fn ensure_slot_from_parts(&mut self, parts: &[String]) -> usize {
        let name = QualifiedName::from_parts(parts);
        self.ensure_slot(&name)
    }

    fn slot_pointer_components(&mut self, slot: usize) -> (Value, Value) {
        let entry_size = self.pointer_bytes * 2;
        let base_offset = slot as i32 * entry_size;
        let ptr = self.builder.ins().load(
            self.pointer_type,
            MemFlags::new(),
            self.slots_ptr,
            base_offset,
        );
        let len_value = self.builder.ins().load(
            self.pointer_type,
            MemFlags::new(),
            self.slots_ptr,
            base_offset + self.pointer_bytes,
        );
        (ptr, len_value)
    }

    fn copy_slot_value(&mut self, dest_slot: usize, src_slot: usize) {
        let (dest_ptr, dest_len) = self.slot_pointer_components(dest_slot);
        let (src_ptr, src_len) = self.slot_pointer_components(src_slot);
        let func_ref = self
            .module
            .declare_func_in_func(self.runtime_helpers.copy_value, self.builder.func);
        self.builder.ins().call(
            func_ref,
            &[self.runtime_ptr, dest_ptr, dest_len, src_ptr, src_len],
        );
    }

    fn clear_slot(&mut self, slot: usize) {
        let (ptr, len_value) = self.slot_pointer_components(slot);
        let func_ref = self
            .module
            .declare_func_in_func(self.runtime_helpers.clear_value, self.builder.func);
        self.builder
            .ins()
            .call(func_ref, &[self.runtime_ptr, ptr, len_value]);
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

fn register_runtime_symbols(builder: &mut JITBuilder) {
    builder.symbol("molang_rt_get_number", molang_rt_get_number as *const u8);
    builder.symbol("molang_rt_set_number", molang_rt_set_number as *const u8);
    builder.symbol("molang_rt_clear_value", molang_rt_clear_value as *const u8);
    builder.symbol("molang_rt_copy_value", molang_rt_copy_value as *const u8);
    builder.symbol(
        "molang_rt_array_push_number",
        molang_rt_array_push_number as *const u8,
    );
    builder.symbol(
        "molang_rt_array_push_string",
        molang_rt_array_push_string as *const u8,
    );
    builder.symbol(
        "molang_rt_array_get_number",
        molang_rt_array_get_number as *const u8,
    );
    builder.symbol(
        "molang_rt_array_length",
        molang_rt_array_length as *const u8,
    );
    builder.symbol(
        "molang_rt_array_copy_element",
        molang_rt_array_copy_element as *const u8,
    );
    builder.symbol("molang_rt_set_string", molang_rt_set_string as *const u8);
}

#[derive(Clone, Copy)]
struct RuntimeHelpers {
    get_number: FuncId,
    set_number: FuncId,
    clear_value: FuncId,
    copy_value: FuncId,
    array_push_number: FuncId,
    array_push_string: FuncId,
    array_get_number: FuncId,
    array_length: FuncId,
    array_copy_element: FuncId,
    set_string: FuncId,
}

impl RuntimeHelpers {
    fn declare(module: &mut JITModule) -> Result<Self, JitError> {
        let pointer_type = module.target_config().pointer_type();
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(pointer_type));
        sig.params.push(AbiParam::new(pointer_type));
        sig.params.push(AbiParam::new(pointer_type));
        sig.returns.push(AbiParam::new(types::F64));
        let get_number = module.declare_function("molang_rt_get_number", Linkage::Import, &sig)?;

        let mut set_sig = module.make_signature();
        set_sig.params.push(AbiParam::new(pointer_type));
        set_sig.params.push(AbiParam::new(pointer_type));
        set_sig.params.push(AbiParam::new(pointer_type));
        set_sig.params.push(AbiParam::new(types::F64));
        let set_number =
            module.declare_function("molang_rt_set_number", Linkage::Import, &set_sig)?;

        let mut clear_sig = module.make_signature();
        clear_sig.params.push(AbiParam::new(pointer_type));
        clear_sig.params.push(AbiParam::new(pointer_type));
        clear_sig.params.push(AbiParam::new(pointer_type));
        let clear_value =
            module.declare_function("molang_rt_clear_value", Linkage::Import, &clear_sig)?;

        let mut copy_sig = module.make_signature();
        copy_sig.params.push(AbiParam::new(pointer_type));
        copy_sig.params.push(AbiParam::new(pointer_type));
        copy_sig.params.push(AbiParam::new(pointer_type));
        copy_sig.params.push(AbiParam::new(pointer_type));
        copy_sig.params.push(AbiParam::new(pointer_type));
        let copy_value =
            module.declare_function("molang_rt_copy_value", Linkage::Import, &copy_sig)?;

        let mut array_push_sig = module.make_signature();
        array_push_sig.params.push(AbiParam::new(pointer_type));
        array_push_sig.params.push(AbiParam::new(pointer_type));
        array_push_sig.params.push(AbiParam::new(pointer_type));
        array_push_sig.params.push(AbiParam::new(types::F64));
        let array_push_number = module.declare_function(
            "molang_rt_array_push_number",
            Linkage::Import,
            &array_push_sig,
        )?;

        let mut array_push_str_sig = module.make_signature();
        array_push_str_sig.params.push(AbiParam::new(pointer_type));
        array_push_str_sig.params.push(AbiParam::new(pointer_type));
        array_push_str_sig.params.push(AbiParam::new(pointer_type));
        array_push_str_sig.params.push(AbiParam::new(pointer_type));
        array_push_str_sig.params.push(AbiParam::new(pointer_type));
        let array_push_string = module.declare_function(
            "molang_rt_array_push_string",
            Linkage::Import,
            &array_push_str_sig,
        )?;

        let mut array_get_sig = module.make_signature();
        array_get_sig.params.push(AbiParam::new(pointer_type));
        array_get_sig.params.push(AbiParam::new(pointer_type));
        array_get_sig.params.push(AbiParam::new(pointer_type));
        array_get_sig.params.push(AbiParam::new(types::F64));
        array_get_sig.returns.push(AbiParam::new(types::F64));
        let array_get_number = module.declare_function(
            "molang_rt_array_get_number",
            Linkage::Import,
            &array_get_sig,
        )?;

        let mut array_len_sig = module.make_signature();
        array_len_sig.params.push(AbiParam::new(pointer_type));
        array_len_sig.params.push(AbiParam::new(pointer_type));
        array_len_sig.params.push(AbiParam::new(pointer_type));
        array_len_sig.returns.push(AbiParam::new(types::I64));
        let array_length =
            module.declare_function("molang_rt_array_length", Linkage::Import, &array_len_sig)?;

        let mut array_copy_sig = module.make_signature();
        array_copy_sig.params.push(AbiParam::new(pointer_type));
        array_copy_sig.params.push(AbiParam::new(pointer_type));
        array_copy_sig.params.push(AbiParam::new(pointer_type));
        array_copy_sig.params.push(AbiParam::new(types::I64));
        array_copy_sig.params.push(AbiParam::new(pointer_type));
        array_copy_sig.params.push(AbiParam::new(pointer_type));
        let array_copy_element = module.declare_function(
            "molang_rt_array_copy_element",
            Linkage::Import,
            &array_copy_sig,
        )?;

        let mut set_string_sig = module.make_signature();
        set_string_sig.params.push(AbiParam::new(pointer_type));
        set_string_sig.params.push(AbiParam::new(pointer_type));
        set_string_sig.params.push(AbiParam::new(pointer_type));
        set_string_sig.params.push(AbiParam::new(pointer_type));
        set_string_sig.params.push(AbiParam::new(pointer_type));
        let set_string =
            module.declare_function("molang_rt_set_string", Linkage::Import, &set_string_sig)?;

        Ok(RuntimeHelpers {
            get_number,
            set_number,
            clear_value,
            copy_value,
            array_push_number,
            array_push_string,
            array_get_number,
            array_length,
            array_copy_element,
            set_string,
        })
    }
}

#[no_mangle]
pub extern "C" fn molang_rt_get_number(
    ctx: *mut RuntimeContext,
    name_ptr: *const u8,
    len: usize,
) -> f64 {
    if ctx.is_null() || name_ptr.is_null() {
        return 0.0;
    }
    let bytes = unsafe { slice::from_raw_parts(name_ptr, len) };
    let canonical = match str::from_utf8(bytes) {
        Ok(s) => s,
        Err(_) => return 0.0,
    };
    let runtime = unsafe { &mut *ctx };
    runtime.get_number_canonical(canonical).unwrap_or(0.0)
}

#[no_mangle]
pub extern "C" fn molang_rt_set_number(
    ctx: *mut RuntimeContext,
    name_ptr: *const u8,
    len: usize,
    value: f64,
) {
    if ctx.is_null() || name_ptr.is_null() {
        return;
    }
    let bytes = unsafe { slice::from_raw_parts(name_ptr, len) };
    let canonical = match str::from_utf8(bytes) {
        Ok(s) => s,
        Err(_) => return,
    };
    let runtime = unsafe { &mut *ctx };
    runtime.set_number_canonical(canonical, value);
}

#[no_mangle]
pub extern "C" fn molang_rt_clear_value(ctx: *mut RuntimeContext, name_ptr: *const u8, len: usize) {
    if ctx.is_null() || name_ptr.is_null() {
        return;
    }
    let bytes = unsafe { slice::from_raw_parts(name_ptr, len) };
    if let Ok(canonical) = str::from_utf8(bytes) {
        let runtime = unsafe { &mut *ctx };
        runtime.clear_value_canonical(canonical);
    }
}

#[no_mangle]
pub extern "C" fn molang_rt_copy_value(
    ctx: *mut RuntimeContext,
    dest_ptr: *const u8,
    dest_len: usize,
    src_ptr: *const u8,
    src_len: usize,
) {
    if ctx.is_null() || dest_ptr.is_null() || src_ptr.is_null() {
        return;
    }
    let dest_bytes = unsafe { slice::from_raw_parts(dest_ptr, dest_len) };
    let src_bytes = unsafe { slice::from_raw_parts(src_ptr, src_len) };
    if let (Ok(dest), Ok(src)) = (str::from_utf8(dest_bytes), str::from_utf8(src_bytes)) {
        let runtime = unsafe { &mut *ctx };
        runtime.copy_value_canonical(dest, src);
    }
}

#[no_mangle]
pub extern "C" fn molang_rt_array_push_number(
    ctx: *mut RuntimeContext,
    name_ptr: *const u8,
    len: usize,
    value: f64,
) {
    if ctx.is_null() || name_ptr.is_null() {
        return;
    }
    let bytes = unsafe { slice::from_raw_parts(name_ptr, len) };
    if let Ok(canonical) = str::from_utf8(bytes) {
        let runtime = unsafe { &mut *ctx };
        runtime.array_push_number_canonical(canonical, value);
    }
}

#[no_mangle]
pub extern "C" fn molang_rt_array_push_string(
    ctx: *mut RuntimeContext,
    name_ptr: *const u8,
    len: usize,
    value_ptr: *const u8,
    value_len: usize,
) {
    if ctx.is_null() || name_ptr.is_null() || value_ptr.is_null() {
        return;
    }
    let name_bytes = unsafe { slice::from_raw_parts(name_ptr, len) };
    let value_bytes = unsafe { slice::from_raw_parts(value_ptr, value_len) };
    if let (Ok(canonical), Ok(value)) = (str::from_utf8(name_bytes), str::from_utf8(value_bytes)) {
        let runtime = unsafe { &mut *ctx };
        runtime.array_push_string_canonical(canonical, value);
    }
}

#[no_mangle]
pub extern "C" fn molang_rt_array_get_number(
    ctx: *mut RuntimeContext,
    name_ptr: *const u8,
    len: usize,
    index: f64,
) -> f64 {
    if ctx.is_null() || name_ptr.is_null() {
        return 0.0;
    }
    let bytes = unsafe { slice::from_raw_parts(name_ptr, len) };
    if let Ok(canonical) = str::from_utf8(bytes) {
        let runtime = unsafe { &mut *ctx };
        return runtime.array_get_number_canonical(canonical, index);
    }
    0.0
}

#[no_mangle]
pub extern "C" fn molang_rt_array_length(
    ctx: *mut RuntimeContext,
    name_ptr: *const u8,
    len: usize,
) -> i64 {
    if ctx.is_null() || name_ptr.is_null() {
        return 0;
    }
    let bytes = unsafe { slice::from_raw_parts(name_ptr, len) };
    if let Ok(canonical) = str::from_utf8(bytes) {
        let runtime = unsafe { &mut *ctx };
        return runtime.array_length_canonical(canonical);
    }
    0
}

#[no_mangle]
pub extern "C" fn molang_rt_array_copy_element(
    ctx: *mut RuntimeContext,
    array_ptr: *const u8,
    array_len: usize,
    index: i64,
    dest_ptr: *const u8,
    dest_len: usize,
) {
    if ctx.is_null() || array_ptr.is_null() || dest_ptr.is_null() {
        return;
    }
    let arr_bytes = unsafe { slice::from_raw_parts(array_ptr, array_len) };
    let dest_bytes = unsafe { slice::from_raw_parts(dest_ptr, dest_len) };
    if let (Ok(array_name), Ok(dest_name)) = (str::from_utf8(arr_bytes), str::from_utf8(dest_bytes))
    {
        let runtime = unsafe { &mut *ctx };
        runtime.array_copy_element_canonical(array_name, index, dest_name);
    }
}

#[no_mangle]
pub extern "C" fn molang_rt_set_string(
    ctx: *mut RuntimeContext,
    name_ptr: *const u8,
    name_len: usize,
    value_ptr: *const u8,
    value_len: usize,
) {
    if ctx.is_null() || name_ptr.is_null() || value_ptr.is_null() {
        return;
    }
    let name_bytes = unsafe { slice::from_raw_parts(name_ptr, name_len) };
    let value_bytes = unsafe { slice::from_raw_parts(value_ptr, value_len) };
    if let (Ok(name), Ok(value)) = (str::from_utf8(name_bytes), str::from_utf8(value_bytes)) {
        let runtime = unsafe { &mut *ctx };
        runtime.set_value_canonical(name, RuntimeValue::string(value));
    }
}

#[derive(Debug, Error)]
pub enum JitError {
    #[error(transparent)]
    Module(#[from] cranelift_module::ModuleError),
    #[error("missing return value from builtin {function:?}")]
    MissingReturnValue { function: BuiltinFunction },
    #[error("unknown variable `{name}`")]
    UnknownVariable { name: String },
    #[error("statement `{feature}` is not supported by the JIT yet")]
    UnsupportedStatement { feature: &'static str },
    #[error("expression `{feature}` is not supported by the JIT yet")]
    UnsupportedExpression { feature: &'static str },
}
