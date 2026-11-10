// Derived from ERFA v2.0.1 (https://github.com/liberfa/erfa, commit
// 9915ba38c9365f8b0738269b8c2ac1fdd5f8dee3), Copyright (C) 2013-2021, NumFOCUS
// Foundation, BSD-3-Clause; ERFA is derived, with permission, from the SOFA library.
// This file is a translation into Rust; it is not ERFA or SOFA software and is not
// endorsed by either.

//! TEME → GCRF on portable math: the conversion brahe 1.3.4's `SGPPropagator`
//! applies to an SGP4 state (`state_eci`), rebuilt on [`crate::math`] so it gives
//! the same bits on every platform.
//!
//! The chain is brahe's, operation for operation, with zero Earth-orientation
//! parameters (what `orbit::use_zero_eop` sets):
//!
//! 1. TEME → PEF: brahe's GMST-82 polynomial ([`gmst82`]) and a rotation about z;
//!    the velocity loses Earth's rotation, `v_pef = R·v − ω × r_pef`.
//! 2. PEF → ITRF: polar motion, `pom00(0, 0, sp00(t_TT))`.
//! 3. ITRF → GCRF: `r = (PM·ER·BPN)ᵀ·r_itrf`,
//!    `v = (ER·BPN)ᵀ·(PMᵀ·v_itrf + ω × (PMᵀ·r_itrf))`, with BPN from the IAU
//!    2006/2000A CIP and CIO locator (`xys06a`, `c2ixys`) and ER the Earth rotation
//!    angle (`era00`).
//!
//! The ERFA routines are in [`iau2006`]. Matrices and vectors are `nalgebra`
//! types, as in brahe, so the products are formed in brahe's order. Only the three
//! times of [`FrameTimes`] come from brahe's `Epoch`.

pub mod iau2006;
mod nut00a_tables;

use brahe::{Epoch, TimeSystem};
use nalgebra::{Matrix3, Vector3, Vector6};

use crate::math;

/// The Julian date of MJD 0 (brahe's `MJD_ZERO`, ERFA's `ERFA_DJM0`).
const MJD_ZERO: f64 = 2400000.5;
/// Earth's rotation rate, rad/s (brahe 1.3.4's `OMEGA_EARTH`).
const OMEGA_EARTH: f64 = 7.292115146706979e-5;
/// Degrees to radians (brahe 1.3.4's `DEG2RAD`).
const DEG2RAD: f64 = std::f64::consts::PI / 180.0;

/// The three times the chain reads from an epoch.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameTimes {
    /// Julian date, UT1 (GMST-82).
    pub jd_ut1: f64,
    /// Modified Julian date, UT1 (the Earth rotation angle).
    pub mjd_ut1: f64,
    /// Modified Julian date, TT (precession-nutation and polar motion).
    pub mjd_tt: f64,
}

impl FrameTimes {
    /// The three times of `epoch`, from brahe's `Epoch` (with zero
    /// Earth-orientation parameters, UT1 = UTC).
    pub fn from_epoch(epoch: Epoch) -> Self {
        Self {
            jd_ut1: epoch.jd_as_time_system(TimeSystem::UT1),
            mjd_ut1: epoch.mjd_as_time_system(TimeSystem::UT1),
            mjd_tt: epoch.mjd_as_time_system(TimeSystem::TT),
        }
    }
}

/// The GCRF state (m, m/s) of the TEME state `state_teme` (m, m/s) at `epoch`.
pub fn teme_to_gcrf(state_teme: Vector6<f64>, epoch: Epoch) -> Vector6<f64> {
    teme_to_gcrf_at(state_teme, &FrameTimes::from_epoch(epoch))
}

