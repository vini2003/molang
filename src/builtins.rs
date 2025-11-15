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

// Trigonometric functions (all in degrees, Molang convention)
pub extern "C" fn builtin_math_acos(value: f64) -> f64 {
    value.acos().to_degrees()
}

pub extern "C" fn builtin_math_asin(value: f64) -> f64 {
    value.asin().to_degrees()
}

pub extern "C" fn builtin_math_atan(value: f64) -> f64 {
    value.atan().to_degrees()
}

pub extern "C" fn builtin_math_atan2(y: f64, x: f64) -> f64 {
    y.atan2(x).to_degrees()
}

// Exponential and logarithmic functions
pub extern "C" fn builtin_math_exp(value: f64) -> f64 {
    value.exp()
}

pub extern "C" fn builtin_math_ln(value: f64) -> f64 {
    value.ln()
}

pub extern "C" fn builtin_math_pow(base: f64, exponent: f64) -> f64 {
    base.powf(exponent)
}

// Basic arithmetic functions
pub extern "C" fn builtin_math_max(a: f64, b: f64) -> f64 {
    a.max(b)
}

pub extern "C" fn builtin_math_min(a: f64, b: f64) -> f64 {
    a.min(b)
}

pub extern "C" fn builtin_math_mod(value: f64, denominator: f64) -> f64 {
    value % denominator
}

pub extern "C" fn builtin_math_sign(value: f64) -> f64 {
    if value > 0.0 {
        1.0
    } else {
        -1.0
    }
}

pub extern "C" fn builtin_math_copy_sign(a: f64, b: f64) -> f64 {
    a.copysign(b)
}

pub extern "C" fn builtin_math_pi() -> f64 {
    std::f64::consts::PI
}

// Angle functions
pub extern "C" fn builtin_math_min_angle(value: f64) -> f64 {
    let mut angle = value % 360.0;
    if angle >= 180.0 {
        angle -= 360.0;
    } else if angle < -180.0 {
        angle += 360.0;
    }
    angle
}

// Interpolation functions
pub extern "C" fn builtin_math_lerp(start: f64, end: f64, t: f64) -> f64 {
    start + (end - start) * t
}

pub extern "C" fn builtin_math_inverse_lerp(start: f64, end: f64, value: f64) -> f64 {
    if (end - start).abs() < f64::EPSILON {
        0.0
    } else {
        (value - start) / (end - start)
    }
}

pub extern "C" fn builtin_math_lerprotate(start: f64, end: f64, t: f64) -> f64 {
    let mut diff = (end - start) % 360.0;
    if diff > 180.0 {
        diff -= 360.0;
    } else if diff < -180.0 {
        diff += 360.0;
    }
    start + diff * t
}

pub extern "C" fn builtin_math_hermite_blend(t: f64) -> f64 {
    3.0 * t * t - 2.0 * t * t * t
}

// Die roll functions
pub extern "C" fn builtin_math_die_roll(num: f64, low: f64, high: f64) -> f64 {
    let count = num.max(0.0) as i32;
    let (low, high) = normalize_low_high(low, high);
    let mut sum = 0.0;
    for _ in 0..count {
        sum += math_random(low, high);
    }
    sum
}

pub extern "C" fn builtin_math_die_roll_integer(num: f64, low: f64, high: f64) -> f64 {
    let count = num.max(0.0) as i32;
    let (low, high) = normalize_low_high(low.floor(), high.floor());
    let low = low as i64;
    let high = high as i64;
    let mut sum = 0.0;
    for _ in 0..count {
        sum += with_rng(|rng| rng.gen_range(low..=high) as f64);
    }
    sum
}

// Easing functions - Quadratic
pub extern "C" fn builtin_math_ease_in_quad(start: f64, end: f64, t: f64) -> f64 {
    start + (end - start) * t * t
}

pub extern "C" fn builtin_math_ease_out_quad(start: f64, end: f64, t: f64) -> f64 {
    start + (end - start) * t * (2.0 - t)
}

pub extern "C" fn builtin_math_ease_in_out_quad(start: f64, end: f64, t: f64) -> f64 {
    let factor = if t < 0.5 {
        2.0 * t * t
    } else {
        -1.0 + (4.0 - 2.0 * t) * t
    };
    start + (end - start) * factor
}

// Easing functions - Cubic
pub extern "C" fn builtin_math_ease_in_cubic(start: f64, end: f64, t: f64) -> f64 {
    start + (end - start) * t * t * t
}

