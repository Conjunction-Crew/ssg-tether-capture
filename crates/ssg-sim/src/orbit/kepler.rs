//! A two-body propagator about the Earth, on portable math.
//!
//! [`TwoBody`] keeps the state it was given and advances it with the
//! universal-variable form of Kepler's equation: Newton's iteration on the
//! universal anomaly, then the f and g functions (Vallado, *Fundamentals of
//! Astrodynamics and Applications*, algorithm 8 "KEPLER"). It holds no orbital
//! elements, so a small eccentricity is never rounded to zero (F-69) and the
//! state at `dt = 0` is the stored state, bit for bit. Every transcendental call
//! goes through [`crate::math`], so the same input gives the same bits on every
//! platform (F-48).
//!
//! [`elements_to_state`] follows brahe 1.3.4's `state_koe_to_eci`;
//! [`state_to_elements`] works from the eccentricity vector and does not round.

use std::f64::consts::PI;

use brahe::{Epoch, GM_EARTH};
use nalgebra::{Vector3, Vector6};

use crate::math;

/// A two-body orbit about the Earth: a state (m, m/s, ECI) at an epoch.
#[derive(Debug, Clone)]
pub struct TwoBody {
    epoch: Epoch,
    state: Vector6<f64>,
}

impl TwoBody {
    /// The orbit through `state` (position m, velocity m/s) at `epoch`. The state
    /// is kept as given.
    pub fn from_eci(epoch: Epoch, state: Vector6<f64>) -> Self {
        Self { epoch, state }
    }

    /// The orbit with osculating `elements` at `epoch`: `[a m, e, i, raan, argp,
    /// M]`, angles in radians. Converted once by [`elements_to_state`].
    pub fn from_elements(epoch: Epoch, elements: Vector6<f64>) -> Self {
        Self {
            epoch,
            state: elements_to_state(elements),
        }
    }

    /// The epoch of the stored state.
    pub fn initial_epoch(&self) -> Epoch {
        self.epoch
    }

    /// The state at `epoch`, `epoch - initial_epoch()` seconds from the stored
    /// one (either direction). `Err` when the iteration does not converge.
    pub fn state_eci(&self, epoch: Epoch) -> Result<Vector6<f64>, String> {
        let dt = epoch - self.epoch;
        universal_step(&self.state, dt)
    }

    /// Osculating elements at `epoch` (the orbit drawing; not on the step path).
    pub fn elements_at(&self, epoch: Epoch) -> Result<Vector6<f64>, String> {
        self.state_eci(epoch).map(state_to_elements)
    }
}

/// Stumpff functions c2 and c3 of `psi`.
fn stumpff(psi: f64) -> (f64, f64) {
    if psi > 1e-2 {
        let x = psi.sqrt();
        let half = math::sin(0.5 * x);
        (2.0 * half * half / psi, (x - math::sin(x)) / (x * psi))
    } else if psi < -1e-2 {
        let x = (-psi).sqrt();
        (
            (1.0 - math::cosh(x)) / psi,
            (math::sinh(x) - x) / (x * -psi),
        )
    } else {
        // Series: c2 = sum (-psi)^k / (2k+2)!, c3 = sum (-psi)^k / (2k+3)!.
        let p = -psi;
        let c2 = 1.0 / 2.0
            + p * (1.0 / 24.0
                + p * (1.0 / 720.0
                    + p * (1.0 / 40_320.0
                        + p * (1.0 / 3_628_800.0
                            + p * (1.0 / 479_001_600.0 + p * (1.0 / 87_178_291_200.0))))));
        let c3 = 1.0 / 6.0
            + p * (1.0 / 120.0
                + p * (1.0 / 5_040.0
                    + p * (1.0 / 362_880.0
                        + p * (1.0 / 39_916_800.0
                            + p * (1.0 / 6_227_020_800.0 + p * (1.0 / 1_307_674_368_000.0))))));
        (c2, c3)
    }
}

