# Molang Compiler & Runtime

## Overview

- Rust implementation of Molang expression runtime with full JIT compilation via Cranelift - all code is compiled to native machine code.
- Grammar closely mirrors Bedrock's Molang spec (case-insensitive identifiers, ternaries, `??`, logical ops, namespaces, `loop`, `for_each`, brace blocks).
- Runtime context exposes `temp.`, `variable.`, `context.` namespaces, supports numbers, strings, arrays, and structured assignments.
- Builtins include `math.cos/sin/abs/random/random_integer/clamp/sqrt/floor/ceil/round/trunc`.
- Interactive REPL with multi-line support, command history, and syntax highlighting.

## Supported Features

- Expressions: numeric ops, precedence, `?:`, `??`, logical `&&/||/!`, unary +/-.
- Literals: numbers, quoted strings, array literals `[a, b, c]`, struct literals `{ x: 1, y: 2 }`.
- Namespaces: `t.`, `temp.`, `v.`, `variable.`, `context.`, `query.` with dot-path segments.
- Statements: brace-delimited blocks, semicolon-separated statements, assignments, `loop(count, expr_or_block)`, `for_each(var, collection, expr_or_block)`, `break`, `continue`, `return`.
- Struct members are built automatically: assigning `temp.location.z = 3` populates `temp.location` as a nested struct. Array literals support indexing (`temp.values[i]`) and `.length`.
- Builtins: `math.*` functions JIT-compiled to direct native calls.
- Query namespace: bind dynamic values with `RuntimeContext::with_query("speed", 2.5)` and read `query.speed` inside Molang.
- JIT caching: repeated pure expressions re-use compiled code keyed by source string.
- Control flow: loops, for_each, break, and continue all compiled to native control flow instructions.

## Unsupported / Not Yet Implemented

- Minecraft-specific systems (textures, geometry, queries beyond math namespace).
- Arrow (`->`) operator and entity references.
- Experimental operators not covered in the public Molang math/documented subset.
- Persistence of `variable.` values across executions (context resets per run).
- Array mutation/slicing (beyond literals, indexing, and `.length`).

## Behavioral Notes & Limitations

- All code is JIT-compiled to native machine code via Cranelift - there is no interpreter fallback.
- Pure expressions are cached; programs with statements are compiled on-demand.
- Random functions use a process-global `SmallRng`; results are non-deterministic between runs but thread-safe.
- `??` is implemented as "null-like" check; only `null` counts as missing, unlike Bedrock's broader definition.

## Examples

```molang
temp.location = { x: 1, y: 2 };
temp.location.z = 3;
return temp.location.x + temp.location.y + temp.location.z;  # -> 6
```

```molang
temp.counter = 0;
loop(10, {
  temp.counter = temp.counter + 1;
  (temp.counter > 5) ? break;
});
return temp.counter;      # -> 6
```

```molang
temp.values = [2, 4, 6, 8];
temp.total = 0;
for_each(temp.item, temp.values, {
  temp.total = temp.total + temp.item;
  (temp.item >= 6) ? break;
});
return temp.total;        # -> 12

# Array indexing + length
temp.index = 1;
return temp.values[temp.index] + temp.values[3] + temp.values.length;  # -> 33
```

```molang
# Pure expression hits the JIT cache
math.clamp(math.random(0, 5), 1, 4) ?? 2
```

## Usage

### Interactive REPL

Run without arguments to start the interactive REPL:

```bash
cargo run --release
```

Features:
- Multi-line input with `\` continuation
- Command history (up/down arrows)
- Special commands: `:help`, `:vars`, `:clear`, `:exit`
- Syntax highlighting and colored output

See [REPL_DEMO.md](REPL_DEMO.md) for examples.

### Single Expression

Evaluate a single expression from the command line:

```bash
cargo run -- "return math.sqrt(16);"
cargo run -- "temp.x = 5; temp.y = 10; return temp.x + temp.y"
```

### Running Tests

```bash
cargo test
```

In Rust you can inject query data before evaluation:

```rust
let mut ctx = RuntimeContext::default()
    .with_query("speed", 2.5)
    .with_query("offset", -1.0);
let value = evaluate_expression("query.speed + math.abs(query.offset)", &mut ctx).unwrap();
```
