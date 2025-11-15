# Molang Engine Internals

This document describes how the runtime executes a Molang snippet from raw text to final `f64` output.

## High-Level Flow

1. **Lexing** (`lexer.rs`) – Splits input into `Token`s (identifiers, numbers, operators, strings, punctuation).
2. **Parsing** (`parser.rs`) – Builds an AST: `Program` with `Statement`s (`Expr`, `Assignment`, `Loop`, `ForEach`, `Return`, `Block`). Expressions are trees of `Expr` nodes (numbers, paths, arrays, strings, unary/binary ops, calls, flow markers).
3. **IR Lowering** (`lib.rs`) – `evaluate_expression` checks whether the program is a single, flow-free expression (`Program::as_jit_expression`):
   - If yes → cached JIT compilation via `jit_cache`
   - If no → on-demand JIT compilation via `jit::compile_program`
4. **Expression JIT Path** (cached):
   - `IrBuilder` (`ir.rs`) lowers the expression AST into `IrExpr`.
   - `jit_cache` stores compiled expressions in a thread-local map keyed by original source. If not cached, `jit::compile_expression` (Cranelift) builds a function that reads required variables from runtime slots.
   - `CompiledExpression::evaluate` executes the JIT-compiled native code and returns the resulting `f64`.
5. **Program JIT Path** (on-demand):
   - `IrBuilder` lowers the entire program into `IrProgram` with statement-level IR.
   - `jit::compile_program` compiles all statements, control flow, loops, and expressions to native machine code.
   - Supports: `loop()` with break/continue, `for_each()` with element binding, array operations, struct literals, string assignments.
6. **Builtins** – `math.*` functions are JIT-compiled to direct native calls using host helpers from `builtins.rs`. A global RNG (mutex-protected) provides thread-safe randomness. Functions are registered via `BuiltinFunction::symbol_name` for Cranelift symbol resolution.

## Runtime Context & Values

- `RuntimeContext` stores a `HashMap<QualifiedName, Value>`. Namespaces are inferred from prefixes (`temp`, `variable`, `context`, `query`). Arrays and strings are fully owned values; struct values use `IndexMap<String, Value>` so nested assignments automatically build parent structs.
- `Value::truthy` mirrors Molang rules (zero/empty => false). Arrays fall back to their length when coerced to `f64`. Query values are injected by host code via `RuntimeContext::with_query(...)`.
- JIT-compiled code accesses the runtime context through FFI helpers (`molang_rt_*` functions) that safely read and write values.

## Cranelift JIT Details

### Expression IR
- `IrExpr` supports: constants, paths, unary/binary ops, conditionals, builtin calls, arrays, structs, strings, indexing, and control flow.
- When used as values (not assignments), arrays return their length, allowing `return [1,2,3]` to produce `3.0`.

### Statement IR
- `IrStatement` includes: assignments, blocks, loops, for_each, return, and expression statements.
- `loop(count, body)` compiles to native loop with header/body/increment blocks and break/continue support.
- `for_each(var, collection, body)` compiles to array iteration with element copying via `molang_rt_array_copy_element`.
- Control flow (`break`/`continue`) compiles to direct jumps to appropriate blocks tracked via `LoopContext` stack.

### Code Generation
- `jit.rs` translates IR into CLIF via `Translator`. Each referenced variable becomes a slot index.
- Builtins are declared through `BuiltinFunction::symbol_name` and registered with Cranelift's JIT builder (`register_builtin_symbols`).
- `jit_cache` caches `Arc<CompiledExpression>` per thread to avoid recompilation of pure expressions.

