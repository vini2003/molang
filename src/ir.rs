use crate::ast::{BinaryOp, ControlFlowExpr, Expr, Program, Statement, UnaryOp};
use indexmap::IndexMap;
use thiserror::Error;

/// Expression IR that can be fed directly to the Cranelift JIT.
#[derive(Debug, Clone)]
pub enum IrExpr {
    Constant(f64),
    Path(Vec<String>),
    String(String),
    Array(Vec<IrExpr>),
    Struct(IndexMap<String, IrExpr>),
    Unary {
        op: UnaryOp,
        expr: Box<IrExpr>,
    },
    Binary {
        op: BinaryOp,
        left: Box<IrExpr>,
        right: Box<IrExpr>,
    },
    Conditional {
        condition: Box<IrExpr>,
        then_branch: Box<IrExpr>,
        else_branch: Option<Box<IrExpr>>,
    },
    Call {
        function: FunctionRef,
        args: Vec<IrExpr>,
    },
    Index {
        target: Box<IrExpr>,
        index: Box<IrExpr>,
    },
    Flow(ControlFlowExpr),
}

/// Statement-level IR compiled to native code via the JIT.
#[derive(Debug, Clone)]
pub enum IrStatement {
    Assign {
        target: Vec<String>,
        value: IrExpr,
    },
    Block(Vec<IrStatement>),
    Loop {
        count: IrExpr,
        body: Box<IrStatement>,
    },
    ForEach {
        variable: Vec<String>,
        collection: IrExpr,
        body: Box<IrStatement>,
    },
    Return(Option<IrExpr>),
    Expr(IrExpr),
}

#[derive(Debug, Clone)]
pub struct IrProgram {
    pub statements: Vec<IrStatement>,
}

