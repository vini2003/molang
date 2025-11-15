use colored::*;
use molang::{eval::RuntimeContext, evaluate_expression};
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;

fn main() {
    // Check if we're in single-expression mode (command-line argument)
    let args: Vec<String> = std::env::args().skip(1).collect();
    if !args.is_empty() {
        let expression = args.join(" ");
        let mut ctx = RuntimeContext::default();
        match evaluate_expression(&expression, &mut ctx) {
            Ok(value) => println!("{value}"),
            Err(err) => {
                eprintln!("Error: {err}");
                std::process::exit(1);
            }
        }
        return;
    }

    // Interactive REPL mode
    run_repl();
}

fn run_repl() {
    println!("{}", "╔══════════════════════════════════════════════════════════════╗".bright_cyan());
    println!("{}", "║           Molang Interactive REPL - JIT Compiler            ║".bright_cyan());
    println!("{}", "╚══════════════════════════════════════════════════════════════╝".bright_cyan());
    println!();
    println!("{}", "  All expressions are compiled to native code via Cranelift JIT".bright_black());
    println!("{}", "  Type :help for available commands".bright_black());
    println!();

    let mut rl = DefaultEditor::new().expect("Failed to create readline editor");
    let mut ctx = RuntimeContext::default();
    let mut multiline_buffer = String::new();

    loop {
        let prompt = if multiline_buffer.is_empty() {
            "molang> ".bright_green().to_string()
        } else {
            "     -> ".bright_yellow().to_string()
        };

        match rl.readline(&prompt) {
            Ok(line) => {
                let trimmed = line.trim();

                // Handle special commands (only when not in multiline mode)
                if multiline_buffer.is_empty() && trimmed.starts_with(':') {
                    match trimmed {
                        ":help" | ":h" => show_help(),
                        ":clear" | ":c" => {
                            ctx = RuntimeContext::default();
                            println!("{}", "✓ Context cleared".bright_green());
                        }
                        ":vars" | ":v" => show_variables(&ctx),
                        ":exit" | ":quit" | ":q" => {
                            println!("{}", "Goodbye!".bright_cyan());
                            break;
                        }
                        _ => println!("{}", format!("Unknown command: {}", trimmed).bright_red()),
                    }
                    continue;
                }

                // Check for multiline continuation (backslash at end)
                if trimmed.ends_with('\\') {
                    multiline_buffer.push_str(&line[..line.len() - 1]);
                    multiline_buffer.push('\n');
                    continue;
                }

                // Add current line to buffer
                if !multiline_buffer.is_empty() {
                    multiline_buffer.push_str(&line);
                    multiline_buffer.push('\n');
                } else if !trimmed.is_empty() {
                    multiline_buffer = line.clone();
                } else {
                    continue; // Skip empty lines
                }

                // Evaluate the complete expression
                let input = multiline_buffer.trim().to_string();
                if !input.is_empty() {
                    rl.add_history_entry(&input).ok();
                    evaluate_and_display(&input, &mut ctx);
                }

                multiline_buffer.clear();
            }
            Err(ReadlineError::Interrupted) => {
                println!("{}", "^C (use :exit to quit)".bright_yellow());
                multiline_buffer.clear();
            }
            Err(ReadlineError::Eof) => {
                println!("{}", "Goodbye!".bright_cyan());
                break;
            }
            Err(err) => {
                eprintln!("{}", format!("Error: {err}").bright_red());
                break;
            }
        }
    }
}

fn evaluate_and_display(input: &str, ctx: &mut RuntimeContext) {
    match evaluate_expression(input, ctx) {
        Ok(value) => {
            // Format the output nicely
            if value.fract() == 0.0 && value.abs() < 1e10 {
                println!("{} {}", "=>".bright_blue(), format!("{:.0}", value).bright_white().bold());
            } else {
                println!("{} {}", "=>".bright_blue(), format!("{}", value).bright_white().bold());
            }
        }
        Err(err) => {
            println!("{} {}", "✗".bright_red(), format!("{}", err).bright_red());
        }
    }
}

