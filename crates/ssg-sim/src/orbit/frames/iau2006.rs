// Derived from ERFA v2.0.1 (https://github.com/liberfa/erfa, commit
// 9915ba38c9365f8b0738269b8c2ac1fdd5f8dee3), Copyright (C) 2013-2021, NumFOCUS
// Foundation, BSD-3-Clause; ERFA is derived, with permission, from the SOFA library.
// This file is a translation into Rust; it is not ERFA or SOFA software and is not
// endorsed by either.

//! The ERFA routines of the IAU 2006/2000A celestial-to-terrestrial chain, in Rust.
//!
//! Each function is the ERFA routine of the same name without the `era` prefix,
//! translated expression for expression: ERFA's operation order and literal digits
//! are kept, C `fmod(a, b)` is `a % b`, `sin`, `cos`, `atan` and `atan2` are
//! [`crate::math`] (the `libm` crate), `sqrt` stays `std` (exact in IEEE 754), and no
//! fused multiply-add is used. The series of `nut00a` and `s06` are summed from the last term
//! to the first, as in ERFA. Matrices are ERFA's `double[3][3]`, row by row.
//! Two-argument dates are ERFA's two-part Julian dates (`date1 + date2`).
//!
//! `fane03`, `rxr` and `tr` are not on the chain `orbit::frames` computes; they are
//! here because the port's routine list names them.

#![allow(clippy::excessive_precision, clippy::approx_constant)]

use super::nut00a_tables::{XLS, XPL};
use crate::math;

/// A 3×3 matrix as ERFA stores it: `r[row][column]`.
pub type Mat = [[f64; 3]; 3];

/// 2π (`ERFA_D2PI`).
const D2PI: f64 = 6.283185307179586476925287;
/// Arcseconds to radians (`ERFA_DAS2R`).
const DAS2R: f64 = 4.848136811095359935899141e-6;
/// Arcseconds in a full circle (`ERFA_TURNAS`).
const TURNAS: f64 = 1296000.0;
/// Reference epoch J2000.0, Julian date (`ERFA_DJ00`).
const DJ00: f64 = 2451545.0;
/// Days per Julian century (`ERFA_DJC`).
const DJC: f64 = 36525.0;

/// Normalize angle into the range 0 <= a < 2π (`anp.c`).
pub fn anp(a: f64) -> f64 {
    let mut w = a % D2PI;
    if w < 0.0 {
        w += D2PI;
    }
    w
}

/// Fundamental argument, IERS Conventions (2003): mean anomaly of the Moon (`fal03.c`).
pub fn fal03(t: f64) -> f64 {
    ((485868.249036 + t * (1717915923.2178 + t * (31.8792 + t * (0.051635 + t * (-0.00024470)))))
        % TURNAS)
        * DAS2R
}

/// Fundamental argument, IERS Conventions (2003): mean anomaly of the Sun (`falp03.c`).
pub fn falp03(t: f64) -> f64 {
    ((1287104.793048 + t * (129596581.0481 + t * (-0.5532 + t * (0.000136 + t * (-0.00001149)))))
        % TURNAS)
        * DAS2R
}

/// Fundamental argument, IERS Conventions (2003): mean longitude of the Moon minus
/// that of the ascending node (`faf03.c`).
pub fn faf03(t: f64) -> f64 {
    ((335779.526232 + t * (1739527262.8478 + t * (-12.7512 + t * (-0.001037 + t * (0.00000417)))))
        % TURNAS)
        * DAS2R
}

/// Fundamental argument, IERS Conventions (2003): mean elongation of the Moon from
/// the Sun (`fad03.c`).
pub fn fad03(t: f64) -> f64 {
    ((1072260.703692 + t * (1602961601.2090 + t * (-6.3706 + t * (0.006593 + t * (-0.00003169)))))
        % TURNAS)
        * DAS2R
}