#[derive(Debug, Clone, Copy)]
pub enum FunctionRef {
    Builtin(BuiltinFunction),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuiltinFunction {
    MathCos,
    MathSin,
    MathAbs,
    MathRandom,
    MathRandomInteger,
    MathClamp,
    MathSqrt,
    MathFloor,
    MathCeil,
    MathRound,
    MathTrunc,
    MathAcos,
    MathAsin,
    MathAtan,
    MathAtan2,
    MathExp,
    MathLn,
    MathPow,
    MathMax,
    MathMin,
    MathMod,
    MathSign,
    MathCopySign,
    MathPi,
    MathMinAngle,
    MathLerp,
    MathInverseLerp,
    MathLerpRotate,
    MathHermiteBlend,
    MathDieRoll,
    MathDieRollInteger,
    MathEaseInQuad,
    MathEaseOutQuad,
    MathEaseInOutQuad,
    MathEaseInCubic,
    MathEaseOutCubic,
    MathEaseInOutCubic,
    MathEaseInQuart,
    MathEaseOutQuart,
    MathEaseInOutQuart,
    MathEaseInQuint,
    MathEaseOutQuint,
    MathEaseInOutQuint,
    MathEaseInSine,
    MathEaseOutSine,
    MathEaseInOutSine,
    MathEaseInExpo,
    MathEaseOutExpo,
    MathEaseInOutExpo,
    MathEaseInCirc,
    MathEaseOutCirc,
    MathEaseInOutCirc,
    MathEaseInBack,
    MathEaseOutBack,
    MathEaseInOutBack,
    MathEaseInElastic,
    MathEaseOutElastic,
    MathEaseInOutElastic,
    MathEaseInBounce,
    MathEaseOutBounce,
    MathEaseInOutBounce,
}

impl BuiltinFunction {
    pub fn from_path(path: &[String]) -> Option<Self> {
        match path {
            [ns, func] if ns == "math" => match func.as_str() {
                "cos" => Some(BuiltinFunction::MathCos),
                "sin" => Some(BuiltinFunction::MathSin),
                "abs" => Some(BuiltinFunction::MathAbs),
                "random" => Some(BuiltinFunction::MathRandom),
                "random_integer" => Some(BuiltinFunction::MathRandomInteger),
                "clamp" => Some(BuiltinFunction::MathClamp),
                "sqrt" => Some(BuiltinFunction::MathSqrt),
                "floor" => Some(BuiltinFunction::MathFloor),
                "ceil" => Some(BuiltinFunction::MathCeil),
                "round" => Some(BuiltinFunction::MathRound),
                "trunc" => Some(BuiltinFunction::MathTrunc),
                "acos" => Some(BuiltinFunction::MathAcos),
                "asin" => Some(BuiltinFunction::MathAsin),
                "atan" => Some(BuiltinFunction::MathAtan),
                "atan2" => Some(BuiltinFunction::MathAtan2),
                "exp" => Some(BuiltinFunction::MathExp),
                "ln" => Some(BuiltinFunction::MathLn),
                "pow" => Some(BuiltinFunction::MathPow),
                "max" => Some(BuiltinFunction::MathMax),
                "min" => Some(BuiltinFunction::MathMin),
                "mod" => Some(BuiltinFunction::MathMod),
                "sign" => Some(BuiltinFunction::MathSign),
                "copy_sign" => Some(BuiltinFunction::MathCopySign),
                "pi" => Some(BuiltinFunction::MathPi),
                "min_angle" => Some(BuiltinFunction::MathMinAngle),
                "lerp" => Some(BuiltinFunction::MathLerp),
                "inverse_lerp" => Some(BuiltinFunction::MathInverseLerp),
                "lerprotate" => Some(BuiltinFunction::MathLerpRotate),
                "hermite_blend" => Some(BuiltinFunction::MathHermiteBlend),
                "die_roll" => Some(BuiltinFunction::MathDieRoll),
                "die_roll_integer" => Some(BuiltinFunction::MathDieRollInteger),
                "ease_in_quad" => Some(BuiltinFunction::MathEaseInQuad),
                "ease_out_quad" => Some(BuiltinFunction::MathEaseOutQuad),
                "ease_in_out_quad" => Some(BuiltinFunction::MathEaseInOutQuad),
                "ease_in_cubic" => Some(BuiltinFunction::MathEaseInCubic),
                "ease_out_cubic" => Some(BuiltinFunction::MathEaseOutCubic),
                "ease_in_out_cubic" => Some(BuiltinFunction::MathEaseInOutCubic),
                "ease_in_quart" => Some(BuiltinFunction::MathEaseInQuart),
                "ease_out_quart" => Some(BuiltinFunction::MathEaseOutQuart),
                "ease_in_out_quart" => Some(BuiltinFunction::MathEaseInOutQuart),
                "ease_in_quint" => Some(BuiltinFunction::MathEaseInQuint),
                "ease_out_quint" => Some(BuiltinFunction::MathEaseOutQuint),
                "ease_in_out_quint" => Some(BuiltinFunction::MathEaseInOutQuint),
                "ease_in_sine" => Some(BuiltinFunction::MathEaseInSine),
                "ease_out_sine" => Some(BuiltinFunction::MathEaseOutSine),
                "ease_in_out_sine" => Some(BuiltinFunction::MathEaseInOutSine),
                "ease_in_expo" => Some(BuiltinFunction::MathEaseInExpo),
                "ease_out_expo" => Some(BuiltinFunction::MathEaseOutExpo),
                "ease_in_out_expo" => Some(BuiltinFunction::MathEaseInOutExpo),
                "ease_in_circ" => Some(BuiltinFunction::MathEaseInCirc),
                "ease_out_circ" => Some(BuiltinFunction::MathEaseOutCirc),
                "ease_in_out_circ" => Some(BuiltinFunction::MathEaseInOutCirc),
                "ease_in_back" => Some(BuiltinFunction::MathEaseInBack),
                "ease_out_back" => Some(BuiltinFunction::MathEaseOutBack),
                "ease_in_out_back" => Some(BuiltinFunction::MathEaseInOutBack),
                "ease_in_elastic" => Some(BuiltinFunction::MathEaseInElastic),
                "ease_out_elastic" => Some(BuiltinFunction::MathEaseOutElastic),
                "ease_in_out_elastic" => Some(BuiltinFunction::MathEaseInOutElastic),
                "ease_in_bounce" => Some(BuiltinFunction::MathEaseInBounce),
                "ease_out_bounce" => Some(BuiltinFunction::MathEaseOutBounce),
                "ease_in_out_bounce" => Some(BuiltinFunction::MathEaseInOutBounce),
                _ => None,
            },
            _ => None,
        }
    }

