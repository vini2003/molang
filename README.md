# Molang Compiler & Runtime

## Overview

- Rust implementation of Molang expression runtime with full JIT compilation via Cranelift - all code is compiled to native machine code.
- Grammar closely mirrors Bedrock's Molang spec (case-insensitive identifiers, ternaries, `??`, logical ops, namespaces, `loop`, `for_each`, brace blocks).
- Runtime context exposes `temp.`, `variable.`, `context.` namespaces, supports numbers, strings, arrays, and structured assignments.
- Comprehensive math library with 67+ functions including trigonometry, easing, interpolation, and more (full Bedrock math namespace).
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

## Math Functions

All math functions are JIT-compiled to direct native calls for maximum performance.

### Basic Functions
- `math.abs(x)` - Absolute value
- `math.floor(x)`, `math.ceil(x)`, `math.round(x)`, `math.trunc(x)` - Rounding
- `math.clamp(value, min, max)` - Clamp value to range
- `math.max(a, b)`, `math.min(a, b)` - Min/max
- `math.mod(value, denominator)` - Modulo
- `math.sign(x)` - Returns 1 if positive, -1 otherwise
- `math.copy_sign(a, b)` - Returns `a` with the sign of `b`
- `math.sqrt(x)` - Square root
- `math.pi()` - Returns π constant

### Trigonometric Functions (degrees)
- `math.cos(degrees)`, `math.sin(degrees)` - Cosine and sine
- `math.acos(x)`, `math.asin(x)`, `math.atan(x)` - Inverse trig functions
- `math.atan2(y, x)` - Two-argument arctangent

### Exponential & Logarithmic
- `math.exp(x)` - e^x
- `math.ln(x)` - Natural logarithm
- `math.pow(base, exponent)` - Power function

### Random Functions
- `math.random(low, high)` - Random float in range
- `math.random_integer(low, high)` - Random integer in range
- `math.die_roll(num, low, high)` - Sum of `num` random floats
- `math.die_roll_integer(num, low, high)` - Sum of `num` random integers

### Angle Functions
- `math.min_angle(degrees)` - Normalize angle to [-180, 180)

### Interpolation Functions
- `math.lerp(start, end, t)` - Linear interpolation
- `math.inverse_lerp(start, end, value)` - Inverse linear interpolation
- `math.lerprotate(start, end, t)` - Shortest rotation interpolation
- `math.hermite_blend(t)` - Hermite smoothing: 3t² - 2t³

### Easing Functions

All easing functions take `(start, end, t)` parameters where `t` is in [0,1].

**Quadratic**: `ease_in_quad`, `ease_out_quad`, `ease_in_out_quad`

**Cubic**: `ease_in_cubic`, `ease_out_cubic`, `ease_in_out_cubic`

**Quartic**: `ease_in_quart`, `ease_out_quart`, `ease_in_out_quart`

**Quintic**: `ease_in_quint`, `ease_out_quint`, `ease_in_out_quint`

**Sine**: `ease_in_sine`, `ease_out_sine`, `ease_in_out_sine`

**Exponential**: `ease_in_expo`, `ease_out_expo`, `ease_in_out_expo`

**Circular**: `ease_in_circ`, `ease_out_circ`, `ease_in_out_circ`

**Back** (overshoot): `ease_in_back`, `ease_out_back`, `ease_in_out_back`

**Elastic** (spring): `ease_in_elastic`, `ease_out_elastic`, `ease_in_out_elastic`

**Bounce**: `ease_in_bounce`, `ease_out_bounce`, `ease_in_out_bounce`

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
