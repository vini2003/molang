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

struct LoopContext {
    break_block: Block,
    continue_block: Block,
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
    loop_stack: Vec<LoopContext>,
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
            loop_stack: Vec::new(),
        }
    }

    /// Assigns an expression to a target variable, handling complex value types
    /// like strings, arrays, and structs.
    fn assign_expression(&mut self, target: &[String], expr: &IrExpr) -> Result<(), JitError> {
        match expr {
            // Numeric constant or computed value - evaluate and store
            IrExpr::Constant(_)
            | IrExpr::Path(_)
            | IrExpr::Unary { .. }
            | IrExpr::Binary { .. }
            | IrExpr::Conditional { .. }
            | IrExpr::Call { .. } => {
                let value = self.translate(expr)?;
                self.store_number(target, value)?;
            }

            // String literal - use molang_rt_set_string
            IrExpr::String(text) => {
                let target_slot = self.ensure_slot_from_parts(target);
                let (target_ptr, target_len) = self.slot_pointer_components(target_slot);

                // Store the string bytes as constants in memory
                let string_bytes = text.as_bytes();
                let string_len = string_bytes.len();
                let string_len_value = self.builder.ins().iconst(self.pointer_type, string_len as i64);

                // We need to emit the string data. For now, we'll use a simpler approach:
                // allocate it as a global data object in the module
                let data_id = self
                    .module
                    .declare_anonymous_data(false, false)
                    .map_err(|e| JitError::Module(e))?;
                let mut data_desc = cranelift_module::DataDescription::new();
                data_desc.define(string_bytes.to_vec().into_boxed_slice());
                self.module.define_data(data_id, &data_desc)?;
                let data_ref = self.module.declare_data_in_func(data_id, self.builder.func);
                let string_ptr = self.builder.ins().global_value(self.pointer_type, data_ref);

                let func_ref = self
                    .module
                    .declare_func_in_func(self.runtime_helpers.set_string, self.builder.func);
                self.builder.ins().call(
                    func_ref,
                    &[self.runtime_ptr, target_ptr, target_len, string_ptr, string_len_value],
                );
            }

            // Array literal - allocate temp slot, clear, push elements
            IrExpr::Array(elements) => {
                let target_slot = self.ensure_slot_from_parts(target);
                self.clear_slot(target_slot);

                // Push each element
                for element in elements {
                    match element {
                        IrExpr::Constant(_)
                        | IrExpr::Path(_)
                        | IrExpr::Unary { .. }
                        | IrExpr::Binary { .. }
                        | IrExpr::Conditional { .. }
                        | IrExpr::Call { .. } => {
                            // Numeric element
                            let value = self.translate(element)?;
                            let (ptr, len) = self.slot_pointer_components(target_slot);
                            let func_ref = self.module.declare_func_in_func(
                                self.runtime_helpers.array_push_number,
                                self.builder.func,
                            );
                            self.builder
                                .ins()
                                .call(func_ref, &[self.runtime_ptr, ptr, len, value]);
                        }
                        IrExpr::String(text) => {
                            // String element
                            let string_bytes = text.as_bytes();
                            let string_len = string_bytes.len();
                            let string_len_value =
                                self.builder.ins().iconst(self.pointer_type, string_len as i64);

                            let data_id = self
                                .module
                                .declare_anonymous_data(false, false)
                                .map_err(|e| JitError::Module(e))?;
                            let mut data_desc = cranelift_module::DataDescription::new();
                            data_desc.define(string_bytes.to_vec().into_boxed_slice());
                            self.module.define_data(data_id, &data_desc)?;
                            let data_ref = self.module.declare_data_in_func(data_id, self.builder.func);
                            let string_ptr = self.builder.ins().global_value(self.pointer_type, data_ref);

                            let (array_ptr, array_len) = self.slot_pointer_components(target_slot);
                            let func_ref = self.module.declare_func_in_func(
                                self.runtime_helpers.array_push_string,
                                self.builder.func,
                            );
                            self.builder.ins().call(
                                func_ref,
                                &[self.runtime_ptr, array_ptr, array_len, string_ptr, string_len_value],
                            );
                        }
                        _ => {
                            // For complex elements (arrays, structs), create a temp variable
                            // and push by copying from the temp
                            let temp_name = format!("__temp_array_elem_{}", self.slot_names.len());
                            let temp_parts = vec![temp_name];
                            self.assign_expression(&temp_parts, element)?;
                            // Array of arrays/structs isn't directly supported,
                            // but we'll leave this for future enhancement
                        }
                    }
                }
            }

            // Struct literal - synthesize temp slots per field, then copy to target
            IrExpr::Struct(fields) => {
                let target_slot = self.ensure_slot_from_parts(target);
                self.clear_slot(target_slot);

                // For each field in insertion order, assign to target.field
                for (field_name, field_expr) in fields.iter() {
                    let mut field_path = target.to_vec();
                    field_path.push(field_name.clone());
                    self.assign_expression(&field_path, field_expr)?;
                }
            }

            // Index expression - handled specially
            IrExpr::Index { .. } => {
                return Err(JitError::UnsupportedExpression {
                    feature: "index as assignment source",
                });
            }

            // Flow expressions can't be assigned
            IrExpr::Flow(_) => {
                return Err(JitError::UnsupportedExpression {
                    feature: "control flow expression as assignment source",
                });
            }
        }
        Ok(())
    }

    fn translate(&mut self, expr: &IrExpr) -> Result<Value, JitError> {
        match expr {
            IrExpr::Constant(value) => Ok(self.builder.ins().f64const(Ieee64::with_float(*value))),
            IrExpr::Path(parts) => self.load_variable(parts),
            IrExpr::String(_) => {
                // String literals can't be used as values directly; they must be assigned
                Err(JitError::UnsupportedExpression {
                    feature: "string literal as value expression",
                })
            }
            IrExpr::Array(elements) => {
                // When an array is used as a value expression, return its length
                // This is useful for cases like: return [1, 2, 3]; => 3.0
                let length = elements.len() as f64;
                Ok(self.const_f64(length))
            }
            IrExpr::Struct(_) => {
                // Struct literals can't be used as values directly; they must be assigned
                Err(JitError::UnsupportedExpression {
                    feature: "struct literal as value expression",
                })
            }
            IrExpr::Index { target, index } => {
                // Check if this is a .length access
                if let IrExpr::Path(base_parts) = target.as_ref() {
                    if let IrExpr::Path(index_parts) = index.as_ref() {
                        if index_parts.len() == 1 && index_parts[0] == "length" {
                            // This is array.length access
                            return self.load_array_length(base_parts);
                        }
                    }
                }

                // Otherwise, this is array indexing
                if let IrExpr::Path(array_path) = target.as_ref() {
                    let index_value = self.translate(index)?;
                    let array_name = QualifiedName::from_parts(array_path);
                    let array_slot = self.ensure_slot(&array_name);
                    let (array_ptr, array_len) = self.slot_pointer_components(array_slot);

                    let func_ref = self.module.declare_func_in_func(
                        self.runtime_helpers.array_get_number,
                        self.builder.func,
                    );
                    let call = self.builder.ins().call(
                        func_ref,
                        &[self.runtime_ptr, array_ptr, array_len, index_value],
                    );
                    let results = self.builder.inst_results(call);
                    Ok(results[0])
                } else {
                    Err(JitError::UnsupportedExpression {
                        feature: "indexing non-path expression",
                    })
                }
            }
            IrExpr::Flow(flow) => {
                use crate::ast::ControlFlowExpr;
                if let Some(ctx) = self.loop_stack.last() {
                    // Extract the target blocks before any mutable borrows
                    let target_block = match flow {
                        ControlFlowExpr::Break => ctx.break_block,
                        ControlFlowExpr::Continue => ctx.continue_block,
                    };

                    // Return a dummy value first (for use in the current block)
                    let dummy = self.const_f64(0.0);

                    // Jump to the target block
                    self.builder.ins().jump(target_block, &[]);

                    // Create a new unreachable block for any code after the break/continue
                    let next = self.builder.create_block();
                    self.builder.switch_to_block(next);
                    self.builder.seal_block(next);

                    Ok(dummy)
                } else {
                    Err(JitError::UnsupportedExpression {
                        feature: "break/continue outside loop",
                    })
                }
            }
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
                BinaryOp::Equal => self.emit_value_equality(left, right, true),
                BinaryOp::NotEqual => self.emit_value_equality(left, right, false),
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
                    self.assign_expression(target, value)?;
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
            IrStatement::Loop { count, body } => {
                // Evaluate the loop count
                let count_value = self.translate(count)?;

                // Create a variable to hold the current iteration index
                let loop_var = Variable::new(self.slot_names.len() + self.loop_stack.len() + 1);
                self.builder.declare_var(loop_var, types::F64);
                let zero = self.const_f64(0.0);
                self.builder.def_var(loop_var, zero);

                // Create loop blocks
                let loop_header = self.builder.create_block();
                let loop_body = self.builder.create_block();
                let loop_exit = self.builder.create_block();
                let loop_increment = self.builder.create_block();

                // Jump to header
                self.builder.ins().jump(loop_header, &[]);

                // Loop header: check condition
                self.builder.switch_to_block(loop_header);
                let current_index = self.builder.use_var(loop_var);
                let condition = self.builder.ins().fcmp(FloatCC::LessThan, current_index, count_value);
                self.builder.ins().brif(condition, loop_body, &[], loop_exit, &[]);

                // Loop body
                self.builder.switch_to_block(loop_body);

                // Push loop context for break/continue
                self.loop_stack.push(LoopContext {
                    break_block: loop_exit,
                    continue_block: loop_increment,
                });

                self.translate_statement(body)?;

                // Pop loop context
                self.loop_stack.pop();

                // If we're still in the loop body (no break/continue), jump to increment
                if self.builder.current_block().is_some() {
                    self.builder.ins().jump(loop_increment, &[]);
                }

                self.builder.seal_block(loop_body);

                // Loop increment block
                self.builder.switch_to_block(loop_increment);
                let current_index = self.builder.use_var(loop_var);
                let one = self.const_f64(1.0);
                let next_index = self.builder.ins().fadd(current_index, one);
                self.builder.def_var(loop_var, next_index);
                self.builder.ins().jump(loop_header, &[]);
                self.builder.seal_block(loop_increment);
                self.builder.seal_block(loop_header);

                // Continue execution after loop
                self.builder.switch_to_block(loop_exit);
                self.builder.seal_block(loop_exit);
            }
            IrStatement::ForEach { variable, collection, body } => {
                // Evaluate the collection expression
                // If it's a path, use it directly; otherwise assign to a temporary
                let collection_parts = match collection {
                    IrExpr::Path(parts) => parts.clone(),
                    _ => {
                        // For non-path collections, assign to a temporary
                        let collection_temp = format!("__temp_collection_{}", self.slot_names.len());
                        let temp_parts = vec![collection_temp.clone()];
                        self.assign_expression(&temp_parts, collection)?;
                        temp_parts
                    }
                };

                // Get the array length
                let array_length = self.load_array_length(&collection_parts)?;

                // Create a variable to hold the current iteration index
                let loop_var = Variable::new(self.slot_names.len() + self.loop_stack.len() + 1);
                self.builder.declare_var(loop_var, types::F64);
                let zero = self.const_f64(0.0);
                self.builder.def_var(loop_var, zero);

                // Create loop blocks
                let loop_header = self.builder.create_block();
                let loop_body = self.builder.create_block();
                let loop_exit = self.builder.create_block();
                let loop_increment = self.builder.create_block();

                // Jump to header
                self.builder.ins().jump(loop_header, &[]);

                // Loop header: check condition
                self.builder.switch_to_block(loop_header);
                let current_index = self.builder.use_var(loop_var);
                let condition = self.builder.ins().fcmp(FloatCC::LessThan, current_index, array_length);
                self.builder.ins().brif(condition, loop_body, &[], loop_exit, &[]);

                // Loop body
                self.builder.switch_to_block(loop_body);

                // Copy current element to the loop variable
                let current_index_f64 = self.builder.use_var(loop_var);
                let current_index_i64 = self.builder.ins().fcvt_to_sint(types::I64, current_index_f64);
                let collection_slot = self.ensure_slot_from_parts(&collection_parts);
                let (array_ptr, array_len) = self.slot_pointer_components(collection_slot);
                let dest_slot = self.ensure_slot_from_parts(variable);
                let (dest_ptr, dest_len) = self.slot_pointer_components(dest_slot);

                let func_ref = self.module.declare_func_in_func(
                    self.runtime_helpers.array_copy_element,
                    self.builder.func,
                );
                self.builder.ins().call(
                    func_ref,
                    &[self.runtime_ptr, array_ptr, array_len, current_index_i64, dest_ptr, dest_len],
                );

                // Push loop context for break/continue
                self.loop_stack.push(LoopContext {
                    break_block: loop_exit,
                    continue_block: loop_increment,
                });

                self.translate_statement(body)?;

                // Pop loop context
                self.loop_stack.pop();

                // If we're still in the loop body (no break/continue), jump to increment
                if self.builder.current_block().is_some() {
                    self.builder.ins().jump(loop_increment, &[]);
                }

                self.builder.seal_block(loop_body);

                // Loop increment block
                self.builder.switch_to_block(loop_increment);
                let current_index = self.builder.use_var(loop_var);
                let one = self.const_f64(1.0);
                let next_index = self.builder.ins().fadd(current_index, one);
                self.builder.def_var(loop_var, next_index);
                self.builder.ins().jump(loop_header, &[]);
                self.builder.seal_block(loop_increment);
                self.builder.seal_block(loop_header);

                // Continue execution after loop
                self.builder.switch_to_block(loop_exit);
                self.builder.seal_block(loop_exit);
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

    fn load_array_length(&mut self, parts: &[String]) -> Result<Value, JitError> {
        let name = QualifiedName::from_parts(parts);
        let slot = self.ensure_slot(&name);
        let (ptr, len_value) = self.slot_pointer_components(slot);
        let func_ref = self
            .module
            .declare_func_in_func(self.runtime_helpers.array_length, self.builder.func);
        let call = self
            .builder
            .ins()
            .call(func_ref, &[self.runtime_ptr, ptr, len_value]);
        let results = self.builder.inst_results(call);
        // Convert i64 to f64
        let i64_len = results[0];
        let f64_len = self.builder.ins().fcvt_from_sint(types::F64, i64_len);
        Ok(f64_len)
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

    fn emit_value_equality(
        &mut self,
        left: &IrExpr,
        right: &IrExpr,
        is_equal: bool,
    ) -> Result<Value, JitError> {
        // Check what we're comparing
        match (left, right) {
            // Path == Path: use runtime helper
            (IrExpr::Path(left_parts), IrExpr::Path(right_parts)) => {
                let left_slot = self.ensure_slot_from_parts(left_parts);
                let (left_ptr, left_len) = self.slot_pointer_components(left_slot);
                let right_slot = self.ensure_slot_from_parts(right_parts);
                let (right_ptr, right_len) = self.slot_pointer_components(right_slot);

                let func_id = if is_equal {
                    self.runtime_helpers.equal_paths
                } else {
                    self.runtime_helpers.not_equal_paths
                };

                let func_ref = self
                    .module
                    .declare_func_in_func(func_id, self.builder.func);

                let result = self.builder.ins().call(
                    func_ref,
                    &[self.runtime_ptr, left_ptr, left_len, right_ptr, right_len],
                );
                Ok(self.builder.inst_results(result)[0])
            }
            // Path == String: use path_string helper
            (IrExpr::Path(path_parts), IrExpr::String(text))
            | (IrExpr::String(text), IrExpr::Path(path_parts)) => {
                let path_slot = self.ensure_slot_from_parts(path_parts);
                let (path_ptr, path_len) = self.slot_pointer_components(path_slot);

                // Create global data for the string literal
                let string_bytes = text.as_bytes();
                let string_len = string_bytes.len();
                let data_id = self
                    .module
                    .declare_anonymous_data(false, false)
                    .map_err(cranelift_module::ModuleError::from)?;
                let mut data_desc = cranelift_module::DataDescription::new();
                data_desc.define(string_bytes.to_vec().into_boxed_slice());
                self.module
                    .define_data(data_id, &data_desc)
                    .map_err(cranelift_module::ModuleError::from)?;

                let global_value = self
                    .module
                    .declare_data_in_func(data_id, self.builder.func);
                let str_ptr = self.builder.ins().global_value(self.pointer_type, global_value);
                let str_len = self.builder.ins().iconst(self.pointer_type, string_len as i64);

                let func_id = if is_equal {
                    self.runtime_helpers.equal_path_string
                } else {
                    self.runtime_helpers.not_equal_path_string
                };

                let func_ref = self
                    .module
                    .declare_func_in_func(func_id, self.builder.func);

                let result = self.builder.ins().call(
                    func_ref,
                    &[self.runtime_ptr, path_ptr, path_len, str_ptr, str_len],
                );
                Ok(self.builder.inst_results(result)[0])
            }
            // String == String: compile-time comparison
            (IrExpr::String(left_str), IrExpr::String(right_str)) => {
                let result = if is_equal {
                    if left_str == right_str { 1.0 } else { 0.0 }
                } else {
                    if left_str != right_str { 1.0 } else { 0.0 }
                };
                Ok(self.const_f64(result))
            }
            // Numeric or other: fall back to float comparison
            _ => {
                let cond = if is_equal {
                    FloatCC::Equal
                } else {
                    FloatCC::NotEqual
                };
                self.emit_comparison(cond, left, right)
            }
        }
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
        "builtin_math_cos",
        builtins::builtin_math_cos as *const u8,
    );
    builder.symbol(
        "builtin_math_sin",
        builtins::builtin_math_sin as *const u8,
    );
    builder.symbol(
        "builtin_math_abs",
        builtins::builtin_math_abs as *const u8,
    );
    builder.symbol(
        "builtin_math_random",
        builtins::builtin_math_random as *const u8,
    );
    builder.symbol(
        "builtin_math_random_integer",
        builtins::builtin_math_random_integer as *const u8,
    );
    builder.symbol(
        "builtin_math_clamp",
        builtins::builtin_math_clamp as *const u8,
    );
    builder.symbol(
        "builtin_math_sqrt",
        builtins::builtin_math_sqrt as *const u8,
    );
    builder.symbol(
        "builtin_math_floor",
        builtins::builtin_math_floor as *const u8,
    );
    builder.symbol(
        "builtin_math_ceil",
        builtins::builtin_math_ceil as *const u8,
    );
    builder.symbol(
        "builtin_math_round",
        builtins::builtin_math_round as *const u8,
    );
    builder.symbol(
        "builtin_math_trunc",
        builtins::builtin_math_trunc as *const u8,
    );
    builder.symbol(
        "builtin_math_acos",
        builtins::builtin_math_acos as *const u8,
    );
    builder.symbol(
        "builtin_math_asin",
        builtins::builtin_math_asin as *const u8,
    );
    builder.symbol(
        "builtin_math_atan",
        builtins::builtin_math_atan as *const u8,
    );
    builder.symbol(
        "builtin_math_atan2",
        builtins::builtin_math_atan2 as *const u8,
    );
    builder.symbol(
        "builtin_math_exp",
        builtins::builtin_math_exp as *const u8,
    );
    builder.symbol("builtin_math_ln", builtins::builtin_math_ln as *const u8);
    builder.symbol(
        "builtin_math_pow",
        builtins::builtin_math_pow as *const u8,
    );
    builder.symbol(
        "builtin_math_max",
        builtins::builtin_math_max as *const u8,
    );
    builder.symbol(
        "builtin_math_min",
        builtins::builtin_math_min as *const u8,
    );
    builder.symbol(
        "builtin_math_mod",
        builtins::builtin_math_mod as *const u8,
    );
    builder.symbol(
        "builtin_math_sign",
        builtins::builtin_math_sign as *const u8,
    );
    builder.symbol(
        "builtin_math_copy_sign",
        builtins::builtin_math_copy_sign as *const u8,
    );
    builder.symbol("builtin_math_pi", builtins::builtin_math_pi as *const u8);
    builder.symbol(
        "builtin_math_min_angle",
        builtins::builtin_math_min_angle as *const u8,
    );
    builder.symbol(
        "builtin_math_lerp",
        builtins::builtin_math_lerp as *const u8,
    );
    builder.symbol(
        "builtin_math_inverse_lerp",
        builtins::builtin_math_inverse_lerp as *const u8,
    );
    builder.symbol(
        "builtin_math_lerprotate",
        builtins::builtin_math_lerprotate as *const u8,
    );
    builder.symbol(
        "builtin_math_hermite_blend",
        builtins::builtin_math_hermite_blend as *const u8,
    );
    builder.symbol(
        "builtin_math_die_roll",
        builtins::builtin_math_die_roll as *const u8,
    );
    builder.symbol(
        "builtin_math_die_roll_integer",
        builtins::builtin_math_die_roll_integer as *const u8,
    );
    builder.symbol(
        "builtin_math_ease_in_quad",
        builtins::builtin_math_ease_in_quad as *const u8,
    );
    builder.symbol(
        "builtin_math_ease_out_quad",
        builtins::builtin_math_ease_out_quad as *const u8,
    );
    builder.symbol(
        "builtin_math_ease_in_out_quad",
        builtins::builtin_math_ease_in_out_quad as *const u8,
    );
    builder.symbol(
        "builtin_math_ease_in_cubic",
        builtins::builtin_math_ease_in_cubic as *const u8,
    );
    builder.symbol(
        "builtin_math_ease_out_cubic",
        builtins::builtin_math_ease_out_cubic as *const u8,
    );
    builder.symbol(
        "builtin_math_ease_in_out_cubic",
        builtins::builtin_math_ease_in_out_cubic as *const u8,
    );
    builder.symbol(
        "builtin_math_ease_in_quart",
        builtins::builtin_math_ease_in_quart as *const u8,
    );
    builder.symbol(
        "builtin_math_ease_out_quart",
        builtins::builtin_math_ease_out_quart as *const u8,
    );
    builder.symbol(
        "builtin_math_ease_in_out_quart",
        builtins::builtin_math_ease_in_out_quart as *const u8,
    );
    builder.symbol(
        "builtin_math_ease_in_quint",
        builtins::builtin_math_ease_in_quint as *const u8,
    );
    builder.symbol(
        "builtin_math_ease_out_quint",
        builtins::builtin_math_ease_out_quint as *const u8,
    );
    builder.symbol(
        "builtin_math_ease_in_out_quint",
        builtins::builtin_math_ease_in_out_quint as *const u8,
    );
    builder.symbol(
        "builtin_math_ease_in_sine",
        builtins::builtin_math_ease_in_sine as *const u8,
    );
    builder.symbol(
        "builtin_math_ease_out_sine",
        builtins::builtin_math_ease_out_sine as *const u8,
    );
    builder.symbol(
        "builtin_math_ease_in_out_sine",
        builtins::builtin_math_ease_in_out_sine as *const u8,
    );
    builder.symbol(
        "builtin_math_ease_in_expo",
        builtins::builtin_math_ease_in_expo as *const u8,
    );
    builder.symbol(
        "builtin_math_ease_out_expo",
        builtins::builtin_math_ease_out_expo as *const u8,
    );
    builder.symbol(
        "builtin_math_ease_in_out_expo",
        builtins::builtin_math_ease_in_out_expo as *const u8,
    );
    builder.symbol(
        "builtin_math_ease_in_circ",
        builtins::builtin_math_ease_in_circ as *const u8,
    );
    builder.symbol(
        "builtin_math_ease_out_circ",
        builtins::builtin_math_ease_out_circ as *const u8,
    );
    builder.symbol(
        "builtin_math_ease_in_out_circ",
        builtins::builtin_math_ease_in_out_circ as *const u8,
    );
    builder.symbol(
        "builtin_math_ease_in_back",
        builtins::builtin_math_ease_in_back as *const u8,
    );
    builder.symbol(
        "builtin_math_ease_out_back",
        builtins::builtin_math_ease_out_back as *const u8,
    );
    builder.symbol(
        "builtin_math_ease_in_out_back",
        builtins::builtin_math_ease_in_out_back as *const u8,
    );
    builder.symbol(
        "builtin_math_ease_in_elastic",
        builtins::builtin_math_ease_in_elastic as *const u8,
    );
    builder.symbol(
        "builtin_math_ease_out_elastic",
        builtins::builtin_math_ease_out_elastic as *const u8,
    );
    builder.symbol(
        "builtin_math_ease_in_out_elastic",
        builtins::builtin_math_ease_in_out_elastic as *const u8,
    );
    builder.symbol(
        "builtin_math_ease_in_bounce",
        builtins::builtin_math_ease_in_bounce as *const u8,
    );
    builder.symbol(
        "builtin_math_ease_out_bounce",
        builtins::builtin_math_ease_out_bounce as *const u8,
    );
    builder.symbol(
        "builtin_math_ease_in_out_bounce",
        builtins::builtin_math_ease_in_out_bounce as *const u8,
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
    builder.symbol(
        "molang_rt_equal_paths",
        molang_rt_equal_paths as *const u8,
    );
    builder.symbol(
        "molang_rt_not_equal_paths",
        molang_rt_not_equal_paths as *const u8,
    );
    builder.symbol(
        "molang_rt_equal_path_string",
        molang_rt_equal_path_string as *const u8,
    );
    builder.symbol(
        "molang_rt_not_equal_path_string",
        molang_rt_not_equal_path_string as *const u8,
    );
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
    equal_paths: FuncId,
    not_equal_paths: FuncId,
    equal_path_string: FuncId,
    not_equal_path_string: FuncId,
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

        let mut equal_paths_sig = module.make_signature();
        equal_paths_sig.params.push(AbiParam::new(pointer_type));
        equal_paths_sig.params.push(AbiParam::new(pointer_type));
        equal_paths_sig.params.push(AbiParam::new(pointer_type));
        equal_paths_sig.params.push(AbiParam::new(pointer_type));
        equal_paths_sig.params.push(AbiParam::new(pointer_type));
        equal_paths_sig.returns.push(AbiParam::new(types::F64));
        let equal_paths =
            module.declare_function("molang_rt_equal_paths", Linkage::Import, &equal_paths_sig)?;

        let not_equal_paths = module.declare_function(
            "molang_rt_not_equal_paths",
            Linkage::Import,
            &equal_paths_sig,
        )?;

        let equal_path_string = module.declare_function(
            "molang_rt_equal_path_string",
            Linkage::Import,
            &equal_paths_sig,
        )?;

        let not_equal_path_string = module.declare_function(
            "molang_rt_not_equal_path_string",
            Linkage::Import,
            &equal_paths_sig,
        )?;

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
            equal_paths,
            not_equal_paths,
            equal_path_string,
            not_equal_path_string,
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

#[no_mangle]
pub extern "C" fn molang_rt_equal_paths(
    ctx: *mut RuntimeContext,
    left_ptr: *const u8,
    left_len: usize,
    right_ptr: *const u8,
    right_len: usize,
) -> f64 {
    if ctx.is_null() || left_ptr.is_null() || right_ptr.is_null() {
        return 0.0;
    }
    let left_bytes = unsafe { slice::from_raw_parts(left_ptr, left_len) };
    let right_bytes = unsafe { slice::from_raw_parts(right_ptr, right_len) };
    if let (Ok(left_name), Ok(right_name)) = (str::from_utf8(left_bytes), str::from_utf8(right_bytes))
    {
        let runtime = unsafe { &*ctx };
        let left_val = runtime.get_value_canonical(left_name);
        let right_val = runtime.get_value_canonical(right_name);

        match (left_val, right_val) {
            (Some(RuntimeValue::String(l)), Some(RuntimeValue::String(r))) => {
                if l == r { 1.0 } else { 0.0 }
            }
            (Some(RuntimeValue::Number(l)), Some(RuntimeValue::Number(r))) => {
                if l == r { 1.0 } else { 0.0 }
            }
            (None, None) => 1.0,
            _ => 0.0,
        }
    } else {
        0.0
    }
}

#[no_mangle]
pub extern "C" fn molang_rt_not_equal_paths(
    ctx: *mut RuntimeContext,
    left_ptr: *const u8,
    left_len: usize,
    right_ptr: *const u8,
    right_len: usize,
) -> f64 {
    if molang_rt_equal_paths(ctx, left_ptr, left_len, right_ptr, right_len) == 1.0 {
        0.0
    } else {
        1.0
    }
}

#[no_mangle]
pub extern "C" fn molang_rt_equal_path_string(
    ctx: *mut RuntimeContext,
    path_ptr: *const u8,
    path_len: usize,
    str_ptr: *const u8,
    str_len: usize,
) -> f64 {
    if ctx.is_null() || path_ptr.is_null() || str_ptr.is_null() {
        return 0.0;
    }
    let path_bytes = unsafe { slice::from_raw_parts(path_ptr, path_len) };
    let str_bytes = unsafe { slice::from_raw_parts(str_ptr, str_len) };
    if let (Ok(path_name), Ok(str_val)) = (str::from_utf8(path_bytes), str::from_utf8(str_bytes)) {
        let runtime = unsafe { &*ctx };
        if let Some(RuntimeValue::String(s)) = runtime.get_value_canonical(path_name) {
            if s == str_val { 1.0 } else { 0.0 }
        } else {
            0.0
        }
    } else {
        0.0
    }
}

#[no_mangle]
pub extern "C" fn molang_rt_not_equal_path_string(
    ctx: *mut RuntimeContext,
    path_ptr: *const u8,
    path_len: usize,
    str_ptr: *const u8,
    str_len: usize,
) -> f64 {
    if molang_rt_equal_path_string(ctx, path_ptr, path_len, str_ptr, str_len) == 1.0 {
        0.0
    } else {
        1.0
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
