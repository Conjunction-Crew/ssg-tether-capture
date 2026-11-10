//! Known-answer tests of `crate::math` (178-P11 S1, T1a): the bit pattern of each
//! function at eight fixed inputs. The constants were made on macOS (aarch64);
//! `full-ci` asserts the same constants on Linux and Windows. A failure on one
//! platform is a finding (a platform difference), never a constant to change.

use std::f64::consts::{FRAC_PI_2, PI};

use crate::math;

/// The smallest positive subnormal `f64`.
const SUB: f64 = 5e-324;

const TRIG_IN: [f64; 8] = [
    0.0,
    SUB,
    FRAC_PI_2,
    FRAC_PI_2 - 1e-7,
    PI,
    PI + 1e-7,
    1e6,
    -2.5,
];
/// `atan` (178-P11 S3, for `c2ixys`): include 0, a subnormal, values near π/2 and
/// π, 1e6, a negative value, and the size of the CIP's `sqrt(r2 / (1 - r2))`.
const ATAN_IN: [f64; 8] = [0.0, SUB, FRAC_PI_2, PI, 1e6, -2.5, 1e-9, 1.5e-2];
const HYP_IN: [f64; 8] = [0.0, SUB, FRAC_PI_2, PI, 1e-3, 1e6, -2.5, 0.5];
const ATAN2_IN: [(f64, f64); 8] = [
    (0.0, 1.0),
    (SUB, 1.0),
    (1.0, 0.0),
    (1.0, 1e-9),
    (0.0, -1.0),
    (1e-9, -1.0),
    (1e6, 3.0),
    (-2.5, -0.5),
];
const POWF_IN: [(f64, f64); 8] = [
    (0.0, 2.0),
    (SUB, 0.5),
    (FRAC_PI_2, 1.5),
    (PI, -0.5),
    (6_918_000.0, 1.5),
    (1e6, 1.0 / 3.0),
    (-2.5, 3.0),
    (0.5, 10.0),
];
const POWI_IN: [(f64, i32); 8] = [
    (1.000_000_1, 0),
    (-2.5, 1),
    (6_918_000.0, 2),
    (6_918_000.0, 3),
    (6_918_000.0, -2),
    (1.1, 7),
    (SUB, 2),
    (-1e6, 3),
];

const SIN_BITS: [u64; 8] = [
    0x0000000000000000,
    0x0000000000000001,
    0x3ff0000000000000,
    0x3fefffffffffffd3,
    0x3ca1a62633145c07,
    0xbe7ad7f29972cec2,
    0xbfd6664b2568d867,
    0xbfe326af0dcfcab0,
];
const COS_BITS: [u64; 8] = [
    0x3ff0000000000000,
    0x3ff0000000000000,
    0x3c91a62633145c07,
    0x3e7ad7f29b46988c,
    0xbff0000000000000,
    0xbfefffffffffffd3,
    0x3fedf9df9906d32c,
    0xbfe9a2f7ef858b7d,
];
const ATAN2_BITS: [u64; 8] = [
    0x0000000000000000,
    0x0000000000000001,
    0x3ff921fb54442d18,
    0x3ff921fb53ff74e9,
    0x400921fb54442d18,
    0x400921fb5421d100,
    0x3ff921f82ef5bef6,
    0xbffc4a83936311da,
];
const ATAN_BITS: [u64; 8] = [
    0x0000000000000000,
    0x0000000000000001,
    0x3ff00fe987ed02ff,
    0x3ff433b8a322ddd2,
    0x3ff921fa47d4b30d,
    0xbff30b6d796a4da8,
    0x3e112e0be826d695,
    0x3f8eb7baf2082977,
];
const POWF_BITS: [u64; 8] = [
    0x0000000000000000,
    0x1e60000000000000,
    0x3fff7fccdff344ac,
    0x3fe20dd750429b6d,
    0x4210f236aec21885,
    0x4058fffffffffffe,
    0xc02f400000000000,
    0x3f50000000000000,
];
const SINH_BITS: [u64; 8] = [
    0x0000000000000000,
    0x0000000000000001,
    0x4002690f661dd820,
    0x402718f45d72e672,
    0x3f50624e00c1c9e3,
    0x7ff0000000000000,
    0xc0183368cdb0b6d3,
    0x3fe0acd00fe63b97,
];
const COSH_BITS: [u64; 8] = [
    0x3ff0000000000000,
    0x3ff0000000000000,
    0x400412cc2a8d4e9e,
    0x40272f147fee4000,
    0x3ff000008637bdc1,
    0x7ff0000000000000,
    0x40188776e4b30aa3,
    0x3ff20ac1862ae8d0,
];
const POWI_BITS: [u64; 8] = [
    0x3ff0000000000000,
    0xc004000000000000,
    0x42c5c37d1f288000,
    0x4431f2c014658382,
    0x3d1786867a1c2ed5,
    0x3fff2df1fb5a7ed7,
    0x0000000000000000,
    0xc3abc16d674ec800,
];

