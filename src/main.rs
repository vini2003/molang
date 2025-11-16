use molang::{eval::RuntimeContext, evaluate_expression, lexer::{lex, TokenKind}};
use nu_ansi_term::{Color, Style};
use reedline::{DefaultPrompt, DefaultPromptSegment, Highlighter, Reedline, Signal, StyledText};

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

struct MolangHighlighter;

impl Highlighter for MolangHighlighter {
    fn highlight(&self, line: &str, _cursor: usize) -> StyledText {
        let mut styled = StyledText::new();

        // Handle empty line
        if line.is_empty() {
            return styled;
        }

        // Try to tokenize the line
        match lex(line) {
            Ok(tokens) => {
                let mut last_end = 0;

                for token in tokens {
                    // Skip EOF token
                    if matches!(token.kind, TokenKind::EOF) {
                        continue;
                    }

                    // Add any whitespace/text before this token as unstyled
                    if token.span.start > last_end {
                        styled.push((
                            Style::new(),
                            line[last_end..token.span.start].to_string(),
                        ));
                    }

                    // Bounds check
                    if token.span.end >= line.len() {
                        continue;
                    }

                    // Get the token text
                    let token_text = &line[token.span.start..=token.span.end];

                    // Apply color based on token kind
                    let style = match token.kind {
                        // Keywords and control flow
                        TokenKind::Identifier(ref name) if is_keyword(name) => {
                            Style::new().fg(Color::Magenta).bold()
                        }
                        // Math functions
                        TokenKind::Identifier(ref name) if name.starts_with("math.") => {
                            Style::new().fg(Color::Blue)
                        }
                        // Identifiers (variables, paths)
                        TokenKind::Identifier(_) => Style::new().fg(Color::Cyan),
                        // Numbers
                        TokenKind::Number(_) => Style::new().fg(Color::Yellow),
                        // Strings
                        TokenKind::String(_) => Style::new().fg(Color::Green),
                        // Operators
                        TokenKind::Plus | TokenKind::Minus | TokenKind::Star | TokenKind::Slash |
                        TokenKind::EqualEqual | TokenKind::BangEqual |
                        TokenKind::Less | TokenKind::LessEqual |
                        TokenKind::Greater | TokenKind::GreaterEqual |
                        TokenKind::AndAnd | TokenKind::OrOr | TokenKind::Bang |
                        TokenKind::Question | TokenKind::QuestionQuestion => {
                            Style::new().fg(Color::Red)
                        }
                        // Assignment
                        TokenKind::Equal => Style::new().fg(Color::Red).bold(),
                        // Punctuation
                        TokenKind::LParen | TokenKind::RParen |
                        TokenKind::LBrace | TokenKind::RBrace |
                        TokenKind::LBracket | TokenKind::RBracket |
                        TokenKind::Comma | TokenKind::Semicolon | TokenKind::Colon => {
                            Style::new().fg(Color::White)
                        }
                        // Dot for member access
                        TokenKind::Dot => Style::new().fg(Color::White),
                        // Arrow (not fully supported but highlight anyway)
                        TokenKind::Arrow => Style::new().fg(Color::Purple),
                        // EOF
                        TokenKind::EOF => Style::new(),
                    };

                    styled.push((style, token_text.to_string()));
                    last_end = token.span.end + 1;
                }

                // Add any remaining text
                if last_end < line.len() {
                    styled.push((Style::new(), line[last_end..].to_string()));
                }
            }
            Err(_) => {
                // If tokenization fails, just show the line without highlighting
                styled.push((Style::new(), line.to_string()));
            }
        }

        styled
    }
}

fn is_keyword(name: &str) -> bool {
    matches!(
        name.to_lowercase().as_str(),
        "return" | "loop" | "for_each" | "break" | "continue" |
        "temp" | "t" | "variable" | "v" | "context" | "c" | "query" | "q"
    )
}