    pub fn arity(self) -> usize {
        match self {
            BuiltinFunction::MathPi => 0,
            BuiltinFunction::MathCos
            | BuiltinFunction::MathSin
            | BuiltinFunction::MathAbs
            | BuiltinFunction::MathSqrt
            | BuiltinFunction::MathFloor
            | BuiltinFunction::MathCeil
            | BuiltinFunction::MathRound
            | BuiltinFunction::MathTrunc
            | BuiltinFunction::MathAcos
            | BuiltinFunction::MathAsin
            | BuiltinFunction::MathAtan
            | BuiltinFunction::MathExp
            | BuiltinFunction::MathLn
            | BuiltinFunction::MathSign
            | BuiltinFunction::MathMinAngle
            | BuiltinFunction::MathHermiteBlend => 1,
            BuiltinFunction::MathRandom
            | BuiltinFunction::MathRandomInteger
            | BuiltinFunction::MathAtan2
            | BuiltinFunction::MathPow
            | BuiltinFunction::MathMax
            | BuiltinFunction::MathMin
            | BuiltinFunction::MathMod
            | BuiltinFunction::MathCopySign => 2,
            BuiltinFunction::MathClamp
            | BuiltinFunction::MathLerp
            | BuiltinFunction::MathInverseLerp
            | BuiltinFunction::MathLerpRotate
            | BuiltinFunction::MathDieRoll
            | BuiltinFunction::MathDieRollInteger
            | BuiltinFunction::MathEaseInQuad
            | BuiltinFunction::MathEaseOutQuad
            | BuiltinFunction::MathEaseInOutQuad
            | BuiltinFunction::MathEaseInCubic
            | BuiltinFunction::MathEaseOutCubic
            | BuiltinFunction::MathEaseInOutCubic
            | BuiltinFunction::MathEaseInQuart
            | BuiltinFunction::MathEaseOutQuart
            | BuiltinFunction::MathEaseInOutQuart
            | BuiltinFunction::MathEaseInQuint
            | BuiltinFunction::MathEaseOutQuint
            | BuiltinFunction::MathEaseInOutQuint
            | BuiltinFunction::MathEaseInSine
            | BuiltinFunction::MathEaseOutSine
            | BuiltinFunction::MathEaseInOutSine
            | BuiltinFunction::MathEaseInExpo
            | BuiltinFunction::MathEaseOutExpo
            | BuiltinFunction::MathEaseInOutExpo
            | BuiltinFunction::MathEaseInCirc
            | BuiltinFunction::MathEaseOutCirc
            | BuiltinFunction::MathEaseInOutCirc
            | BuiltinFunction::MathEaseInBack
            | BuiltinFunction::MathEaseOutBack
            | BuiltinFunction::MathEaseInOutBack
            | BuiltinFunction::MathEaseInElastic
            | BuiltinFunction::MathEaseOutElastic
            | BuiltinFunction::MathEaseInOutElastic
            | BuiltinFunction::MathEaseInBounce
            | BuiltinFunction::MathEaseOutBounce
            | BuiltinFunction::MathEaseInOutBounce => 3,
        }
    }