/// The GCRF state (m, m/s) of the TEME state `state_teme` (m, m/s) at `times`.
pub fn teme_to_gcrf_at(state_teme: Vector6<f64>, times: &FrameTimes) -> Vector6<f64> {
    let omega = Vector3::new(0.0, 0.0, OMEGA_EARTH);

    // 1. TEME → PEF.
    let r_gmst = rz(gmst82(times.jd_ut1));
    let r_pef: Vector3<f64> = r_gmst * Vector3::new(state_teme[0], state_teme[1], state_teme[2]);
    let v_pef: Vector3<f64> =
        r_gmst * Vector3::new(state_teme[3], state_teme[4], state_teme[5]) - omega.cross(&r_pef);

    // 2. PEF → ITRF.
    let pm = polar_motion(times.mjd_tt);
    let r_itrf = pm * r_pef;
    let v_itrf = pm * v_pef;

    // 3. ITRF → GCRF.
    let bpn = bias_precession_nutation(times.mjd_tt);
    let er = earth_rotation(times.mjd_ut1);
    let p: Vector3<f64> = (pm * er * bpn).transpose() * r_itrf;
    let v: Vector3<f64> = (er * bpn).transpose()
        * (pm.transpose() * v_itrf + omega.cross(&(pm.transpose() * r_itrf)));

    Vector6::new(p[0], p[1], p[2], v[0], v[1], v[2])
}

/// Greenwich mean sidereal time (IAU 1982), radians in [0, 2π), at the UT1 Julian
/// date `jd_ut1`: brahe 1.3.4's `tle_gmst82`, the angle of the TEME → PEF rotation.
pub fn gmst82(jd_ut1: f64) -> f64 {
    let tut1 = (jd_ut1 - 2451545.0) / 36525.0;

    // GMST in seconds.
    let gmst_sec =
        67310.54841 + (876600.0 * 3600.0 + 8640184.812866) * tut1 + 0.093104 * tut1 * tut1
            - 6.2e-6 * tut1 * tut1 * tut1;

    (gmst_sec * DEG2RAD / 240.0) % (2.0 * std::f64::consts::PI)
}

/// The GCRS → CIRS matrix (bias, precession, nutation; IAU 2006/2000A) at the TT
/// modified Julian date `mjd_tt`: brahe 1.3.4's `bias_precession_nutation` with
/// zero CIP corrections (brahe adds the EOP's dX, dY to X, Y; here they are 0).
pub fn bias_precession_nutation(mjd_tt: f64) -> Matrix3<f64> {
    let (x, y, s) = iau2006::xys06a(MJD_ZERO, mjd_tt);
    matrix3(&iau2006::c2ixys(x, y, s))
}

/// The CIRS → TIRS matrix (the Earth rotation angle, IAU 2000) at the UT1 modified
/// Julian date `mjd_ut1`: brahe 1.3.4's `earth_rotation`.
pub fn earth_rotation(mjd_ut1: f64) -> Matrix3<f64> {
    let mut r = [[0.0; 3]; 3];
    iau2006::ir(&mut r);
    iau2006::rz(iau2006::era00(MJD_ZERO, mjd_ut1), &mut r);
    matrix3(&r)
}

/// The TIRS → ITRF matrix (polar motion, IAU 2000) at the TT modified Julian date
/// `mjd_tt`: brahe 1.3.4's `polar_motion` with zero pole coordinates.
pub fn polar_motion(mjd_tt: f64) -> Matrix3<f64> {
    matrix3(&iau2006::pom00(0.0, 0.0, iau2006::sp00(MJD_ZERO, mjd_tt)))
}

/// brahe 1.3.4's `RotationMatrix::Rz` (radians) on portable math.
fn rz(angle: f64) -> Matrix3<f64> {
    Matrix3::new(
        math::cos(angle),
        math::sin(angle),
        0.0,
        -math::sin(angle),
        math::cos(angle),
        0.0,
        0.0,
        0.0,
        1.0,
    )
}

/// An ERFA matrix (`r[row][column]`) as an `nalgebra` matrix (brahe 1.3.4's
/// `matrix3_from_array`).
fn matrix3(r: &iau2006::Mat) -> Matrix3<f64> {
    Matrix3::new(
        r[0][0], r[0][1], r[0][2], r[1][0], r[1][1], r[1][2], r[2][0], r[2][1], r[2][2],
    )
}