fn run_repl() {
    println!("{}", Color::Cyan.bold().paint("╔══════════════════════════════════════════════════════════════╗"));
    println!("{}", Color::Cyan.bold().paint("║          Molang Interactive REPL - JIT Compiler              ║"));
    println!("{}", Color::Cyan.bold().paint("╚══════════════════════════════════════════════════════════════╝"));
    println!();
    println!("{}", Color::DarkGray.paint("  All expressions are compiled to native code via Cranelift JIT"));
    println!("{}", Color::DarkGray.paint("  Type :help for available commands"));
    println!();

    let mut line_editor = Reedline::create().with_highlighter(Box::new(MolangHighlighter));
    let mut ctx = RuntimeContext::default();
    let mut multiline_buffer = String::new();

    let default_prompt = DefaultPrompt::new(
        DefaultPromptSegment::Basic("molang".to_string()),
        DefaultPromptSegment::Empty,
    );

    let continuation_prompt = DefaultPrompt::new(
        DefaultPromptSegment::Basic("     ".to_string()),
        DefaultPromptSegment::Empty,
    );

    loop {
        let prompt = if multiline_buffer.is_empty() {
            &default_prompt
        } else {
            &continuation_prompt
        };

        let sig = line_editor.read_line(prompt);

        match sig {
            Ok(Signal::Success(line)) => {
                let trimmed = line.trim();

                // Handle special commands (only when not in multiline mode)
                if multiline_buffer.is_empty() && trimmed.starts_with(':') {
                    match trimmed {
                        ":help" | ":h" => show_help(),
                        ":clear" | ":c" => {
                            ctx = RuntimeContext::default();
                            println!("{}", Color::Green.paint("✓ Context cleared"));
                        }
                        ":vars" | ":v" => show_variables(&ctx),
                        ":exit" | ":quit" | ":q" => {
                            println!("{}", Color::Cyan.paint("Goodbye!"));
                            break;
                        }
                        _ => println!("{}", Color::Red.paint(format!("Unknown command: {}", trimmed))),
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
                    evaluate_and_display(&input, &mut ctx);
                }

                multiline_buffer.clear();
            }
            Ok(Signal::CtrlC) => {
                println!("{}", Color::Yellow.paint("^C (use :exit to quit)"));
                multiline_buffer.clear();
            }
            Ok(Signal::CtrlD) => {
                println!("{}", Color::Cyan.paint("Goodbye!"));
                break;
            }
            Err(err) => {
                eprintln!("{}", Color::Red.paint(format!("Error: {err}")));
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
                println!(
                    "{} {}",
                    Color::Blue.bold().paint("=>"),
                    Color::White.bold().paint(format!("{:.0}", value))
                );
            } else {
                println!(
                    "{} {}",
                    Color::Blue.bold().paint("=>"),
                    Color::White.bold().paint(format!("{}", value))
                );
            }
        }
        Err(err) => {
            println!(
                "{} {}",
                Color::Red.bold().paint("✗"),
                Color::Red.paint(format!("{}", err))
            );
        }
    }
}

fn show_help() {
    println!();
    println!("{}", Color::Cyan.bold().paint("╔══════════════════════════════════════════════════════════════╗"));
    println!("{}", Color::Cyan.bold().paint("║                      REPL Commands                           ║"));
    println!("{}", Color::Cyan.bold().paint("╚══════════════════════════════════════════════════════════════╝"));
    println!();
    println!("  {}  Show this help message", Color::Green.paint(":help, :h"));
    println!("  {}  Clear the runtime context (all variables)", Color::Green.paint(":clear, :c"));
    println!("  {}  Show all variables in context", Color::Green.paint(":vars, :v"));
    println!("  {}  Exit the REPL", Color::Green.paint(":exit, :quit, :q"));
    println!();
    println!("{}", Color::Cyan.bold().paint("╔══════════════════════════════════════════════════════════════╗"));
    println!("{}", Color::Cyan.bold().paint("║                    Molang Features                           ║"));
    println!("{}", Color::Cyan.bold().paint("╚══════════════════════════════════════════════════════════════╝"));
    println!();
    println!("  {} Variables and namespaces", Color::Yellow.paint("•"));
    println!("    {}    temp.x = 42; temp.y = temp.x * 2", Color::DarkGray.paint("Example:"));
    println!();
    println!("  {} Arrays and indexing", Color::Yellow.paint("•"));
    println!("    {}    temp.arr = [1, 2, 3]; temp.arr[0]", Color::DarkGray.paint("Example:"));
    println!("    {}    temp.arr.length", Color::DarkGray.paint("Example:"));
    println!();
    println!("  {} Structs and nested fields", Color::Yellow.paint("•"));
    println!("    {}    temp.player = {{x: 10, y: 20}}; temp.player.x", Color::DarkGray.paint("Example:"));
    println!();
    println!("  {} Control flow", Color::Yellow.paint("•"));
    println!("    {}    loop(5, {{ temp.i = temp.i + 1; }})", Color::DarkGray.paint("Example:"));
    println!("    {}    for_each(temp.item, temp.arr, {{ ... }})", Color::DarkGray.paint("Example:"));
    println!("    {}    (temp.x > 10) ? break", Color::DarkGray.paint("Example:"));
    println!();
    println!("  {} Math functions", Color::Yellow.paint("•"));
    println!("    {}    math.cos, math.sin, math.sqrt, math.abs", Color::DarkGray.paint("Available:"));
    println!("    {}    math.floor, math.ceil, math.round, math.trunc", Color::DarkGray.paint("          "));
    println!("    {}    math.clamp, math.random, math.random_integer", Color::DarkGray.paint("          "));
    println!();
    println!("  {} String comparison", Color::Yellow.paint("•"));
    println!("    {}    temp.name = 'alice'; temp.name == 'bob'", Color::DarkGray.paint("Example:"));
    println!();
    println!("  {} Multi-line input", Color::Yellow.paint("•"));
    println!("    {}    End a line with \\ to continue on the next line", Color::DarkGray.paint("Tip:"));
    println!();
}

fn show_variables(ctx: &RuntimeContext) {
    let vars = ctx.list_variables();

    if vars.is_empty() {
        println!("{}", Color::DarkGray.paint("  No variables in context"));
        return;
    }

    println!();
    println!("{}", Color::Cyan.bold().paint("╔══════════════════════════════════════════════════════════════╗"));
    println!("{}", Color::Cyan.bold().paint("║                    Context Variables                         ║"));
    println!("{}", Color::Cyan.bold().paint("╚══════════════════════════════════════════════════════════════╝"));
    println!();

    for (name, value) in vars {
        let value_str = match value {
            molang::eval::Value::Number(n) => {
                if n.fract() == 0.0 && n.abs() < 1e10 {
                    Color::White.paint(format!("{:.0}", n)).to_string()
                } else {
                    Color::White.paint(format!("{}", n)).to_string()
                }
            }
            molang::eval::Value::String(s) => Color::Green.paint(format!("\"{}\"", s)).to_string(),
            molang::eval::Value::Array(arr) => {
                Color::Yellow.paint(format!("[{} items]", arr.len())).to_string()
            }
            molang::eval::Value::Struct(map) => {
                Color::Magenta.paint(format!("{{{}  fields}}", map.len())).to_string()
            }
            molang::eval::Value::Null => Color::DarkGray.paint("null").to_string(),
        };

        println!("  {} = {}", Color::Blue.paint(name), value_str);
    }
    println!();
}
