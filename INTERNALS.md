# Molang Engine Internals

This document describes how the runtime executes a Molang snippet from raw text to final `f64` output.

## High-Level Flow

1. **Lexing** (`lexer.rs`) – Splits input into `Token`s (identifiers, numbers, operators, strings, punctuation).
2. **Parsing** (`parser.rs`) – Builds an AST: `Program` with `Statement`s (`Expr`, `Assignment`, `Loop`, `ForEach`, `Return`, `Block`). Expressions are trees of `Expr` nodes (numbers, paths, arrays, strings, unary/binary ops, calls, flow markers).
3. **Execution Choice** (`lib.rs`) – `evaluate_expression` checks whether the program is a single, flow-free expression (`Program::as_jit_expression`). If yes, it goes through the JIT path; otherwise it runs through the interpreter.
4. **JIT Path**:
   - `IrBuilder` (`ir.rs`) lowers the expression AST into `IrExpr` (only numeric, path, unary/binary, conditional, builtin calls). Control-flow or literal expressions return `LowerError`.
   - `jit_cache` stores compiled expressions in a thread-local map keyed by original source. If not cached, `jit::compile_expression` (Cranelift) builds a function that reads required variables from a contiguous slot array.
   - `CompiledExpression::evaluate` converts requested namespaces to slot values via `RuntimeContext::get_number`, executes the JITed function, and returns the resulting `f64`.
5. **Interpreter Path**:
   - `Executor` walks the statement list with a shared `RuntimeContext`. `temp.`/`variable.`/`context.` namespaces are normalized via `QualifiedName` and stored as `Value` (numbers, strings, arrays, null).
   - Statements: blocks recursively evaluate; assignments mutate the context; `loop` clamps counts to 1024 iterations and respects `break`/`continue`; `for_each` iterates a computed array, binding each element to the provided variable path; `return` short-circuits with a `Value`.
   - Expressions: support the full operator set (logical short-circuit, `??`, ternaries). `Value::truthy` matches Molang semantics (non-zero numbers, non-empty strings/arrays). Builtin calls map to `BuiltinFunction::evaluate`, which delegates to helpers in `builtins.rs`.
6. **Builtins** – Implemented for both interpreter and JIT. `math.*` functions call host helpers (`builtins.rs`) which use a global RNG (mutex) for random operations and rely on `BuiltinFunction::symbol_name` for Cranelift symbol registration (`jit.rs`).

## Runtime Context & Values

- `RuntimeContext` stores a `HashMap<QualifiedName, Value>`. Namespaces are inferred from prefixes (`temp`, `variable`, `context`) with case-insensitive keys. Arrays and strings are fully owned values; numbers remain `f64`.
- `Value::truthy` mirrors Molang rules (zero/empty => false). Arrays fall back to their length when coerced to `f64` (used for simple queries like `return [1,2];`).

## Interpreter Details

- Control-flow is modeled with `ControlSignal` (`None`, `Break`, `Continue`, `Return(Value)`). `Executor::exec_statement` validates `break`/`continue` usage based on `loop_depth`.
- `loop(count, body)` evaluates `count` as a number and clamps to `[0, 1024]`. Each iteration runs the body; `break`/`continue` propagate via `ControlSignal`.
- `for_each(temp.item, expr, body)` evaluates `expr` and expects a `Value::Array`, iterating elements and rebinding the target variable each time.
- Expressions inside statements are evaluated via `eval_expr`, which returns `(Value, ControlSignal)`. Null-coalescing returns the first non-null Value; logical ops short-circuit but return numbers (`1` or `0`) to match Molang semantics.

## Cranelift JIT Details

- `IrExpr` is a numeric-only expression graph: constants, paths, unary/binary ops, conditional, builtin calls. Lowering rejects array/string/flow literals.
- `jit.rs` translates `IrExpr` into CLIF via `Translator`. Each referenced path becomes a slot index; the interpreter must provide the numeric values when executing the compiled function (`CompiledExpression::evaluate`).
- Builtins are declared through `BuiltinFunction::symbol_name` and registered with Cranelift's JIT builder (`register_builtin_symbols`). Random/clamp/floor/ceil suck in helper functions from `builtins.rs`.
- `jit_cache` caches `Arc<CompiledExpression>` per thread to avoid recompilation. Keying by the original input string means identical expressions reuse compiled code even if evaluated in different contexts; interpreter values are injected per call.

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
3. **Execution Choice**
   - `Program::as_jit_expression` returns `None` (multiple statements + arrays), so we dispatch to the interpreter.
4. **Interpreter**
   - `Executor` evaluates assignment statements, storing `Value::Array([1,2,3,4])` and `Value::Number(0)` inside `RuntimeContext`.
   - `ForEach` evaluates the collection to `Value::Array`, iterates over elements, rebinding `temp.item` each time, and mutating `temp.total`.
   - `Return` short-circuits with `Value::Number(10)`.
5. **If Pure Expression**
   - For a pure numeric expression like `return temp.total + math.clamp(math.random(0, 5), 1, 4);`, the parser would output a single `Expr::Binary`. The lowering pipeline would build an `IrExpr::Binary` tree with builtin calls (`FunctionRef::Builtin(MathClamp)` etc.), and Cranelift would emit machine code that reads `temp.total` from a slot, calls the registered math helpers, and returns the final `f64`.
