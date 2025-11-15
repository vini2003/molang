//! Host implementations of math helpers that mirror Molang `math.*` builtins.
use once_cell::sync::Lazy;
use rand::{rngs::SmallRng, Rng, SeedableRng};
use std::sync::Mutex;

/// Shared RNG used by all math.random helpers. Mutex guards concurrent JIT-compiled code.
static RNG: Lazy<Mutex<SmallRng>> = Lazy::new(|| Mutex::new(SmallRng::from_entropy()));

fn with_rng<T>(f: impl FnOnce(&mut SmallRng) -> T) -> T {
    let mut rng = RNG.lock().expect("rng poisoned");
    f(&mut rng)
}

fn normalize_low_high(mut low: f64, mut high: f64) -> (f64, f64) {
    if low > high {
        std::mem::swap(&mut low, &mut high);
    }
    (low, high)
}

/// Molang-compatible random float in `[low, high]`.
pub fn math_random(low: f64, high: f64) -> f64 {
    let (low, high) = normalize_low_high(low, high);
    with_rng(|rng| rng.gen_range(low..=high))
}

/// Molang-compatible random integer in `[low, high]`.
pub fn math_random_integer(low: f64, high: f64) -> f64 {
    let (low, high) = normalize_low_high(low.floor(), high.floor());
    let low = low as i64;
    let high = high as i64;
    with_rng(|rng| rng.gen_range(low..=high) as f64)
}

pub fn math_clamp(value: f64, min: f64, max: f64) -> f64 {
    value.clamp(min, max)
}

pub extern "C" fn builtin_math_cos(value: f64) -> f64 {
    value.cos()
}

pub extern "C" fn builtin_math_sin(value: f64) -> f64 {
    value.sin()
}

pub extern "C" fn builtin_math_abs(value: f64) -> f64 {
    value.abs()
}

pub extern "C" fn builtin_math_random(low: f64, high: f64) -> f64 {
    math_random(low, high)
}

pub extern "C" fn builtin_math_random_integer(low: f64, high: f64) -> f64 {
    math_random_integer(low, high)
}

pub extern "C" fn builtin_math_clamp(value: f64, min: f64, max: f64) -> f64 {
    math_clamp(value, min, max)
}

pub extern "C" fn builtin_math_sqrt(value: f64) -> f64 {
    value.sqrt()
}

pub extern "C" fn builtin_math_floor(value: f64) -> f64 {
    value.floor()
}

pub extern "C" fn builtin_math_ceil(value: f64) -> f64 {
    value.ceil()
}

pub extern "C" fn builtin_math_round(value: f64) -> f64 {
    value.round()
}

pub extern "C" fn builtin_math_trunc(value: f64) -> f64 {
    value.trunc()
}
