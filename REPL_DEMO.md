# Molang Interactive REPL - Quick Start Guide

## Running the REPL

Simply run the executable without any arguments to start the interactive REPL:

```bash
cargo run --release
# or
./target/release/molang
```

## Example Session

Here's a quick demo of what you can do:

```molang
molang> 1 + 2 * 3
=> 7

molang> temp.x = 42
=> 42

molang> temp.y = temp.x * 2 + 10
=> 94

molang> :vars
╔══════════════════════════════════════════════════════════════╗
║                    Context Variables                         ║
╚══════════════════════════════════════════════════════════════╝

  temp.x = 42
  temp.y = 94

molang> temp.arr = [1, 2, 3, 4, 5]
=> 5

molang> temp.arr[2]
=> 3

molang> temp.arr.length
=> 5

molang> temp.sum = 0; \
     -> loop(temp.arr.length, { \
     ->   temp.sum = temp.sum + temp.arr[temp.i ?? 0]; \
     ->   temp.i = (temp.i ?? 0) + 1; \
     -> }); \
     -> temp.sum
=> 15

molang> temp.player = {x: 100, y: 200, health: 20}
=> 3

molang> temp.player.x
=> 100

molang> for_each(temp.val, temp.arr, { \
     ->   temp.product = (temp.product ?? 1) * temp.val; \
     -> }); \
     -> temp.product
=> 120

molang> math.sqrt(16)
=> 4

molang> math.random(1, 10)
=> 7.234...

molang> :vars
╔══════════════════════════════════════════════════════════════╗
║                    Context Variables                         ║
╚══════════════════════════════════════════════════════════════╝

  temp.arr = [5 items]
  temp.i = 5
  temp.player = {3 fields}
  temp.player.health = 20
  temp.player.x = 100
  temp.player.y = 200
  temp.product = 120
  temp.sum = 15
  temp.val = 5
  temp.x = 42
  temp.y = 94

molang> :clear
✓ Context cleared

molang> :exit
Goodbye!
```

## REPL Commands

| Command | Shortcut | Description |
|---------|----------|-------------|
| `:help` | `:h` | Show help message with all commands and features |
| `:vars` | `:v` | Display all variables in the current context |
| `:clear` | `:c` | Clear all variables and reset the context |
| `:exit` | `:q` | Exit the REPL |

## Multi-line Input

End any line with a backslash `\` to continue on the next line:

```molang
molang> temp.result = loop(10, { \
     ->   temp.i = temp.i + 1; \
     ->   (temp.i > 5) ? break; \
     -> }); \
     -> temp.i
=> 6
```

## Single-Expression Mode

You can also evaluate a single expression from the command line:

```bash
molang "1 + 2 * 3"        # => 7
molang "math.sqrt(144)"   # => 12
```

## Features

All features are JIT-compiled to native code for maximum performance:

- ✅ Variables and namespaces (`temp`, `variable`, `context`, `query`)
- ✅ Arrays with indexing and `.length` property
- ✅ Structs (objects) with field access
- ✅ Control flow: `loop()`, `for_each()`, `break`, `continue`
- ✅ Conditionals: `?` ternary operator
- ✅ Null coalescing: `??` operator
- ✅ Math functions: `cos`, `sin`, `sqrt`, `abs`, `floor`, `ceil`, `round`, `trunc`, `clamp`, `random`, `random_integer`
- ✅ Logical operators: `&&`, `||`, `!`
- ✅ Comparison operators: `<`, `<=`, `>`, `>=`, `==`, `!=`
- ✅ Arithmetic: `+`, `-`, `*`, `/`
- ✅ Command history (use up/down arrows)
- ✅ Line editing (Ctrl+A, Ctrl+E, etc.)

## Color Coding

The REPL uses colors to make output easier to read:

- 🟢 **Green prompts** - Ready for input
- 🟡 **Yellow prompts** - Multi-line continuation
- 🔵 **Blue arrows** - Successful result
- 🔴 **Red X** - Error message
- 🔵 **Blue text** - Variable names
- ⚪ **White text** - Numeric values
- 🟢 **Green text** - String values
- 🟡 **Yellow text** - Arrays
- 🟣 **Magenta text** - Structs

## Tips

1. **Tab completion** - Currently not implemented, but variable names autocomplete in some terminals
2. **History** - Use ↑ and ↓ to navigate through command history
3. **Ctrl+C** - Cancels the current multi-line input
4. **Ctrl+D** or `:exit`** - Exits the REPL

Enjoy your JIT-compiled Molang experience! 🚀