### Runtime Helpers
- All compiled functions receive `(RuntimeContext*, RuntimeSlot*)` parameters.
- `RuntimeSlot` is a compile-time table storing canonical path strings (`temp.speed`, `query.foo`, etc.).
- FFI helpers (`molang_rt_*`) provide safe access to runtime values:
  - `molang_rt_get_number` / `molang_rt_set_number` - numeric variable access
  - `molang_rt_set_string` - string literal assignment (via global data)
  - `molang_rt_array_push_number` / `molang_rt_array_push_string` - array construction
  - `molang_rt_array_get_number` - array element access
  - `molang_rt_array_length` - array length queries
  - `molang_rt_array_copy_element` - array iteration support
  - `molang_rt_copy_value` - variable-to-variable assignment
  - `molang_rt_clear_value` - variable deletion

### Assignment Strategy
- Simple numeric assignments use `molang_rt_set_number`
- String literals compile to global data objects with `molang_rt_set_string` calls
- Array literals clear the target slot then push elements via typed helpers
- Struct literals assign each field individually (building parent structs automatically)
- Path-to-path copies optimize to `molang_rt_copy_value` instead of load+store

## Testing

- `cargo test` executes unit tests covering loops, `continue`, `for_each`, math helpers, literals, and cache reuse (`src/lib.rs`). Docstrings and examples in `README.md` show how to run scripts via the CLI (`cargo run -- "<script>"`).

## Worked Example

Expression:

```molang
temp.values = [1, 2, 3, 4];
temp.total = 0;
for_each(temp.item, temp.values, {
  temp.total = temp.total + temp.item;
});
return temp.total;
```

1. **Lexing**
   - Tokens (kind → lexeme): `Identifier(temp)`, `Dot`, `Identifier(values)`, `Equal`, `LBracket`, `Number(1)`, `Comma`, `Number(2)`, `Comma`, `Number(3)`, `Comma`, `Number(4)`, `RBracket`, `Semicolon`, … etc. `for_each` is an identifier (case-insensitive check later).
2. **Parsing**
   - Statements:
     1. `Assignment` target `["temp","values"]`, value `Expr::Array([...Number nodes...])`
     2. `Assignment` target `["temp","total"]`, value `Expr::Number(0)`
     3. `ForEach`:
        - variable path `["temp","item"]`
        - collection `Expr::Path(["temp","values"])`
        - body `Block` with a single `Assignment` that adds `temp.item` to `temp.total`
     4. `Return(Expr::Path(["temp","total"]))`
3. **IR Lowering**
   - `Program::as_jit_expression` returns `None` (multiple statements), so we use `jit::compile_program`.
   - `IrBuilder::lower_program` produces `IrProgram` with 4 statements.
4. **JIT Compilation**
   - First assignment: `assign_expression` clears `temp.values` slot, then pushes 4 numeric elements via `molang_rt_array_push_number`.
   - Second assignment: Translates to `molang_rt_set_number(temp.total, 0.0)`.
   - ForEach: Compiles to:
     - Call `molang_rt_array_length(temp.values)` to get count
     - Create loop blocks (header, body, increment, exit)
     - Loop body calls `molang_rt_array_copy_element` to bind current element to `temp.item`
     - Translates body statement (load `temp.total`, load `temp.item`, add, store)
     - Increment block advances loop counter
   - Return: Loads `temp.total` via `molang_rt_get_number` and returns it.
5. **Execution**
   - `CompiledExpression::evaluate` calls the JIT-compiled function with RuntimeContext pointer.
   - Native code executes, calling runtime helpers as needed.
   - Final result: `10.0`

### Pure Expression Example

For a pure expression like `return temp.total + math.clamp(math.random(0, 5), 1, 4);`:
- Parser outputs a single `Expr::Binary`
- Lowering builds an `IrExpr::Binary` tree with builtin calls (`FunctionRef::Builtin(MathClamp)`)
- Cranelift emits machine code that:
  1. Calls `molang_rt_get_number` for `temp.total`
  2. Calls `molang_builtin_random` and `molang_builtin_clamp` (direct function calls)
  3. Adds the results
  4. Returns the final `f64`
- Result is cached for subsequent evaluations with the same source text