/// Fundamental argument, IERS Conventions (2003): mean longitude of the Moon's
/// ascending node (`faom03.c`).
pub fn faom03(t: f64) -> f64 {
    ((450160.398036 + t * (-6962890.5431 + t * (7.4722 + t * (0.007702 + t * (-0.00005939)))))
        % TURNAS)
        * DAS2R
}

/// Fundamental argument, IERS Conventions (2003): mean longitude of Mercury (`fame03.c`).
pub fn fame03(t: f64) -> f64 {
    (4.402608842 + 2608.7903141574 * t) % D2PI
}

/// Fundamental argument, IERS Conventions (2003): mean longitude of Venus (`fave03.c`).
pub fn fave03(t: f64) -> f64 {
    (3.176146697 + 1021.3285546211 * t) % D2PI
}

/// Fundamental argument, IERS Conventions (2003): mean longitude of Earth (`fae03.c`).
pub fn fae03(t: f64) -> f64 {
    (1.753470314 + 628.3075849991 * t) % D2PI
}

/// Fundamental argument, IERS Conventions (2003): mean longitude of Mars (`fama03.c`).
pub fn fama03(t: f64) -> f64 {
    (6.203480913 + 334.0612426700 * t) % D2PI
}

/// Fundamental argument, IERS Conventions (2003): mean longitude of Jupiter (`faju03.c`).
pub fn faju03(t: f64) -> f64 {
    (0.599546497 + 52.9690962641 * t) % D2PI
}

/// Fundamental argument, IERS Conventions (2003): mean longitude of Saturn (`fasa03.c`).
pub fn fasa03(t: f64) -> f64 {
    (0.874016757 + 21.3299104960 * t) % D2PI
}

/// Fundamental argument, IERS Conventions (2003): mean longitude of Uranus (`faur03.c`).
pub fn faur03(t: f64) -> f64 {
    (5.481293872 + 7.4781598567 * t) % D2PI
}

/// Fundamental argument, IERS Conventions (2003): mean longitude of Neptune (`fane03.c`).
pub fn fane03(t: f64) -> f64 {
    (5.311886287 + 3.8133035638 * t) % D2PI
}

/// Fundamental argument, IERS Conventions (2003): general accumulated precession in
/// longitude (`fapa03.c`).
pub fn fapa03(t: f64) -> f64 {
    (0.024381750 + 0.00000538691 * t) * t
}

/// Mean obliquity of the ecliptic, IAU 2006 precession model (`obl06.c`).
pub fn obl06(date1: f64, date2: f64) -> f64 {
    let t = ((date1 - DJ00) + date2) / DJC;
    (84381.406
        + (-46.836769
            + (-0.0001831 + (0.00200340 + (-0.000000576 + (-0.0000000434) * t) * t) * t) * t)
            * t)
        * DAS2R
}

/// Precession angles, IAU 2006 (Fukushima-Williams 4-angle formulation): returns
/// `(gamb, phib, psib, epsa)` (`pfw06.c`).
pub fn pfw06(date1: f64, date2: f64) -> (f64, f64, f64, f64) {
    let t = ((date1 - DJ00) + date2) / DJC;
    let gamb = (-0.052928
        + (10.556378
            + (0.4932044 + (-0.00031238 + (-0.000002788 + (0.0000000260) * t) * t) * t) * t)
            * t)
        * DAS2R;
    let phib = (84381.412819
        + (-46.811016
            + (0.0511268 + (0.00053289 + (-0.000000440 + (-0.0000000176) * t) * t) * t) * t)
            * t)
        * DAS2R;
    let psib = (-0.041775
        + (5038.481484
            + (1.5584175 + (-0.00018522 + (-0.000026452 + (-0.0000000148) * t) * t) * t) * t)
            * t)
        * DAS2R;
    let epsa = obl06(date1, date2);
    (gamb, phib, psib, epsa)
}

/// Form rotation matrix given the Fukushima-Williams angles (`fw2m.c`).
pub fn fw2m(gamb: f64, phib: f64, psi: f64, eps: f64) -> Mat {
    let mut r = [[0.0; 3]; 3];
    ir(&mut r);
    rz(gamb, &mut r);
    rx(phib, &mut r);
    rz(-psi, &mut r);
    rx(-eps, &mut r);
    r
}

