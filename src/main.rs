use molang::{evaluate_expression, RuntimeContext};

fn main() {
    let expression = std::env::args().skip(1).collect::<Vec<_>>().join(" ");
    if expression.is_empty() {
        eprintln!("Usage: molang \"<expression>\"");
        std::process::exit(1);
    }

    let mut ctx = RuntimeContext::default();
    match evaluate_expression(&expression, &mut ctx) {
        Ok(value) => println!("{value}"),
        Err(err) => {
            eprintln!("Error: {err}");
            std::process::exit(1);
        }
    }
}