    pub fn symbol_name(self) -> &'static str {
        match self {
            BuiltinFunction::MathCos => "builtin_math_cos",
            BuiltinFunction::MathSin => "builtin_math_sin",
            BuiltinFunction::MathAbs => "builtin_math_abs",
            BuiltinFunction::MathRandom => "builtin_math_random",
            BuiltinFunction::MathRandomInteger => "builtin_math_random_integer",
            BuiltinFunction::MathClamp => "builtin_math_clamp",
            BuiltinFunction::MathSqrt => "builtin_math_sqrt",
            BuiltinFunction::MathFloor => "builtin_math_floor",
            BuiltinFunction::MathCeil => "builtin_math_ceil",
            BuiltinFunction::MathRound => "builtin_math_round",
            BuiltinFunction::MathTrunc => "builtin_math_trunc",
            BuiltinFunction::MathAcos => "builtin_math_acos",
            BuiltinFunction::MathAsin => "builtin_math_asin",
            BuiltinFunction::MathAtan => "builtin_math_atan",
            BuiltinFunction::MathAtan2 => "builtin_math_atan2",
            BuiltinFunction::MathExp => "builtin_math_exp",
            BuiltinFunction::MathLn => "builtin_math_ln",
            BuiltinFunction::MathPow => "builtin_math_pow",
            BuiltinFunction::MathMax => "builtin_math_max",
            BuiltinFunction::MathMin => "builtin_math_min",
            BuiltinFunction::MathMod => "builtin_math_mod",
            BuiltinFunction::MathSign => "builtin_math_sign",
            BuiltinFunction::MathCopySign => "builtin_math_copy_sign",
            BuiltinFunction::MathPi => "builtin_math_pi",
            BuiltinFunction::MathMinAngle => "builtin_math_min_angle",
            BuiltinFunction::MathLerp => "builtin_math_lerp",
            BuiltinFunction::MathInverseLerp => "builtin_math_inverse_lerp",
            BuiltinFunction::MathLerpRotate => "builtin_math_lerprotate",
            BuiltinFunction::MathHermiteBlend => "builtin_math_hermite_blend",
            BuiltinFunction::MathDieRoll => "builtin_math_die_roll",
            BuiltinFunction::MathDieRollInteger => "builtin_math_die_roll_integer",
            BuiltinFunction::MathEaseInQuad => "builtin_math_ease_in_quad",
            BuiltinFunction::MathEaseOutQuad => "builtin_math_ease_out_quad",
            BuiltinFunction::MathEaseInOutQuad => "builtin_math_ease_in_out_quad",
            BuiltinFunction::MathEaseInCubic => "builtin_math_ease_in_cubic",
            BuiltinFunction::MathEaseOutCubic => "builtin_math_ease_out_cubic",
            BuiltinFunction::MathEaseInOutCubic => "builtin_math_ease_in_out_cubic",
            BuiltinFunction::MathEaseInQuart => "builtin_math_ease_in_quart",
            BuiltinFunction::MathEaseOutQuart => "builtin_math_ease_out_quart",
            BuiltinFunction::MathEaseInOutQuart => "builtin_math_ease_in_out_quart",
            BuiltinFunction::MathEaseInQuint => "builtin_math_ease_in_quint",
            BuiltinFunction::MathEaseOutQuint => "builtin_math_ease_out_quint",
            BuiltinFunction::MathEaseInOutQuint => "builtin_math_ease_in_out_quint",
            BuiltinFunction::MathEaseInSine => "builtin_math_ease_in_sine",
            BuiltinFunction::MathEaseOutSine => "builtin_math_ease_out_sine",
            BuiltinFunction::MathEaseInOutSine => "builtin_math_ease_in_out_sine",
            BuiltinFunction::MathEaseInExpo => "builtin_math_ease_in_expo",
            BuiltinFunction::MathEaseOutExpo => "builtin_math_ease_out_expo",
            BuiltinFunction::MathEaseInOutExpo => "builtin_math_ease_in_out_expo",
            BuiltinFunction::MathEaseInCirc => "builtin_math_ease_in_circ",
            BuiltinFunction::MathEaseOutCirc => "builtin_math_ease_out_circ",
            BuiltinFunction::MathEaseInOutCirc => "builtin_math_ease_in_out_circ",
            BuiltinFunction::MathEaseInBack => "builtin_math_ease_in_back",
            BuiltinFunction::MathEaseOutBack => "builtin_math_ease_out_back",
            BuiltinFunction::MathEaseInOutBack => "builtin_math_ease_in_out_back",
            BuiltinFunction::MathEaseInElastic => "builtin_math_ease_in_elastic",
            BuiltinFunction::MathEaseOutElastic => "builtin_math_ease_out_elastic",
            BuiltinFunction::MathEaseInOutElastic => "builtin_math_ease_in_out_elastic",
            BuiltinFunction::MathEaseInBounce => "builtin_math_ease_in_bounce",
            BuiltinFunction::MathEaseOutBounce => "builtin_math_ease_out_bounce",
            BuiltinFunction::MathEaseInOutBounce => "builtin_math_ease_in_out_bounce",
        }
    }