/// Nutation, IAU 2000A model (MHB2000 luni-solar and planetary nutation with free
/// core nutation omitted): returns `(dpsi, deps)` (`nut00a.c`).
pub fn nut00a(date1: f64, date2: f64) -> (f64, f64) {
    // Units of 0.1 microarcsecond to radians.
    const U2R: f64 = DAS2R / 1e7;

    // Interval between fundamental date J2000.0 and given date (JC).
    let t = ((date1 - DJ00) + date2) / DJC;

    // Luni-solar nutation. Fundamental (Delaunay) arguments.
    // Mean anomaly of the Moon (IERS 2003).
    let el = fal03(t);
    // Mean anomaly of the Sun (MHB2000).
    let elp = ((1287104.79305
        + t * (129596581.0481 + t * (-0.5532 + t * (0.000136 + t * (-0.00001149)))))
        % TURNAS)
        * DAS2R;
    // Mean longitude of the Moon minus that of the ascending node (IERS 2003).
    let f = faf03(t);
    // Mean elongation of the Moon from the Sun (MHB2000).
    let d = ((1072260.70369
        + t * (1602961601.2090 + t * (-6.3706 + t * (0.006593 + t * (-0.00003169)))))
        % TURNAS)
        * DAS2R;
    // Mean longitude of the ascending node of the Moon (IERS 2003).
    let om = faom03(t);

    // Summation of luni-solar nutation series (in reverse order).
    let mut dp = 0.0;
    let mut de = 0.0;
    for x in XLS.iter().rev() {
        let arg = (x.n[0] as f64 * el
            + x.n[1] as f64 * elp
            + x.n[2] as f64 * f
            + x.n[3] as f64 * d
            + x.n[4] as f64 * om)
            % D2PI;
        let sarg = math::sin(arg);
        let carg = math::cos(arg);
        dp += (x.sp + x.spt * t) * sarg + x.cp * carg;
        de += (x.ce + x.cet * t) * carg + x.se * sarg;
    }
    // Convert from 0.1 microarcsec units to radians.
    let dpsils = dp * U2R;
    let depsls = de * U2R;

    // Planetary nutation (MHB2000 arguments where ERFA uses them).
    // Mean anomaly of the Moon (MHB2000).
    let al = (2.35555598 + 8328.6914269554 * t) % D2PI;
    // Mean longitude of the Moon minus that of the ascending node (MHB2000).
    let af = (1.627905234 + 8433.466158131 * t) % D2PI;
    // Mean elongation of the Moon from the Sun (MHB2000).
    let ad = (5.198466741 + 7771.3771468121 * t) % D2PI;
    // Mean longitude of the ascending node of the Moon (MHB2000).
    let aom = (2.18243920 - 33.757045 * t) % D2PI;
    // General accumulated precession in longitude (IERS 2003).
    let apa = fapa03(t);
    // Planetary longitudes, Mercury through Uranus (IERS 2003).
    let alme = fame03(t);
    let alve = fave03(t);
    let alea = fae03(t);
    let alma = fama03(t);
    let alju = faju03(t);
    let alsa = fasa03(t);
    let alur = faur03(t);
    // Neptune longitude (MHB2000).
    let alne = (5.321159000 + 3.8127774000 * t) % D2PI;

    // Summation of planetary nutation series (in reverse order).
    let mut dp = 0.0;
    let mut de = 0.0;
    for x in XPL.iter().rev() {
        let arg = (x.n[0] as f64 * al
            + x.n[1] as f64 * af
            + x.n[2] as f64 * ad
            + x.n[3] as f64 * aom
            + x.n[4] as f64 * alme
            + x.n[5] as f64 * alve
            + x.n[6] as f64 * alea
            + x.n[7] as f64 * alma
            + x.n[8] as f64 * alju
            + x.n[9] as f64 * alsa
            + x.n[10] as f64 * alur
            + x.n[11] as f64 * alne
            + x.n[12] as f64 * apa)
            % D2PI;
        let sarg = math::sin(arg);
        let carg = math::cos(arg);
        dp += x.sp as f64 * sarg + x.cp as f64 * carg;
        de += x.se as f64 * sarg + x.ce as f64 * carg;
    }
    // Convert from 0.1 microarcsec units to radians.
    let dpsipl = dp * U2R;
    let depspl = de * U2R;

    // Add luni-solar and planetary components.
    (dpsils + dpsipl, depsls + depspl)
}