fn show_help() {
    println!();
    println!("{}", "╔══════════════════════════════════════════════════════════════╗".bright_cyan());
    println!("{}", "║                      REPL Commands                           ║".bright_cyan());
    println!("{}", "╚══════════════════════════════════════════════════════════════╝".bright_cyan());
    println!();
    println!("  {}  Show this help message", ":help, :h".bright_green());
    println!("  {}  Clear the runtime context (all variables)", ":clear, :c".bright_green());
    println!("  {}  Show all variables in context", ":vars, :v".bright_green());
    println!("  {}  Exit the REPL", ":exit, :quit, :q".bright_green());
    println!();
    println!("{}", "╔══════════════════════════════════════════════════════════════╗".bright_cyan());
    println!("{}", "║                    Molang Features                           ║".bright_cyan());
    println!("{}", "╚══════════════════════════════════════════════════════════════╝".bright_cyan());
    println!();
    println!("  {} Variables and namespaces", "•".bright_yellow());
    println!("    {}    temp.x = 42; temp.y = temp.x * 2", "Example:".bright_black());
    println!();
    println!("  {} Arrays and indexing", "•".bright_yellow());
    println!("    {}    temp.arr = [1, 2, 3]; temp.arr[0]", "Example:".bright_black());
    println!("    {}    temp.arr.length", "Example:".bright_black());
    println!();
    println!("  {} Structs and nested fields", "•".bright_yellow());
    println!("    {}    temp.player = {{x: 10, y: 20}}; temp.player.x", "Example:".bright_black());
    println!();
    println!("  {} Control flow", "•".bright_yellow());
    println!("    {}    loop(5, {{ temp.i = temp.i + 1; }})", "Example:".bright_black());
    println!("    {}    for_each(temp.item, temp.arr, {{ ... }})", "Example:".bright_black());
    println!("    {}    (temp.x > 10) ? break", "Example:".bright_black());
    println!();
    println!("  {} Math functions", "•".bright_yellow());
    println!("    {}    math.cos, math.sin, math.sqrt, math.abs", "Available:".bright_black());
    println!("    {}    math.floor, math.ceil, math.round, math.trunc", "          ".bright_black());
    println!("    {}    math.clamp, math.random, math.random_integer", "          ".bright_black());
    println!();
    println!("  {} Multi-line input", "•".bright_yellow());
    println!("    {}    End a line with \\ to continue on the next line", "Tip:".bright_black());
    println!();
}

fn show_variables(ctx: &RuntimeContext) {
    let vars = ctx.list_variables();

    if vars.is_empty() {
        println!("{}", "  No variables in context".bright_black());
        return;
    }

    println!();
    println!("{}", "╔══════════════════════════════════════════════════════════════╗".bright_cyan());
    println!("{}", "║                    Context Variables                         ║".bright_cyan());
    println!("{}", "╚══════════════════════════════════════════════════════════════╝".bright_cyan());
    println!();

    for (name, value) in vars {
        let value_str = match value {
            molang::eval::Value::Number(n) => {
                if n.fract() == 0.0 && n.abs() < 1e10 {
                    format!("{:.0}", n).bright_white().to_string()
                } else {
                    format!("{}", n).bright_white().to_string()
                }
            }
            molang::eval::Value::String(s) => format!("\"{}\"", s).bright_green().to_string(),
            molang::eval::Value::Array(arr) => {
                format!("[{} items]", arr.len()).bright_yellow().to_string()
            }
            molang::eval::Value::Struct(map) => {
                format!("{{{}  fields}}", map.len()).bright_magenta().to_string()
            }
            molang::eval::Value::Null => {
                "null".bright_black().to_string()
            }
        };

        println!("  {} = {}", name.bright_blue(), value_str);
    }
    println!();
}