pub extern "C" fn builtin_math_ease_out_cubic(start: f64, end: f64, t: f64) -> f64 {
    let t1 = t - 1.0;
    start + (end - start) * (t1 * t1 * t1 + 1.0)
}

pub extern "C" fn builtin_math_ease_in_out_cubic(start: f64, end: f64, t: f64) -> f64 {
    let factor = if t < 0.5 {
        4.0 * t * t * t
    } else {
        let t1 = 2.0 * t - 2.0;
        1.0 + t1 * t1 * t1 / 2.0
    };
    start + (end - start) * factor
}

// Easing functions - Quartic
pub extern "C" fn builtin_math_ease_in_quart(start: f64, end: f64, t: f64) -> f64 {
    start + (end - start) * t * t * t * t
}

pub extern "C" fn builtin_math_ease_out_quart(start: f64, end: f64, t: f64) -> f64 {
    let t1 = t - 1.0;
    start + (end - start) * (1.0 - t1 * t1 * t1 * t1)
}

pub extern "C" fn builtin_math_ease_in_out_quart(start: f64, end: f64, t: f64) -> f64 {
    let factor = if t < 0.5 {
        8.0 * t * t * t * t
    } else {
        let t1 = t - 1.0;
        1.0 - 8.0 * t1 * t1 * t1 * t1
    };
    start + (end - start) * factor
}

// Easing functions - Quintic
pub extern "C" fn builtin_math_ease_in_quint(start: f64, end: f64, t: f64) -> f64 {
    start + (end - start) * t * t * t * t * t
}

pub extern "C" fn builtin_math_ease_out_quint(start: f64, end: f64, t: f64) -> f64 {
    let t1 = t - 1.0;
    start + (end - start) * (1.0 + t1 * t1 * t1 * t1 * t1)
}

pub extern "C" fn builtin_math_ease_in_out_quint(start: f64, end: f64, t: f64) -> f64 {
    let factor = if t < 0.5 {
        16.0 * t * t * t * t * t
    } else {
        let t1 = 2.0 * t - 2.0;
        1.0 + t1 * t1 * t1 * t1 * t1 / 2.0
    };
    start + (end - start) * factor
}

// Easing functions - Sine
pub extern "C" fn builtin_math_ease_in_sine(start: f64, end: f64, t: f64) -> f64 {
    let pi = std::f64::consts::PI;
    start + (end - start) * (1.0 - (t * pi / 2.0).cos())
}

pub extern "C" fn builtin_math_ease_out_sine(start: f64, end: f64, t: f64) -> f64 {
    let pi = std::f64::consts::PI;
    start + (end - start) * (t * pi / 2.0).sin()
}

pub extern "C" fn builtin_math_ease_in_out_sine(start: f64, end: f64, t: f64) -> f64 {
    let pi = std::f64::consts::PI;
    start + (end - start) * (1.0 - (t * pi).cos()) / 2.0
}

// Easing functions - Exponential
pub extern "C" fn builtin_math_ease_in_expo(start: f64, end: f64, t: f64) -> f64 {
    let factor = if t == 0.0 { 0.0 } else { (2.0_f64).powf(10.0 * t - 10.0) };
    start + (end - start) * factor
}

pub extern "C" fn builtin_math_ease_out_expo(start: f64, end: f64, t: f64) -> f64 {
    let factor = if t == 1.0 { 1.0 } else { 1.0 - (2.0_f64).powf(-10.0 * t) };
    start + (end - start) * factor
}

pub extern "C" fn builtin_math_ease_in_out_expo(start: f64, end: f64, t: f64) -> f64 {
    let factor = if t == 0.0 {
        0.0
    } else if t == 1.0 {
        1.0
    } else if t < 0.5 {
        (2.0_f64).powf(20.0 * t - 10.0) / 2.0
    } else {
        (2.0 - (2.0_f64).powf(-20.0 * t + 10.0)) / 2.0
    };
    start + (end - start) * factor
}

// Easing functions - Circular
pub extern "C" fn builtin_math_ease_in_circ(start: f64, end: f64, t: f64) -> f64 {
    start + (end - start) * (1.0 - (1.0 - t * t).sqrt())
}

pub extern "C" fn builtin_math_ease_out_circ(start: f64, end: f64, t: f64) -> f64 {
    start + (end - start) * ((1.0 - (t - 1.0) * (t - 1.0)).sqrt())
}