/// IAU 2000A nutation with adjustments to match the IAU 2006 precession: returns
/// `(dpsi, deps)` (`nut06a.c`).
pub fn nut06a(date1: f64, date2: f64) -> (f64, f64) {
    let t = ((date1 - DJ00) + date2) / DJC;
    // Factor correcting for secular variation of J2.
    let fj2 = -2.7774e-6 * t;
    let (dp, de) = nut00a(date1, date2);
    // Apply P03 adjustments (Wallace & Capitaine, 2006, Eqs.5).
    let dpsi = dp + dp * (0.4697e-6 + fj2);
    let deps = de + de * fj2;
    (dpsi, deps)
}

/// Form the matrix of precession-nutation for a given date (including frame bias),
/// equinox based, IAU 2006 precession and IAU 2000A nutation models (`pnm06a.c`).
pub fn pnm06a(date1: f64, date2: f64) -> Mat {
    let (gamb, phib, psib, epsa) = pfw06(date1, date2);
    let (dp, de) = nut06a(date1, date2);
    fw2m(gamb, phib, psib + dp, epsa + de)
}

/// Extract from the bias-precession-nutation matrix the X,Y coordinates of the
/// Celestial Intermediate Pole: returns `(x, y)` (`bpn2xy.c`).
pub fn bpn2xy(rbpn: &Mat) -> (f64, f64) {
    (rbpn[2][0], rbpn[2][1])
}

/// One term of an `s06` series: the multipliers of l, l', F, D, Om, LVe, LE and pA,
/// then the sine and cosine coefficients.
struct Term {
    nfa: [i32; 8],
    s: f64,
    c: f64,
}

/// Polynomial coefficients of s + XY/2 (`sp` of `s06.c`).
const SP: [f64; 6] = [
    94.00e-6,
    3808.65e-6,
    -122.68e-6,
    -72574.11e-6,
    27.98e-6,
    15.62e-6,
];

// The five series tables of `s06.c`, written from the C source by the same script
// that generates `nut00a_tables.rs`.