/// Every mismatch of `actual` against `expected`, one line each with both bit
/// patterns, so a CI failure names the inputs and both values.
fn mismatches<I: std::fmt::Debug>(
    name: &str,
    inputs: &[I],
    actual: &[f64],
    expected: &[u64],
) -> Vec<String> {
    inputs
        .iter()
        .zip(actual.iter().zip(expected))
        .filter(|(_, (a, e))| a.to_bits() != **e)
        .map(|(i, (a, e))| {
            format!(
                "{name}{i:?}: got {:#018x} ({a:e}), expected {e:#018x} ({:e})",
                a.to_bits(),
                f64::from_bits(*e)
            )
        })
        .collect()
}

fn assert_none(found: Vec<String>) {
    assert!(
        found.is_empty(),
        "known-answer mismatches:\n{}",
        found.join("\n")
    );
}

#[test]
fn sin_cos_known_answers() {
    let sin: Vec<f64> = TRIG_IN.iter().map(|&x| math::sin(x)).collect();
    let cos: Vec<f64> = TRIG_IN.iter().map(|&x| math::cos(x)).collect();
    let mut found = mismatches("sin", &TRIG_IN, &sin, &SIN_BITS);
    found.extend(mismatches("cos", &TRIG_IN, &cos, &COS_BITS));
    // `sin_cos` is the two functions, each on its own.
    let pair: Vec<(f64, f64)> = TRIG_IN.iter().map(|&x| math::sin_cos(x)).collect();
    let (s, c): (Vec<f64>, Vec<f64>) = pair.into_iter().unzip();
    found.extend(mismatches("sin_cos.0", &TRIG_IN, &s, &SIN_BITS));
    found.extend(mismatches("sin_cos.1", &TRIG_IN, &c, &COS_BITS));
    assert_none(found);
}

#[test]
fn atan2_known_answers() {
    let got: Vec<f64> = ATAN2_IN.iter().map(|&(y, x)| math::atan2(y, x)).collect();
    assert_none(mismatches("atan2", &ATAN2_IN, &got, &ATAN2_BITS));
}

#[test]
fn powf_known_answers() {
    let got: Vec<f64> = POWF_IN.iter().map(|&(x, y)| math::powf(x, y)).collect();
    assert_none(mismatches("powf", &POWF_IN, &got, &POWF_BITS));
}

#[test]
fn sinh_cosh_known_answers() {
    let sinh: Vec<f64> = HYP_IN.iter().map(|&x| math::sinh(x)).collect();
    let cosh: Vec<f64> = HYP_IN.iter().map(|&x| math::cosh(x)).collect();
    let mut found = mismatches("sinh", &HYP_IN, &sinh, &SINH_BITS);
    found.extend(mismatches("cosh", &HYP_IN, &cosh, &COSH_BITS));
    assert_none(found);
}

#[test]
fn powi_known_answers() {
    let got: Vec<f64> = POWI_IN.iter().map(|&(x, n)| math::powi(x, n)).collect();
    assert_none(mismatches("powi", &POWI_IN, &got, &POWI_BITS));
}

#[test]
fn powi_is_exact_where_the_product_is() {
    // Small integers: every partial product is exact, so the loop's order cannot
    // show; these pin the loop's handling of each bit of `n` and of a negative `n`.
    assert_eq!(math::powi(3.0, 0), 1.0);
    assert_eq!(math::powi(3.0, 5), 243.0);
    assert_eq!(math::powi(2.0, -3), 0.125);
    assert_eq!(math::powi(-2.0, 7), -128.0);
    assert_eq!(math::powi(0.5, -1), 2.0);
}

#[test]
fn atan_known_answers() {
    let got: Vec<f64> = ATAN_IN.iter().map(|&x| math::atan(x)).collect();
    assert_none(mismatches("atan", &ATAN_IN, &got, &ATAN_BITS));
}
