pub mod ast;
pub mod builtins;
pub mod eval;
pub mod executor;
pub mod ir;
pub mod jit;
mod jit_cache;
pub mod lexer;
pub mod parser;

use crate::ir::IrBuilder;
use thiserror::Error;

pub use eval::{Namespace, RuntimeContext, Value};

#[derive(Debug, Error)]
pub enum MolangError {
    #[error(transparent)]
    Lex(#[from] lexer::LexError),
    #[error(transparent)]
    Parse(#[from] parser::ParseError),
    #[error(transparent)]
    Lower(#[from] ir::LowerError),
    #[error(transparent)]
    Jit(#[from] jit::JitError),
    #[error(transparent)]
    Exec(#[from] executor::ExecError),
}

/// Entry point for host code: lex/parse a Molang snippet, and dispatch either to the
/// Cranelift JIT (pure expression) or to the interpreter (statements/complex values).
pub fn evaluate_expression(input: &str, ctx: &mut RuntimeContext) -> Result<f64, MolangError> {
    let tokens = lexer::lex(input)?;
    let mut parser = parser::Parser::new(&tokens);
    let program = parser.parse_program()?;
    if let Some(expr) = program.as_jit_expression() {
        let builder = IrBuilder::default();
        let ir = builder.lower(expr)?;
        let compiled = jit_cache::compile_cached(input, &ir)?;
        compiled.evaluate(ctx).map_err(MolangError::from)
    } else {
        let mut executor = executor::Executor::default();
        let value = executor.execute(&program, ctx)?;
        Ok(value.as_number())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{eval::RuntimeContext, jit_cache};

    #[test]
    fn evaluates_simple_expression() {
        let mut ctx = RuntimeContext::default();
        let result =
            evaluate_expression("1 + math.cos(37)", &mut ctx).expect("evaluation should succeed");
        assert!((result - (1.0 + 37f64.cos())).abs() < 1e-9);
    }

    #[test]
    fn evaluates_conditionals_and_logical_ops() {
        let mut ctx = RuntimeContext::default();
        let ternary =
            evaluate_expression("(1 < 2) ? 5.0 : 10.0", &mut ctx).expect("ternary should work");
        assert!((ternary - 5.0).abs() < 1e-9);

        let null_coalesce =
            evaluate_expression("0 ?? 3 + 2", &mut ctx).expect("null coalesce should work");
        assert!((null_coalesce - 5.0).abs() < 1e-9);

        let logical = evaluate_expression("!(1 - 1) || (2 > 1) && (3 == 3)", &mut ctx)
            .expect("logical operations should work");
        assert!((logical - 1.0).abs() < 1e-9);
    }

    #[test]
    fn executes_loop_and_breaks() {
        let mut ctx = RuntimeContext::default();
        let script = "
            temp.counter = 0;
            loop(10, {
                temp.counter = temp.counter + 1;
                (temp.counter > 5) ? break;
            });
            return temp.counter;
        ";
        let value = evaluate_expression(script, &mut ctx).expect("loop should execute");
        assert!((value - 6.0).abs() < 1e-9);
    }

    #[test]
    fn for_each_accumulates_values() {
        let mut ctx = RuntimeContext::default();
        let script = "
            temp.values = [1, 2, 3, 4];
            temp.total = 0;
            for_each(temp.item, temp.values, {
                temp.total = temp.total + temp.item;
            });
            return temp.total;
        ";
        let value = evaluate_expression(script, &mut ctx).expect("for_each should execute");
        assert!((value - 10.0).abs() < 1e-9);
    }

    #[test]
    fn jit_compiled_expressions_are_cached() {
        jit_cache::clear_cache();
        let mut ctx = RuntimeContext::default();
        let expr = "1 + math.cos(0)";
        evaluate_expression(expr, &mut ctx).expect("first evaluation");
        assert_eq!(jit_cache::cache_size(), 1);
        evaluate_expression(expr, &mut ctx).expect("second evaluation reuses cache");
        assert_eq!(jit_cache::cache_size(), 1);
    }

    fn eval(script: &str) -> f64 {
        let mut ctx = RuntimeContext::default();
        evaluate_expression(script, &mut ctx).expect("script evaluation to succeed")
    }

    #[test]
    fn string_and_array_literals() {
        let value = eval("return ['a', 'b', 'c'];");
        assert!((value - 3.0).abs() < 1e-9);

        let value = eval("return (temp.missing ?? 5);");
        assert!((value - 5.0).abs() < 1e-9);
    }

    #[test]
    fn continue_skips_iteration() {
        let value = eval(
            "
            temp.sum = 0;
            loop(5, {
                temp.index = temp.index ?? 0;
                temp.index = temp.index + 1;
                (temp.index == 3) ? continue;
                temp.sum = temp.sum + temp.index;
            });
            return temp.sum;
            ",
        );
        assert!((value - 12.0).abs() < 1e-9); // skips adding 3
    }

    #[test]
    fn for_each_breaks_early() {
        let value = eval(
            "
            temp.values = [2, 4, 6, 8];
            temp.total = 0;
            for_each(temp.item, temp.values, {
                temp.total = temp.total + temp.item;
                (temp.item >= 6) ? break;
            });
            return temp.total;
            ",
        );
        assert!((value - 12.0).abs() < 1e-9);
    }

    #[test]
    fn math_helpers() {
        let clamp = eval("return math.clamp(-2, 0, 10);");
        assert!((clamp - 0.0).abs() < 1e-9);

        let sqrt = eval("return math.sqrt(9);");
        assert!((sqrt - 3.0).abs() < 1e-9);

        let round =
            eval("return math.round(2.4) + math.ceil(2.1) + math.floor(2.9) + math.trunc(-2.8);");
        assert!((round - 5.0).abs() < 1e-9);
    }
}