/// Terms of order t^0 (33).
#[rustfmt::skip]
static S0: [Term; 33] = [
    Term { nfa: [0, 0, 0, 0, 1, 0, 0, 0], s: -2640.73e-6, c: 0.39e-6 },
    Term { nfa: [0, 0, 0, 0, 2, 0, 0, 0], s: -63.53e-6, c: 0.02e-6 },
    Term { nfa: [0, 0, 2, -2, 3, 0, 0, 0], s: -11.75e-6, c: -0.01e-6 },
    Term { nfa: [0, 0, 2, -2, 1, 0, 0, 0], s: -11.21e-6, c: -0.01e-6 },
    Term { nfa: [0, 0, 2, -2, 2, 0, 0, 0], s: 4.57e-6, c: 0.00e-6 },
    Term { nfa: [0, 0, 2, 0, 3, 0, 0, 0], s: -2.02e-6, c: 0.00e-6 },
    Term { nfa: [0, 0, 2, 0, 1, 0, 0, 0], s: -1.98e-6, c: 0.00e-6 },
    Term { nfa: [0, 0, 0, 0, 3, 0, 0, 0], s: 1.72e-6, c: 0.00e-6 },
    Term { nfa: [0, 1, 0, 0, 1, 0, 0, 0], s: 1.41e-6, c: 0.01e-6 },
    Term { nfa: [0, 1, 0, 0, -1, 0, 0, 0], s: 1.26e-6, c: 0.01e-6 },
    Term { nfa: [1, 0, 0, 0, -1, 0, 0, 0], s: 0.63e-6, c: 0.00e-6 },
    Term { nfa: [1, 0, 0, 0, 1, 0, 0, 0], s: 0.63e-6, c: 0.00e-6 },
    Term { nfa: [0, 1, 2, -2, 3, 0, 0, 0], s: -0.46e-6, c: 0.00e-6 },
    Term { nfa: [0, 1, 2, -2, 1, 0, 0, 0], s: -0.45e-6, c: 0.00e-6 },
    Term { nfa: [0, 0, 4, -4, 4, 0, 0, 0], s: -0.36e-6, c: 0.00e-6 },
    Term { nfa: [0, 0, 1, -1, 1, -8, 12, 0], s: 0.24e-6, c: 0.12e-6 },
    Term { nfa: [0, 0, 2, 0, 0, 0, 0, 0], s: -0.32e-6, c: 0.00e-6 },
    Term { nfa: [0, 0, 2, 0, 2, 0, 0, 0], s: -0.28e-6, c: 0.00e-6 },
    Term { nfa: [1, 0, 2, 0, 3, 0, 0, 0], s: -0.27e-6, c: 0.00e-6 },
    Term { nfa: [1, 0, 2, 0, 1, 0, 0, 0], s: -0.26e-6, c: 0.00e-6 },
    Term { nfa: [0, 0, 2, -2, 0, 0, 0, 0], s: 0.21e-6, c: 0.00e-6 },
    Term { nfa: [0, 1, -2, 2, -3, 0, 0, 0], s: -0.19e-6, c: 0.00e-6 },
    Term { nfa: [0, 1, -2, 2, -1, 0, 0, 0], s: -0.18e-6, c: 0.00e-6 },
    Term { nfa: [0, 0, 0, 0, 0, 8, -13, -1], s: 0.10e-6, c: -0.05e-6 },
    Term { nfa: [0, 0, 0, 2, 0, 0, 0, 0], s: -0.15e-6, c: 0.00e-6 },
    Term { nfa: [2, 0, -2, 0, -1, 0, 0, 0], s: 0.14e-6, c: 0.00e-6 },
    Term { nfa: [0, 1, 2, -2, 2, 0, 0, 0], s: 0.14e-6, c: 0.00e-6 },
    Term { nfa: [1, 0, 0, -2, 1, 0, 0, 0], s: -0.14e-6, c: 0.00e-6 },
    Term { nfa: [1, 0, 0, -2, -1, 0, 0, 0], s: -0.14e-6, c: 0.00e-6 },
    Term { nfa: [0, 0, 4, -2, 4, 0, 0, 0], s: -0.13e-6, c: 0.00e-6 },
    Term { nfa: [0, 0, 2, -2, 4, 0, 0, 0], s: 0.11e-6, c: 0.00e-6 },
    Term { nfa: [1, 0, -2, 0, -3, 0, 0, 0], s: -0.11e-6, c: 0.00e-6 },
    Term { nfa: [1, 0, -2, 0, -1, 0, 0, 0], s: -0.11e-6, c: 0.00e-6 },
];

/// Terms of order t^1 (3).
#[rustfmt::skip]
static S1: [Term; 3] = [
    Term { nfa: [0, 0, 0, 0, 2, 0, 0, 0], s: -0.07e-6, c: 3.57e-6 },
    Term { nfa: [0, 0, 0, 0, 1, 0, 0, 0], s: 1.73e-6, c: -0.03e-6 },
    Term { nfa: [0, 0, 2, -2, 3, 0, 0, 0], s: 0.00e-6, c: 0.48e-6 },
];