    pub fn evaluate(self, args: &[f64]) -> f64 {
        match self {
            BuiltinFunction::MathCos => {
                crate::builtins::builtin_math_cos(args.first().copied().unwrap_or(0.0))
            }
            BuiltinFunction::MathSin => {
                crate::builtins::builtin_math_sin(args.first().copied().unwrap_or(0.0))
            }
            BuiltinFunction::MathAbs => {
                crate::builtins::builtin_math_abs(args.first().copied().unwrap_or(0.0))
            }
            BuiltinFunction::MathRandom => crate::builtins::math_random(
                args.get(0).copied().unwrap_or(0.0),
                args.get(1).copied().unwrap_or(1.0),
            ),
            BuiltinFunction::MathRandomInteger => crate::builtins::math_random_integer(
                args.get(0).copied().unwrap_or(0.0),
                args.get(1).copied().unwrap_or(1.0),
            ),
            BuiltinFunction::MathClamp => crate::builtins::math_clamp(
                args.get(0).copied().unwrap_or(0.0),
                args.get(1).copied().unwrap_or(0.0),
                args.get(2).copied().unwrap_or(0.0),
            ),
            BuiltinFunction::MathSqrt => {
                crate::builtins::builtin_math_sqrt(args.first().copied().unwrap_or(0.0))
            }
            BuiltinFunction::MathFloor => {
                crate::builtins::builtin_math_floor(args.first().copied().unwrap_or(0.0))
            }
            BuiltinFunction::MathCeil => {
                crate::builtins::builtin_math_ceil(args.first().copied().unwrap_or(0.0))
            }
            BuiltinFunction::MathRound => {
                crate::builtins::builtin_math_round(args.first().copied().unwrap_or(0.0))
            }
            BuiltinFunction::MathTrunc => {
                crate::builtins::builtin_math_trunc(args.first().copied().unwrap_or(0.0))
            }
            BuiltinFunction::MathAcos => {
                crate::builtins::builtin_math_acos(args.first().copied().unwrap_or(0.0))
            }
            BuiltinFunction::MathAsin => {
                crate::builtins::builtin_math_asin(args.first().copied().unwrap_or(0.0))
            }
            BuiltinFunction::MathAtan => {
                crate::builtins::builtin_math_atan(args.first().copied().unwrap_or(0.0))
            }
            BuiltinFunction::MathAtan2 => crate::builtins::builtin_math_atan2(
                args.get(0).copied().unwrap_or(0.0),
                args.get(1).copied().unwrap_or(0.0),
            ),
            BuiltinFunction::MathExp => {
                crate::builtins::builtin_math_exp(args.first().copied().unwrap_or(0.0))
            }
            BuiltinFunction::MathLn => {
                crate::builtins::builtin_math_ln(args.first().copied().unwrap_or(0.0))
            }
            BuiltinFunction::MathPow => crate::builtins::builtin_math_pow(
                args.get(0).copied().unwrap_or(0.0),
                args.get(1).copied().unwrap_or(0.0),
            ),
            BuiltinFunction::MathMax => crate::builtins::builtin_math_max(
                args.get(0).copied().unwrap_or(0.0),
                args.get(1).copied().unwrap_or(0.0),
            ),
            BuiltinFunction::MathMin => crate::builtins::builtin_math_min(
                args.get(0).copied().unwrap_or(0.0),
                args.get(1).copied().unwrap_or(0.0),
            ),
            BuiltinFunction::MathMod => crate::builtins::builtin_math_mod(
                args.get(0).copied().unwrap_or(0.0),
                args.get(1).copied().unwrap_or(0.0),
            ),
            BuiltinFunction::MathSign => {
                crate::builtins::builtin_math_sign(args.first().copied().unwrap_or(0.0))
            }
            BuiltinFunction::MathCopySign => crate::builtins::builtin_math_copy_sign(
                args.get(0).copied().unwrap_or(0.0),
                args.get(1).copied().unwrap_or(0.0),
            ),
            BuiltinFunction::MathPi => crate::builtins::builtin_math_pi(),
            BuiltinFunction::MathMinAngle => {
                crate::builtins::builtin_math_min_angle(args.first().copied().unwrap_or(0.0))
            }
            BuiltinFunction::MathLerp => crate::builtins::builtin_math_lerp(
                args.get(0).copied().unwrap_or(0.0),
                args.get(1).copied().unwrap_or(0.0),
                args.get(2).copied().unwrap_or(0.0),
            ),
            BuiltinFunction::MathInverseLerp => crate::builtins::builtin_math_inverse_lerp(
                args.get(0).copied().unwrap_or(0.0),
                args.get(1).copied().unwrap_or(0.0),
                args.get(2).copied().unwrap_or(0.0),
            ),
            BuiltinFunction::MathLerpRotate => crate::builtins::builtin_math_lerprotate(
                args.get(0).copied().unwrap_or(0.0),
                args.get(1).copied().unwrap_or(0.0),
                args.get(2).copied().unwrap_or(0.0),
            ),
            BuiltinFunction::MathHermiteBlend => {
                crate::builtins::builtin_math_hermite_blend(args.first().copied().unwrap_or(0.0))
            }
            BuiltinFunction::MathDieRoll => crate::builtins::builtin_math_die_roll(
                args.get(0).copied().unwrap_or(0.0),
                args.get(1).copied().unwrap_or(0.0),
                args.get(2).copied().unwrap_or(0.0),
            ),
            BuiltinFunction::MathDieRollInteger => crate::builtins::builtin_math_die_roll_integer(
                args.get(0).copied().unwrap_or(0.0),
                args.get(1).copied().unwrap_or(0.0),
                args.get(2).copied().unwrap_or(0.0),
            ),
            BuiltinFunction::MathEaseInQuad => crate::builtins::builtin_math_ease_in_quad(
                args.get(0).copied().unwrap_or(0.0),
                args.get(1).copied().unwrap_or(0.0),
                args.get(2).copied().unwrap_or(0.0),
            ),
            BuiltinFunction::MathEaseOutQuad => crate::builtins::builtin_math_ease_out_quad(
                args.get(0).copied().unwrap_or(0.0),
                args.get(1).copied().unwrap_or(0.0),
                args.get(2).copied().unwrap_or(0.0),
            ),
            BuiltinFunction::MathEaseInOutQuad => crate::builtins::builtin_math_ease_in_out_quad(
                args.get(0).copied().unwrap_or(0.0),
                args.get(1).copied().unwrap_or(0.0),
                args.get(2).copied().unwrap_or(0.0),
            ),
            BuiltinFunction::MathEaseInCubic => crate::builtins::builtin_math_ease_in_cubic(
                args.get(0).copied().unwrap_or(0.0),
                args.get(1).copied().unwrap_or(0.0),
                args.get(2).copied().unwrap_or(0.0),
            ),
            BuiltinFunction::MathEaseOutCubic => crate::builtins::builtin_math_ease_out_cubic(
                args.get(0).copied().unwrap_or(0.0),
                args.get(1).copied().unwrap_or(0.0),
                args.get(2).copied().unwrap_or(0.0),
            ),
            BuiltinFunction::MathEaseInOutCubic => crate::builtins::builtin_math_ease_in_out_cubic(
                args.get(0).copied().unwrap_or(0.0),
                args.get(1).copied().unwrap_or(0.0),
                args.get(2).copied().unwrap_or(0.0),
            ),
            BuiltinFunction::MathEaseInQuart => crate::builtins::builtin_math_ease_in_quart(
                args.get(0).copied().unwrap_or(0.0),
                args.get(1).copied().unwrap_or(0.0),
                args.get(2).copied().unwrap_or(0.0),
            ),
            BuiltinFunction::MathEaseOutQuart => crate::builtins::builtin_math_ease_out_quart(
                args.get(0).copied().unwrap_or(0.0),
                args.get(1).copied().unwrap_or(0.0),
                args.get(2).copied().unwrap_or(0.0),
            ),
            BuiltinFunction::MathEaseInOutQuart => crate::builtins::builtin_math_ease_in_out_quart(
                args.get(0).copied().unwrap_or(0.0),
                args.get(1).copied().unwrap_or(0.0),
                args.get(2).copied().unwrap_or(0.0),
            ),
            BuiltinFunction::MathEaseInQuint => crate::builtins::builtin_math_ease_in_quint(
                args.get(0).copied().unwrap_or(0.0),
                args.get(1).copied().unwrap_or(0.0),
                args.get(2).copied().unwrap_or(0.0),
            ),
            BuiltinFunction::MathEaseOutQuint => crate::builtins::builtin_math_ease_out_quint(
                args.get(0).copied().unwrap_or(0.0),
                args.get(1).copied().unwrap_or(0.0),
                args.get(2).copied().unwrap_or(0.0),
            ),
            BuiltinFunction::MathEaseInOutQuint => crate::builtins::builtin_math_ease_in_out_quint(
                args.get(0).copied().unwrap_or(0.0),
                args.get(1).copied().unwrap_or(0.0),
                args.get(2).copied().unwrap_or(0.0),
            ),
            BuiltinFunction::MathEaseInSine => crate::builtins::builtin_math_ease_in_sine(
                args.get(0).copied().unwrap_or(0.0),
                args.get(1).copied().unwrap_or(0.0),
                args.get(2).copied().unwrap_or(0.0),
            ),
            BuiltinFunction::MathEaseOutSine => crate::builtins::builtin_math_ease_out_sine(
                args.get(0).copied().unwrap_or(0.0),
                args.get(1).copied().unwrap_or(0.0),
                args.get(2).copied().unwrap_or(0.0),
            ),
            BuiltinFunction::MathEaseInOutSine => crate::builtins::builtin_math_ease_in_out_sine(
                args.get(0).copied().unwrap_or(0.0),
                args.get(1).copied().unwrap_or(0.0),
                args.get(2).copied().unwrap_or(0.0),
            ),
            BuiltinFunction::MathEaseInExpo => crate::builtins::builtin_math_ease_in_expo(
                args.get(0).copied().unwrap_or(0.0),
                args.get(1).copied().unwrap_or(0.0),
                args.get(2).copied().unwrap_or(0.0),
            ),
            BuiltinFunction::MathEaseOutExpo => crate::builtins::builtin_math_ease_out_expo(
                args.get(0).copied().unwrap_or(0.0),
                args.get(1).copied().unwrap_or(0.0),
                args.get(2).copied().unwrap_or(0.0),
            ),
            BuiltinFunction::MathEaseInOutExpo => crate::builtins::builtin_math_ease_in_out_expo(
                args.get(0).copied().unwrap_or(0.0),
                args.get(1).copied().unwrap_or(0.0),
                args.get(2).copied().unwrap_or(0.0),
            ),
            BuiltinFunction::MathEaseInCirc => crate::builtins::builtin_math_ease_in_circ(
                args.get(0).copied().unwrap_or(0.0),
                args.get(1).copied().unwrap_or(0.0),
                args.get(2).copied().unwrap_or(0.0),
            ),
            BuiltinFunction::MathEaseOutCirc => crate::builtins::builtin_math_ease_out_circ(
                args.get(0).copied().unwrap_or(0.0),
                args.get(1).copied().unwrap_or(0.0),
                args.get(2).copied().unwrap_or(0.0),
            ),
            BuiltinFunction::MathEaseInOutCirc => crate::builtins::builtin_math_ease_in_out_circ(
                args.get(0).copied().unwrap_or(0.0),
                args.get(1).copied().unwrap_or(0.0),
                args.get(2).copied().unwrap_or(0.0),
            ),
            BuiltinFunction::MathEaseInBack => crate::builtins::builtin_math_ease_in_back(
                args.get(0).copied().unwrap_or(0.0),
                args.get(1).copied().unwrap_or(0.0),
                args.get(2).copied().unwrap_or(0.0),
            ),
            BuiltinFunction::MathEaseOutBack => crate::builtins::builtin_math_ease_out_back(
                args.get(0).copied().unwrap_or(0.0),
                args.get(1).copied().unwrap_or(0.0),
                args.get(2).copied().unwrap_or(0.0),
            ),
            BuiltinFunction::MathEaseInOutBack => crate::builtins::builtin_math_ease_in_out_back(
                args.get(0).copied().unwrap_or(0.0),
                args.get(1).copied().unwrap_or(0.0),
                args.get(2).copied().unwrap_or(0.0),
            ),
            BuiltinFunction::MathEaseInElastic => crate::builtins::builtin_math_ease_in_elastic(
                args.get(0).copied().unwrap_or(0.0),
                args.get(1).copied().unwrap_or(0.0),
                args.get(2).copied().unwrap_or(0.0),
            ),
            BuiltinFunction::MathEaseOutElastic => crate::builtins::builtin_math_ease_out_elastic(
                args.get(0).copied().unwrap_or(0.0),
                args.get(1).copied().unwrap_or(0.0),
                args.get(2).copied().unwrap_or(0.0),
            ),
            BuiltinFunction::MathEaseInOutElastic => {
                crate::builtins::builtin_math_ease_in_out_elastic(
                    args.get(0).copied().unwrap_or(0.0),
                    args.get(1).copied().unwrap_or(0.0),
                    args.get(2).copied().unwrap_or(0.0),
                )
            }
            BuiltinFunction::MathEaseInBounce => crate::builtins::builtin_math_ease_in_bounce(
                args.get(0).copied().unwrap_or(0.0),
                args.get(1).copied().unwrap_or(0.0),
                args.get(2).copied().unwrap_or(0.0),
            ),
            BuiltinFunction::MathEaseOutBounce => crate::builtins::builtin_math_ease_out_bounce(
                args.get(0).copied().unwrap_or(0.0),
                args.get(1).copied().unwrap_or(0.0),
                args.get(2).copied().unwrap_or(0.0),
            ),
            BuiltinFunction::MathEaseInOutBounce => {
                crate::builtins::builtin_math_ease_in_out_bounce(
                    args.get(0).copied().unwrap_or(0.0),
                    args.get(1).copied().unwrap_or(0.0),
                    args.get(2).copied().unwrap_or(0.0),
                )
            }
        }
    }
}

