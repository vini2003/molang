pub mod ast;
pub mod builtins;
pub mod eval;
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
}

/// Entry point for host code: lex/parse a Molang snippet and compile to native code via
/// Cranelift JIT. Pure expressions are cached; programs are compiled on demand.
pub fn evaluate_expression(input: &str, ctx: &mut RuntimeContext) -> Result<f64, MolangError> {
    let tokens = lexer::lex(input)?;
    let mut parser = parser::Parser::new(&tokens);
    let program = parser.parse_program()?;
    let builder = IrBuilder::default();
    if let Some(expr) = program.as_jit_expression() {
        let ir = builder.lower(expr)?;
        let compiled = jit_cache::compile_cached(input, &ir)?;
        compiled.evaluate(ctx).map_err(MolangError::from)
    } else {
        let ir_program = builder.lower_program(&program)?;
        let compiled = jit::compile_program(&ir_program)?;
        compiled.evaluate(ctx).map_err(MolangError::from)
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

    #[test]
    fn struct_literals_and_nested_assignment() {
        let value = eval(
            "
            temp.location = { x: 1, y: 2 };
            temp.location.z = 3;
            return temp.location.x + temp.location.y + temp.location.z;
            ",
        );
        assert!((value - 6.0).abs() < 1e-9);
    }

    #[test]
    fn query_bindings_work() {
        let mut ctx = RuntimeContext::default()
            .with_query("speed", 2.5)
            .with_query("height", -1.5);
        let jit_value =
            evaluate_expression("return query.speed + math.abs(query.height);", &mut ctx).unwrap();
        assert!((jit_value - 4.0).abs() < 1e-9);

        let mut ctx = RuntimeContext::default().with_query("offset", -5.0);
        let interp_value = evaluate_expression(
            "
            temp.base = 10;
            return temp.base + query.offset;
            ",
            &mut ctx,
        )
        .unwrap();
        assert!((interp_value - 5.0).abs() < 1e-9);
    }

    #[test]
    fn query_strings_work() {
        let mut ctx = RuntimeContext::default()
            .with_query_string("name", "player")
            .with_query_string("message", "hello");
        let value = evaluate_expression(
            "
            temp.result = query.name;
            return 1.0;
            ",
            &mut ctx,
        )
        .unwrap();
        assert!((value - 1.0).abs() < 1e-9);

        // Verify string was stored
        let stored = ctx.get_value_canonical("query.name");
        assert!(matches!(stored, Some(Value::String(_))));
    }

    #[test]
    fn query_arrays_work() {
        let array_value = Value::array(vec![
            Value::number(1.0),
            Value::number(2.0),
            Value::number(3.0),
        ]);
        let mut ctx = RuntimeContext::default().with_query_value("items", array_value);

        let value = evaluate_expression("return query.items.length;", &mut ctx).unwrap();
        assert!((value - 3.0).abs() < 1e-9);
    }

    #[test]
    fn query_structs_work() {
        use indexmap::IndexMap;
        let mut map = IndexMap::new();
        map.insert("x".to_string(), Value::number(10.0));
        map.insert("y".to_string(), Value::number(20.0));
        let struct_value = Value::Struct(map);

        let mut ctx = RuntimeContext::default().with_query_value("position", struct_value);
        let value = evaluate_expression(
            "
            temp.sum = query.position.x + query.position.y;
            return temp.sum;
            ",
            &mut ctx,
        )
        .unwrap();
        assert!((value - 30.0).abs() < 1e-9);
    }

    #[test]
    fn query_mixed_types() {
        let mut ctx = RuntimeContext::default()
            .with_query("speed", 5.0)
            .with_query_string("mode", "fast")
            .with_query_value(
                "data",
                Value::array(vec![Value::number(1.0), Value::number(2.0)]),
            );

        let value = evaluate_expression(
            "
            temp.result = query.speed + query.data.length;
            return temp.result;
            ",
            &mut ctx,
        )
        .unwrap();
        assert!((value - 7.0).abs() < 1e-9);

        // Verify all types are accessible
        assert!(matches!(ctx.get_value_canonical("query.speed"), Some(Value::Number(_))));
        assert!(matches!(ctx.get_value_canonical("query.mode"), Some(Value::String(_))));
        assert!(matches!(ctx.get_value_canonical("query.data"), Some(Value::Array(_))));
    }

    #[test]
    fn query_mutation_after_creation() {
        let mut ctx = RuntimeContext::default().with_query("value", 10.0);

        // Initial value
        let result = evaluate_expression("return query.value;", &mut ctx).unwrap();
        assert!((result - 10.0).abs() < 1e-9);

        // Mutate after creation
        ctx.set_query_value("value", 20.0);
        let result = evaluate_expression("return query.value;", &mut ctx).unwrap();
        assert!((result - 20.0).abs() < 1e-9);

        // Add new query values dynamically
        ctx.set_query_string("type", "dynamic");
        let stored = ctx.get_value_canonical("query.type");
        assert!(matches!(stored, Some(Value::String(_))));
    }

    #[test]
    fn array_indexing_and_length() {
        let value = eval(
            "
            temp.values = [10, 20, 30];
            temp.index = 1;
            temp.sum = temp.values[temp.index] + temp.values[3] + temp.values.length;
            return temp.sum;
            ",
        );
        assert!((value - 33.0).abs() < 1e-9);
    }

    #[test]
    fn trigonometric_functions() {
        // Test acos, asin, atan
        let acos_result = eval("return math.acos(0);");
        assert!((acos_result - 90.0).abs() < 1e-6);

        let asin_result = eval("return math.asin(1);");
        assert!((asin_result - 90.0).abs() < 1e-6);

        let atan_result = eval("return math.atan(1);");
        assert!((atan_result - 45.0).abs() < 1e-6);

        // Test atan2
        let atan2_result = eval("return math.atan2(1, 1);");
        assert!((atan2_result - 45.0).abs() < 1e-6);
    }

    #[test]
    fn exponential_and_logarithmic_functions() {
        // Test exp
        let exp_result = eval("return math.exp(0);");
        assert!((exp_result - 1.0).abs() < 1e-9);

        // Test ln
        let ln_result = eval("return math.ln(2.718281828459045);");
        assert!((ln_result - 1.0).abs() < 1e-6);

        // Test pow
        let pow_result = eval("return math.pow(2, 3);");
        assert!((pow_result - 8.0).abs() < 1e-9);
    }

    #[test]
    fn basic_arithmetic_functions() {
        // Test max and min
        let max_result = eval("return math.max(5, 10);");
        assert!((max_result - 10.0).abs() < 1e-9);

        let min_result = eval("return math.min(5, 10);");
        assert!((min_result - 5.0).abs() < 1e-9);

        // Test mod
        let mod_result = eval("return math.mod(10, 3);");
        assert!((mod_result - 1.0).abs() < 1e-9);

        // Test sign
        let sign_pos = eval("return math.sign(5);");
        assert!((sign_pos - 1.0).abs() < 1e-9);

        let sign_neg = eval("return math.sign(-5);");
        assert!((sign_neg - (-1.0)).abs() < 1e-9);

        // Test copy_sign
        let copy_sign_result = eval("return math.copy_sign(10, -1);");
        assert!((copy_sign_result - (-10.0)).abs() < 1e-9);

        // Test pi
        let pi_result = eval("return math.pi();");
        assert!((pi_result - std::f64::consts::PI).abs() < 1e-9);
    }

    #[test]
    fn angle_functions() {
        // Test min_angle
        let angle1 = eval("return math.min_angle(190);");
        assert!((angle1 - (-170.0)).abs() < 1e-9);

        let angle2 = eval("return math.min_angle(-200);");
        assert!((angle2 - 160.0).abs() < 1e-9);

        let angle3 = eval("return math.min_angle(45);");
        assert!((angle3 - 45.0).abs() < 1e-9);
    }

    #[test]
    fn interpolation_functions() {
        // Test lerp
        let lerp_result = eval("return math.lerp(0, 10, 0.5);");
        assert!((lerp_result - 5.0).abs() < 1e-9);

        // Test inverse_lerp
        let inverse_lerp_result = eval("return math.inverse_lerp(0, 10, 5);");
        assert!((inverse_lerp_result - 0.5).abs() < 1e-9);

        // Test lerprotate
        let lerprotate_result = eval("return math.lerprotate(10, 350, 0.5);");
        assert!((lerprotate_result - 0.0).abs() < 1e-6);

        // Test hermite_blend
        let hermite_result = eval("return math.hermite_blend(0.5);");
        assert!((hermite_result - 0.5).abs() < 1e-9);
    }

    #[test]
    fn die_roll_functions() {
        // Test that die_roll returns a value in the expected range
        let die_roll_result = eval("return math.die_roll(3, 1, 6);");
        assert!(die_roll_result >= 3.0 && die_roll_result <= 18.0);

        // Test that die_roll_integer returns an integer value
        let die_roll_int_result = eval("return math.die_roll_integer(2, 1, 6);");
        assert!(die_roll_int_result >= 2.0 && die_roll_int_result <= 12.0);
        assert!((die_roll_int_result - die_roll_int_result.floor()).abs() < 1e-9);
    }

    #[test]
    fn easing_functions_quad() {
        // Test quadratic easing
        let ease_in = eval("return math.ease_in_quad(0, 10, 0.5);");
        assert!((ease_in - 2.5).abs() < 1e-9);

        let ease_out = eval("return math.ease_out_quad(0, 10, 0.5);");
        assert!((ease_out - 7.5).abs() < 1e-9);

        let ease_in_out = eval("return math.ease_in_out_quad(0, 10, 0.5);");
        assert!((ease_in_out - 5.0).abs() < 1e-9);
    }

    #[test]
    fn easing_functions_cubic() {
        // Test cubic easing at boundaries
        let ease_in_start = eval("return math.ease_in_cubic(0, 10, 0);");
        assert!((ease_in_start - 0.0).abs() < 1e-9);

        let ease_in_end = eval("return math.ease_in_cubic(0, 10, 1);");
        assert!((ease_in_end - 10.0).abs() < 1e-9);

        let ease_out_start = eval("return math.ease_out_cubic(0, 10, 0);");
        assert!((ease_out_start - 0.0).abs() < 1e-9);

        let ease_out_end = eval("return math.ease_out_cubic(0, 10, 1);");
        assert!((ease_out_end - 10.0).abs() < 1e-9);
    }

    #[test]
    fn easing_functions_sine() {
        // Test sine easing
        let ease_in_sine = eval("return math.ease_in_sine(0, 10, 1);");
        assert!((ease_in_sine - 10.0).abs() < 1e-6);

        let ease_out_sine = eval("return math.ease_out_sine(0, 10, 0);");
        assert!((ease_out_sine - 0.0).abs() < 1e-9);

        let ease_in_out_sine = eval("return math.ease_in_out_sine(0, 10, 0.5);");
        assert!((ease_in_out_sine - 5.0).abs() < 1e-9);
    }

    #[test]
    fn easing_functions_circular() {
        // Test circular easing at boundaries
        let ease_in = eval("return math.ease_in_circ(0, 10, 0);");
        assert!((ease_in - 0.0).abs() < 1e-9);

        let ease_out = eval("return math.ease_out_circ(0, 10, 1);");
        assert!((ease_out - 10.0).abs() < 1e-6);
    }

    #[test]
    fn easing_functions_back() {
        // Test back easing - should overshoot
        let ease_in_back = eval("return math.ease_in_back(0, 10, 1);");
        assert!((ease_in_back - 10.0).abs() < 1e-6);

        let ease_out_back = eval("return math.ease_out_back(0, 10, 1);");
        assert!((ease_out_back - 10.0).abs() < 1e-6);
    }

    #[test]
    fn string_comparison_equal() {
        // String literals comparison
        let result = eval("return 'hello' == 'hello';");
        assert!((result - 1.0).abs() < 1e-9);

        let result = eval("return 'hello' == 'world';");
        assert!((result - 0.0).abs() < 1e-9);

        // String variable comparison
        let result = eval("temp.name = 'alice'; return temp.name == 'alice';");
        assert!((result - 1.0).abs() < 1e-9);

        let result = eval("temp.name = 'alice'; return temp.name == 'bob';");
        assert!((result - 0.0).abs() < 1e-9);

        // Two string variables
        let result = eval("temp.a = 'test'; temp.b = 'test'; return temp.a == temp.b;");
        assert!((result - 1.0).abs() < 1e-9);

        let result = eval("temp.a = 'foo'; temp.b = 'bar'; return temp.a == temp.b;");
        assert!((result - 0.0).abs() < 1e-9);
    }

    #[test]
    fn string_comparison_not_equal() {
        // String literals comparison
        let result = eval("return 'hello' != 'world';");
        assert!((result - 1.0).abs() < 1e-9);

        let result = eval("return 'hello' != 'hello';");
        assert!((result - 0.0).abs() < 1e-9);

        // String variable comparison
        let result = eval("temp.name = 'alice'; return temp.name != 'bob';");
        assert!((result - 1.0).abs() < 1e-9);

        let result = eval("temp.name = 'alice'; return temp.name != 'alice';");
        assert!((result - 0.0).abs() < 1e-9);

        // Two string variables
        let result = eval("temp.a = 'foo'; temp.b = 'bar'; return temp.a != temp.b;");
        assert!((result - 1.0).abs() < 1e-9);

        let result = eval("temp.a = 'test'; temp.b = 'test'; return temp.a != temp.b;");
        assert!((result - 0.0).abs() < 1e-9);
    }

    #[test]
    fn string_comparison_mixed_types() {
        // String compared with number should return false
        let result = eval("temp.name = 'alice'; temp.age = 25; return temp.name == temp.age;");
        assert!((result - 0.0).abs() < 1e-9);

        // String compared with null
        let result = eval("temp.name = 'alice'; return temp.name == temp.missing;");
        assert!((result - 0.0).abs() < 1e-9);
    }

    #[test]
    fn string_comparison_in_conditionals() {
        // Using string comparison in ternary
        let result = eval(
            "temp.role = 'admin'; return temp.role == 'admin' ? 1.0 : 0.0;"
        );
        assert!((result - 1.0).abs() < 1e-9);

        // Using in loops with conditional
        let result = eval(
            "
            temp.found = 0;
            temp.names = ['alice', 'bob', 'charlie'];
            for_each(temp.name, temp.names, {
                temp.found = temp.found + (temp.name == 'bob' ? 1 : 0);
            });
            return temp.found;
            "
        );
        assert!((result - 1.0).abs() < 1e-9);
    }

    #[test]
    fn number_comparison_still_works() {
        // Ensure numeric comparison still works
        let result = eval("temp.a = 5; temp.b = 5; return temp.a == temp.b;");
        assert!((result - 1.0).abs() < 1e-9);

        let result = eval("temp.a = 5; temp.b = 10; return temp.a != temp.b;");
        assert!((result - 1.0).abs() < 1e-9);

        // Numeric literals
        let result = eval("return 42 == 42;");
        assert!((result - 1.0).abs() < 1e-9);

        let result = eval("return 42 != 43;");
        assert!((result - 1.0).abs() < 1e-9);
    }

    #[test]
    fn all_easing_functions_preserve_boundaries() {
        // All easing functions should map 0 to start and 1 to end
        let functions = vec![
            "ease_in_quad", "ease_out_quad", "ease_in_out_quad",
            "ease_in_cubic", "ease_out_cubic", "ease_in_out_cubic",
            "ease_in_quart", "ease_out_quart", "ease_in_out_quart",
            "ease_in_quint", "ease_out_quint", "ease_in_out_quint",
            "ease_in_sine", "ease_out_sine", "ease_in_out_sine",
            "ease_in_expo", "ease_out_expo", "ease_in_out_expo",
            "ease_in_circ", "ease_out_circ", "ease_in_out_circ",
            "ease_in_back", "ease_out_back", "ease_in_out_back",
            "ease_in_elastic", "ease_out_elastic", "ease_in_out_elastic",
            "ease_in_bounce", "ease_out_bounce", "ease_in_out_bounce",
        ];

        for func in functions {
            let start_script = format!("return math.{}(5, 15, 0);", func);
            let end_script = format!("return math.{}(5, 15, 1);", func);

            let start_result = eval(&start_script);
            let end_result = eval(&end_script);

            assert!(
                (start_result - 5.0).abs() < 1e-6,
                "{} failed at t=0: expected 5.0, got {}",
                func,
                start_result
            );
            assert!(
                (end_result - 15.0).abs() < 1e-6,
                "{} failed at t=1: expected 15.0, got {}",
                func,
                end_result
            );
        }
    }
}