/// Terms of order t^2 (25).
#[rustfmt::skip]
static S2: [Term; 25] = [
    Term { nfa: [0, 0, 0, 0, 1, 0, 0, 0], s: 743.52e-6, c: -0.17e-6 },
    Term { nfa: [0, 0, 2, -2, 2, 0, 0, 0], s: 56.91e-6, c: 0.06e-6 },
    Term { nfa: [0, 0, 2, 0, 2, 0, 0, 0], s: 9.84e-6, c: -0.01e-6 },
    Term { nfa: [0, 0, 0, 0, 2, 0, 0, 0], s: -8.85e-6, c: 0.01e-6 },
    Term { nfa: [0, 1, 0, 0, 0, 0, 0, 0], s: -6.38e-6, c: -0.05e-6 },
    Term { nfa: [1, 0, 0, 0, 0, 0, 0, 0], s: -3.07e-6, c: 0.00e-6 },
    Term { nfa: [0, 1, 2, -2, 2, 0, 0, 0], s: 2.23e-6, c: 0.00e-6 },
    Term { nfa: [0, 0, 2, 0, 1, 0, 0, 0], s: 1.67e-6, c: 0.00e-6 },
    Term { nfa: [1, 0, 2, 0, 2, 0, 0, 0], s: 1.30e-6, c: 0.00e-6 },
    Term { nfa: [0, 1, -2, 2, -2, 0, 0, 0], s: 0.93e-6, c: 0.00e-6 },
    Term { nfa: [1, 0, 0, -2, 0, 0, 0, 0], s: 0.68e-6, c: 0.00e-6 },
    Term { nfa: [0, 0, 2, -2, 1, 0, 0, 0], s: -0.55e-6, c: 0.00e-6 },
    Term { nfa: [1, 0, -2, 0, -2, 0, 0, 0], s: 0.53e-6, c: 0.00e-6 },
    Term { nfa: [0, 0, 0, 2, 0, 0, 0, 0], s: -0.27e-6, c: 0.00e-6 },
    Term { nfa: [1, 0, 0, 0, 1, 0, 0, 0], s: -0.27e-6, c: 0.00e-6 },
    Term { nfa: [1, 0, -2, -2, -2, 0, 0, 0], s: -0.26e-6, c: 0.00e-6 },
    Term { nfa: [1, 0, 0, 0, -1, 0, 0, 0], s: -0.25e-6, c: 0.00e-6 },
    Term { nfa: [1, 0, 2, 0, 1, 0, 0, 0], s: 0.22e-6, c: 0.00e-6 },
    Term { nfa: [2, 0, 0, -2, 0, 0, 0, 0], s: -0.21e-6, c: 0.00e-6 },
    Term { nfa: [2, 0, -2, 0, -1, 0, 0, 0], s: 0.20e-6, c: 0.00e-6 },
    Term { nfa: [0, 0, 2, 2, 2, 0, 0, 0], s: 0.17e-6, c: 0.00e-6 },
    Term { nfa: [2, 0, 2, 0, 2, 0, 0, 0], s: 0.13e-6, c: 0.00e-6 },
    Term { nfa: [2, 0, 0, 0, 0, 0, 0, 0], s: -0.13e-6, c: 0.00e-6 },
    Term { nfa: [1, 0, 2, -2, 2, 0, 0, 0], s: -0.12e-6, c: 0.00e-6 },
    Term { nfa: [0, 0, 2, 0, 0, 0, 0, 0], s: -0.11e-6, c: 0.00e-6 },
];

/// Terms of order t^3 (4).
#[rustfmt::skip]
static S3: [Term; 4] = [
    Term { nfa: [0, 0, 0, 0, 1, 0, 0, 0], s: 0.30e-6, c: -23.42e-6 },
    Term { nfa: [0, 0, 2, -2, 2, 0, 0, 0], s: -0.03e-6, c: -1.46e-6 },
    Term { nfa: [0, 0, 2, 0, 2, 0, 0, 0], s: -0.01e-6, c: -0.25e-6 },
    Term { nfa: [0, 0, 0, 0, 2, 0, 0, 0], s: 0.00e-6, c: 0.23e-6 },
];