#[derive(Default)]
pub struct IrBuilder;

impl IrBuilder {
    /// Lowers a full AST program into statement-level IR.
    pub fn lower_program(&self, program: &Program) -> Result<IrProgram, LowerError> {
        let mut statements = Vec::new();
        for stmt in &program.statements {
            statements.push(self.lower_statement(stmt)?);
        }
        Ok(IrProgram { statements })
    }

    fn lower_statement(&self, statement: &Statement) -> Result<IrStatement, LowerError> {
        Ok(match statement {
            Statement::Expr(expr) => IrStatement::Expr(self.lower_expr(expr)?),
            Statement::Assignment { target, value } => IrStatement::Assign {
                target: target.clone(),
                value: self.lower_expr(value)?,
            },
            Statement::Block(list) => IrStatement::Block(
                list.iter()
                    .map(|stmt| self.lower_statement(stmt))
                    .collect::<Result<Vec<_>, _>>()?,
            ),
            Statement::Loop { count, body } => IrStatement::Loop {
                count: self.lower_expr(count)?,
                body: Box::new(self.lower_statement(body)?),
            },
            Statement::ForEach {
                variable,
                collection,
                body,
            } => IrStatement::ForEach {
                variable: variable.clone(),
                collection: self.lower_expr(collection)?,
                body: Box::new(self.lower_statement(body)?),
            },
            Statement::Return(expr) => IrStatement::Return(match expr {
                Some(expr) => Some(self.lower_expr(expr)?),
                None => None,
            }),
        })
    }

