//! The transcendental functions simulation code may call.
//!
//! These are the only transcendental functions simulation code may call. They give
//! the same bits on every platform because the `libm` crate is plain Rust, not the
//! platform's C math library. `sqrt`, `abs`, `floor`, `%` and the like are exact in
//! IEEE 754 and stay as `std` methods; `mul_add` is not used.

/// Sine of `x` (radians).
#[inline]
pub fn sin(x: f64) -> f64 {
    libm::sin(x)
}

/// Cosine of `x` (radians).
#[inline]
pub fn cos(x: f64) -> f64 {
    libm::cos(x)
}

/// `(sin(x), cos(x))`, each computed by its own function.
#[inline]
pub fn sin_cos(x: f64) -> (f64, f64) {
    (libm::sin(x), libm::cos(x))
}

/// Four-quadrant arctangent of `y / x`.
#[inline]
pub fn atan2(y: f64, x: f64) -> f64 {
    libm::atan2(y, x)
}

/// `x` raised to the power `y`.
#[inline]
pub fn powf(x: f64, y: f64) -> f64 {
    libm::pow(x, y)
}

/// Hyperbolic sine of `x`.
#[inline]
pub fn sinh(x: f64) -> f64 {
    libm::sinh(x)
}

/// Hyperbolic cosine of `x`.
#[inline]
pub fn cosh(x: f64) -> f64 {
    libm::cosh(x)
}

/// `x` raised to the integer power `n`, by a fixed square-and-multiply loop over
/// the bits of `n` (a negative `n` takes the reciprocal at the end). `f64::powi`
/// is an LLVM intrinsic whose operation order is not specified, so it is not used.
#[inline]
pub fn powi(x: f64, n: i32) -> f64 {
    let recip = n < 0;
    let mut b = n;
    let mut a = x;
    let mut r = 1.0;
    loop {
        if b & 1 != 0 {
            r *= a;
        }
        b /= 2;
        if b == 0 {
            break;
        }
        a *= a;
    }
    if recip { 1.0 / r } else { r }
}