/// Terms of order t^4 (1).
#[rustfmt::skip]
static S4: [Term; 1] = [
    Term { nfa: [0, 0, 0, 0, 1, 0, 0, 0], s: -0.26e-6, c: -0.01e-6 },
];

/// The sum of one series of `s06`, from the last term to the first, onto `w`.
fn s06_series(mut w: f64, terms: &[Term], fa: &[f64; 8]) -> f64 {
    for term in terms.iter().rev() {
        let mut a = 0.0;
        for (n, f) in term.nfa.iter().zip(fa) {
            a += *n as f64 * f;
        }
        w += term.s * math::sin(a) + term.c * math::cos(a);
    }
    w
}

/// The CIO locator s, positioning the Celestial Intermediate Origin on the equator
/// of the Celestial Intermediate Pole, given the CIP's X,Y coordinates; IAU 2006
/// precession and IAU 2000A nutation (`s06.c`).
pub fn s06(date1: f64, date2: f64, x: f64, y: f64) -> f64 {
    // Interval between fundamental epoch J2000.0 and current date (JC).
    let t = ((date1 - DJ00) + date2) / DJC;

    // Fundamental Arguments (from IERS Conventions 2003).
    let fa = [
        fal03(t),
        falp03(t),
        faf03(t),
        fad03(t),
        faom03(t),
        fave03(t),
        fae03(t),
        fapa03(t),
    ];

    // Evaluate s.
    let w0 = s06_series(SP[0], &S0, &fa);
    let w1 = s06_series(SP[1], &S1, &fa);
    let w2 = s06_series(SP[2], &S2, &fa);
    let w3 = s06_series(SP[3], &S3, &fa);
    let w4 = s06_series(SP[4], &S4, &fa);
    let w5 = SP[5];

    (w0 + (w1 + (w2 + (w3 + (w4 + w5 * t) * t) * t) * t) * t) * DAS2R - x * y / 2.0
}

/// The Celestial Intermediate Pole coordinates X,Y and the CIO locator s, IAU 2006
/// precession and IAU 2000A nutation: returns `(x, y, s)` (`xys06a.c`).
pub fn xys06a(date1: f64, date2: f64) -> (f64, f64, f64) {
    let rbpn = pnm06a(date1, date2);
    let (x, y) = bpn2xy(&rbpn);
    let s = s06(date1, date2, x, y);
    (x, y, s)
}

/// Form the celestial to intermediate-frame-of-date matrix given the CIP X,Y and the
/// CIO locator s (`c2ixys.c`).
pub fn c2ixys(x: f64, y: f64, s: f64) -> Mat {
    // Obtain the spherical angles E and d.
    let r2 = x * x + y * y;
    let e = if r2 > 0.0 { math::atan2(y, x) } else { 0.0 };
    let d = math::atan((r2 / (1.0 - r2)).sqrt());

    // Form the matrix.
    let mut rc2i = [[0.0; 3]; 3];
    ir(&mut rc2i);
    rz(e, &mut rc2i);
    ry(d, &mut rc2i);
    rz(-(e + s), &mut rc2i);
    rc2i
}

/// Earth rotation angle (IAU 2000 model) from the two-part UT1 Julian date
/// (`era00.c`).
pub fn era00(dj1: f64, dj2: f64) -> f64 {
    // Days since fundamental epoch.
    let (d1, d2) = if dj1 < dj2 { (dj1, dj2) } else { (dj2, dj1) };
    let t = d1 + (d2 - DJ00);

    // Fractional part of T (days).
    let f = d1 % 1.0 + d2 % 1.0;

    // Earth rotation angle at this UT1.
    anp(D2PI * (f + 0.7790572732640 + 0.00273781191135448 * t))
}

/// The TIO locator s', positioning the Terrestrial Intermediate Origin on the
/// equator of the Celestial Intermediate Pole (`sp00.c`).
pub fn sp00(date1: f64, date2: f64) -> f64 {
    // Interval between fundamental epoch J2000.0 and current date (JC).
    let t = ((date1 - DJ00) + date2) / DJC;

    // Approximate s'.
    -47e-6 * t * DAS2R
}