pub extern "C" fn builtin_math_ease_in_out_circ(start: f64, end: f64, t: f64) -> f64 {
    let factor = if t < 0.5 {
        (1.0 - (1.0 - (2.0 * t) * (2.0 * t)).sqrt()) / 2.0
    } else {
        ((1.0 - (-2.0 * t + 2.0) * (-2.0 * t + 2.0)).sqrt() + 1.0) / 2.0
    };
    start + (end - start) * factor
}

// Easing functions - Back
pub extern "C" fn builtin_math_ease_in_back(start: f64, end: f64, t: f64) -> f64 {
    const C1: f64 = 1.70158;
    const C3: f64 = C1 + 1.0;
    start + (end - start) * (C3 * t * t * t - C1 * t * t)
}

pub extern "C" fn builtin_math_ease_out_back(start: f64, end: f64, t: f64) -> f64 {
    const C1: f64 = 1.70158;
    const C3: f64 = C1 + 1.0;
    let t1 = t - 1.0;
    start + (end - start) * (1.0 + C3 * t1 * t1 * t1 + C1 * t1 * t1)
}

pub extern "C" fn builtin_math_ease_in_out_back(start: f64, end: f64, t: f64) -> f64 {
    const C1: f64 = 1.70158;
    const C2: f64 = C1 * 1.525;
    let factor = if t < 0.5 {
        ((2.0 * t) * (2.0 * t) * ((C2 + 1.0) * 2.0 * t - C2)) / 2.0
    } else {
        let t1 = 2.0 * t - 2.0;
        (t1 * t1 * ((C2 + 1.0) * t1 + C2) + 2.0) / 2.0
    };
    start + (end - start) * factor
}

// Easing functions - Elastic
pub extern "C" fn builtin_math_ease_in_elastic(start: f64, end: f64, t: f64) -> f64 {
    const C4: f64 = (2.0 * std::f64::consts::PI) / 3.0;
    let factor = if t == 0.0 {
        0.0
    } else if t == 1.0 {
        1.0
    } else {
        -(2.0_f64).powf(10.0 * t - 10.0) * ((t * 10.0 - 10.75) * C4).sin()
    };
    start + (end - start) * factor
}

pub extern "C" fn builtin_math_ease_out_elastic(start: f64, end: f64, t: f64) -> f64 {
    const C4: f64 = (2.0 * std::f64::consts::PI) / 3.0;
    let factor = if t == 0.0 {
        0.0
    } else if t == 1.0 {
        1.0
    } else {
        (2.0_f64).powf(-10.0 * t) * ((t * 10.0 - 0.75) * C4).sin() + 1.0
    };
    start + (end - start) * factor
}

pub extern "C" fn builtin_math_ease_in_out_elastic(start: f64, end: f64, t: f64) -> f64 {
    const C5: f64 = (2.0 * std::f64::consts::PI) / 4.5;
    let factor = if t == 0.0 {
        0.0
    } else if t == 1.0 {
        1.0
    } else if t < 0.5 {
        -(2.0_f64).powf(20.0 * t - 10.0) * ((20.0 * t - 11.125) * C5).sin() / 2.0
    } else {
        (2.0_f64).powf(-20.0 * t + 10.0) * ((20.0 * t - 11.125) * C5).sin() / 2.0 + 1.0
    };
    start + (end - start) * factor
}

// Easing functions - Bounce
fn bounce_out(t: f64) -> f64 {
    const N1: f64 = 7.5625;
    const D1: f64 = 2.75;
    if t < 1.0 / D1 {
        N1 * t * t
    } else if t < 2.0 / D1 {
        let t1 = t - 1.5 / D1;
        N1 * t1 * t1 + 0.75
    } else if t < 2.5 / D1 {
        let t1 = t - 2.25 / D1;
        N1 * t1 * t1 + 0.9375
    } else {
        let t1 = t - 2.625 / D1;
        N1 * t1 * t1 + 0.984375
    }
}

pub extern "C" fn builtin_math_ease_in_bounce(start: f64, end: f64, t: f64) -> f64 {
    start + (end - start) * (1.0 - bounce_out(1.0 - t))
}

pub extern "C" fn builtin_math_ease_out_bounce(start: f64, end: f64, t: f64) -> f64 {
    start + (end - start) * bounce_out(t)
}

pub extern "C" fn builtin_math_ease_in_out_bounce(start: f64, end: f64, t: f64) -> f64 {
    let factor = if t < 0.5 {
        (1.0 - bounce_out(1.0 - 2.0 * t)) / 2.0
    } else {
        (1.0 + bounce_out(2.0 * t - 1.0)) / 2.0
    };
    start + (end - start) * factor
}