    pub fn lower(&self, expr: &Expr) -> Result<IrExpr, LowerError> {
        self.lower_expr(expr)
    }

    fn lower_expr(&self, expr: &Expr) -> Result<IrExpr, LowerError> {
        match expr {
            Expr::Number(value) => Ok(IrExpr::Constant(*value)),
            Expr::Path(parts) => Ok(IrExpr::Path(parts.clone())),
            Expr::String(text) => Ok(IrExpr::String(text.clone())),
            Expr::Array(items) => {
                let lowered = items
                    .iter()
                    .map(|expr| self.lower_expr(expr))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(IrExpr::Array(lowered))
            }
            Expr::Struct(entries) => {
                let mut lowered = IndexMap::new();
                for (key, value) in entries.iter() {
                    lowered.insert(key.clone(), self.lower_expr(value)?);
                }
                Ok(IrExpr::Struct(lowered))
            }
            Expr::Unary { op, expr } => Ok(IrExpr::Unary {
                op: *op,
                expr: Box::new(self.lower_expr(expr)?),
            }),
            Expr::Binary { op, left, right } => Ok(IrExpr::Binary {
                op: *op,
                left: Box::new(self.lower_expr(left)?),
                right: Box::new(self.lower_expr(right)?),
            }),
            Expr::Conditional {
                condition,
                then_branch,
                else_branch,
            } => Ok(IrExpr::Conditional {
                condition: Box::new(self.lower_expr(condition)?),
                then_branch: Box::new(self.lower_expr(then_branch)?),
                else_branch: match else_branch {
                    Some(expr) => Some(Box::new(self.lower_expr(expr)?)),
                    None => None,
                },
            }),
            Expr::Call { target, args } => {
                let lowered_args = args
                    .iter()
                    .map(|arg| self.lower_expr(arg))
                    .collect::<Result<Vec<_>, _>>()?;
                let function = self.lower_call_target(target)?;
                self.validate_call(&function, lowered_args.len())?;
                Ok(IrExpr::Call {
                    function,
                    args: lowered_args,
                })
            }
            Expr::Flow(flow) => Ok(IrExpr::Flow(*flow)),
            Expr::Index { target, index } => Ok(IrExpr::Index {
                target: Box::new(self.lower_expr(target)?),
                index: Box::new(self.lower_expr(index)?),
            }),
        }
    }