/// Form the matrix of polar motion for a given date, IAU 2000 (`pom00.c`).
pub fn pom00(xp: f64, yp: f64, sp: f64) -> Mat {
    let mut rpom = [[0.0; 3]; 3];
    ir(&mut rpom);
    rz(sp, &mut rpom);
    ry(-xp, &mut rpom);
    rx(-yp, &mut rpom);
    rpom
}

/// Initialize an r-matrix to the identity matrix (`ir.c`).
pub fn ir(r: &mut Mat) {
    r[0][0] = 1.0;
    r[0][1] = 0.0;
    r[0][2] = 0.0;
    r[1][0] = 0.0;
    r[1][1] = 1.0;
    r[1][2] = 0.0;
    r[2][0] = 0.0;
    r[2][1] = 0.0;
    r[2][2] = 1.0;
}

/// Rotate an r-matrix about the x-axis (`rx.c`).
pub fn rx(phi: f64, r: &mut Mat) {
    let s = math::sin(phi);
    let c = math::cos(phi);

    let a10 = c * r[1][0] + s * r[2][0];
    let a11 = c * r[1][1] + s * r[2][1];
    let a12 = c * r[1][2] + s * r[2][2];
    let a20 = -s * r[1][0] + c * r[2][0];
    let a21 = -s * r[1][1] + c * r[2][1];
    let a22 = -s * r[1][2] + c * r[2][2];

    r[1][0] = a10;
    r[1][1] = a11;
    r[1][2] = a12;
    r[2][0] = a20;
    r[2][1] = a21;
    r[2][2] = a22;
}

/// Rotate an r-matrix about the y-axis (`ry.c`).
pub fn ry(theta: f64, r: &mut Mat) {
    let s = math::sin(theta);
    let c = math::cos(theta);

    let a00 = c * r[0][0] - s * r[2][0];
    let a01 = c * r[0][1] - s * r[2][1];
    let a02 = c * r[0][2] - s * r[2][2];
    let a20 = s * r[0][0] + c * r[2][0];
    let a21 = s * r[0][1] + c * r[2][1];
    let a22 = s * r[0][2] + c * r[2][2];

    r[0][0] = a00;
    r[0][1] = a01;
    r[0][2] = a02;
    r[2][0] = a20;
    r[2][1] = a21;
    r[2][2] = a22;
}

/// Rotate an r-matrix about the z-axis (`rz.c`).
pub fn rz(psi: f64, r: &mut Mat) {
    let s = math::sin(psi);
    let c = math::cos(psi);

    let a00 = c * r[0][0] + s * r[1][0];
    let a01 = c * r[0][1] + s * r[1][1];
    let a02 = c * r[0][2] + s * r[1][2];
    let a10 = -s * r[0][0] + c * r[1][0];
    let a11 = -s * r[0][1] + c * r[1][1];
    let a12 = -s * r[0][2] + c * r[1][2];

    r[0][0] = a00;
    r[0][1] = a01;
    r[0][2] = a02;
    r[1][0] = a10;
    r[1][1] = a11;
    r[1][2] = a12;
}

/// Multiply two r-matrices: returns `a·b` (`rxr.c`).
pub fn rxr(a: &Mat, b: &Mat) -> Mat {
    let mut atb = [[0.0; 3]; 3];
    for (i, row) in atb.iter_mut().enumerate() {
        for (j, out) in row.iter_mut().enumerate() {
            let mut w = 0.0;
            for k in 0..3 {
                w += a[i][k] * b[k][j];
            }
            *out = w;
        }
    }
    atb
}

/// Transpose an r-matrix (`tr.c`).
pub fn tr(r: &Mat) -> Mat {
    let mut rt = [[0.0; 3]; 3];
    for (i, row) in rt.iter_mut().enumerate() {
        for (j, out) in row.iter_mut().enumerate() {
            *out = r[j][i];
        }
    }
    rt
}