/// The state `dt` seconds after `state` on its two-body orbit about the Earth.
fn universal_step(state: &Vector6<f64>, dt: f64) -> Result<Vector6<f64>, String> {
    if dt == 0.0 {
        return Ok(*state);
    }
    let r0 = Vector3::new(state[0], state[1], state[2]);
    let v0 = Vector3::new(state[3], state[4], state[5]);
    let sqrt_mu = GM_EARTH.sqrt();
    let r0n = r0.norm();
    let rdotv = r0.dot(&v0);
    let alpha = 2.0 / r0n - v0.dot(&v0) / GM_EARTH;

    // Newton's iteration on the universal anomaly.
    let mut chi = sqrt_mu * alpha.abs() * dt;
    let mut converged = false;
    let mut last_delta = f64::INFINITY;
    for _ in 0..50 {
        let psi = chi * chi * alpha;
        let (c2, c3) = stumpff(psi);
        let chi2 = chi * chi;
        let f = rdotv / sqrt_mu * chi2 * c2 + (1.0 - alpha * r0n) * chi2 * chi * c3 + r0n * chi
            - sqrt_mu * dt;
        let df = rdotv / sqrt_mu * chi * (1.0 - psi * c3) + (1.0 - alpha * r0n) * chi2 * c2 + r0n;
        let delta = f / df;
        chi -= delta;
        last_delta = delta;
        if delta.abs() <= 4.0 * f64::EPSILON * chi.abs() {
            converged = true;
            break;
        }
    }
    // On eccentric orbits Newton can end in a two-value cycle a few ULP wide, just
    // above the test above (|delta| up to about 7 eps |chi| at e = 0.73); that chi
    // is converged too. It is accepted only after the loop, so a case that meets
    // the test exits at the same iteration and keeps its bits.
    if !converged && last_delta.abs() <= 16.0 * f64::EPSILON * chi.abs() {
        converged = true;
    }
    if !converged {
        return Err(format!("universal Kepler: no convergence (dt {dt} s)"));
    }
    let psi = chi * chi * alpha;
    let (c2, c3) = stumpff(psi);
    let chi2 = chi * chi;

    let f = 1.0 - chi2 / r0n * c2;
    let g = dt - chi2 * chi / sqrt_mu * c3;
    let r = f * r0 + g * v0;
    let rn = r.norm();
    let fdot = sqrt_mu / (rn * r0n) * chi * (psi * c3 - 1.0);
    let gdot = 1.0 - chi2 / rn * c2;
    let v = fdot * r0 + gdot * v0;
    Ok(Vector6::new(r[0], r[1], r[2], v[0], v[1], v[2]))
}

/// Eccentric anomaly from mean anomaly `anm_mean` (radians) and eccentricity `e`
/// by Newton's iteration (brahe 1.3.4's `anomaly_mean_to_eccentric`).
fn anomaly_mean_to_eccentric(anm_mean: f64, e: f64) -> f64 {
    let eps = 100.0 * f64::EPSILON;
    let m = anm_mean % (2.0 * PI);
    let mut ecc = if e < 0.8 { m } else { PI };
    let mut f = ecc - e * math::sin(ecc) - m;
    let mut i = 0;
    while f.abs() > eps {
        f = ecc - e * math::sin(ecc) - m;
        ecc -= f / (1.0 - e * math::cos(ecc));
        i += 1;
        if i > 10 {
            break;
        }
    }
    ecc
}

/// The ECI state (m, m/s) of osculating elements `[a m, e, i, raan, argp, M]`,
/// angles in radians (brahe 1.3.4's `state_koe_to_eci`).
#[allow(non_snake_case)]
pub fn elements_to_state(elements: Vector6<f64>) -> Vector6<f64> {
    let x = elements;
    let (a, e, i, raan, omega, m) = (x[0], x[1], x[2], x[3], x[4], x[5]);
    let E = anomaly_mean_to_eccentric(m, e);
    let (so, co) = (math::sin(omega), math::cos(omega));
    let (sr, cr) = (math::sin(raan), math::cos(raan));
    let (si, ci) = (math::sin(i), math::cos(i));
    let (sE, cE) = (math::sin(E), math::cos(E));
    let P = Vector3::new(co * cr - so * ci * sr, co * sr + so * ci * cr, so * si);
    let Q = Vector3::new(-so * cr - co * ci * sr, -so * sr + co * ci * cr, co * si);
    let p = a * (cE - e) * P + a * (1.0 - e * e).sqrt() * sE * Q;
    let v = (GM_EARTH * a).sqrt() / p.norm() * (-sE * P + (1.0 - e * e).sqrt() * cE * Q);
    Vector6::new(p[0], p[1], p[2], v[0], v[1], v[2])
}

/// Osculating elements `[a m, e, i, raan, argp, M]` (radians, each angle in
/// `[0, 2π)`) of an ECI state, from the eccentricity vector. No eccentricity is
/// rounded: a near-circular state keeps its small `e` (F-69).
#[allow(non_snake_case)]
pub fn state_to_elements(state: Vector6<f64>) -> Vector6<f64> {
    let x = state;
    let r = Vector3::new(x[0], x[1], x[2]);
    let v = Vector3::new(x[3], x[4], x[5]);
    let rn = r.norm();
    let h = r.cross(&v);
    let W = h / h.norm();
    let i = math::atan2((W[0] * W[0] + W[1] * W[1]).sqrt(), W[2]);
    let raan = math::atan2(W[0], -W[1]);
    // In-plane basis: N toward the ascending node, T = W x N.
    let N = Vector3::new(math::cos(raan), math::sin(raan), 0.0);
    let T = W.cross(&N);
    let a = 1.0 / (2.0 / rn - v.dot(&v) / GM_EARTH);
    let ev = ((v.dot(&v) - GM_EARTH / rn) * r - r.dot(&v) * v) / GM_EARTH;
    let e = ev.norm();
    let omega = math::atan2(ev.dot(&T), ev.dot(&N));
    let u = math::atan2(r.dot(&T), r.dot(&N));
    let nu = u - omega;
    let E = math::atan2((1.0 - e * e).sqrt() * math::sin(nu), e + math::cos(nu));
    let M = E - e * math::sin(E);
    let two_pi = 2.0 * PI;
    let wrap = |x: f64| ((x % two_pi) + two_pi) % two_pi;
    Vector6::new(a, e, i, wrap(raan), wrap(omega), wrap(M))
}