    fn lower_call_target(&self, target: &Expr) -> Result<FunctionRef, LowerError> {
        match target {
            Expr::Path(parts) => {
                if let Some(builtin) = BuiltinFunction::from_path(parts) {
                    Ok(FunctionRef::Builtin(builtin))
                } else {
                    Err(LowerError::UnknownFunction {
                        name: parts.join("."),
                    })
                }
            }
            other => Err(LowerError::UnsupportedCallTarget {
                description: format!("{other:?}"),
            }),
        }
    }

    fn validate_call(&self, function: &FunctionRef, arg_count: usize) -> Result<(), LowerError> {
        match function {
            FunctionRef::Builtin(builtin) => {
                let expected = builtin.arity();
                if expected != arg_count {
                    Err(LowerError::InvalidArgumentCount {
                        name: builtin.symbol_name().to_string(),
                        expected,
                        actual: arg_count,
                    })
                } else {
                    Ok(())
                }
            }
        }
    }
}

#[derive(Debug, Error)]
pub enum LowerError {
    #[error("unknown function `{name}`")]
    UnknownFunction { name: String },
    #[error("unsupported call target: {description}")]
    UnsupportedCallTarget { description: String },
    #[error("invalid argument count for `{name}`: expected {expected}, got {actual}")]
    InvalidArgumentCount {
        name: String,
        expected: usize,
        actual: usize,
    },
}
