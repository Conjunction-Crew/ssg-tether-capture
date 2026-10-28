//! The local-vertical/local-horizontal (LVLH) frame of an orbit, and relative
//! dynamics about it (F-09, F-35).
//!
//! Axes: `radial` = r̂, `cross` = (r × v)/|r × v| (orbit normal), `along` =
//! cross × radial (along-track; the velocity direction on a circular orbit). The
//! frame rotates with `omega` = (r × v)/|r|².
//!
//! The physics bubble keeps ECI axes and translates with the anchor's orbit, so
//! relative motion in it needs only [`differential_gravity`]: CW's Coriolis and
//! centrifugal terms belong to the rotating LVLH frame and do not exist in the
//! bubble. [`cw_gravity_gradient`] (the linearised form) and [`cw_solution`] (the
//! closed-form Clohessy-Wiltshire solution) are for the tests and the docs.

use bevy::math::DVec3;
use brahe::GM_EARTH;
use nalgebra::Vector6;

/// The LVLH frame of an orbit at one instant, with ECI-axis unit vectors.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LvlhFrame {
    pub origin_r: DVec3,
    pub origin_v: DVec3,
    pub radial: DVec3,
    pub along: DVec3,
    pub cross: DVec3,
    pub omega: DVec3,
}

impl LvlhFrame {
    /// The frame of the ECI state `[r, v]`; `None` when `|r|` or `|r × v|` is 0.
    pub fn from_eci(state: &Vector6<f64>) -> Option<Self> {
        let r = DVec3::new(state[0], state[1], state[2]);
        let v = DVec3::new(state[3], state[4], state[5]);
        let h = r.cross(v);
        let r2 = r.length_squared();
        if r2 == 0.0 || h.length_squared() == 0.0 {
            return None;
        }
        let radial = r.normalize();
        let cross = h.normalize();
        Some(Self {
            origin_r: r,
            origin_v: v,
            radial,
            along: cross.cross(radial),
            cross,
            omega: h / r2,
        })
    }

    /// The velocity of a body at `rel_r_eci` from the origin, moving at
    /// `rel_v_eci` relative to it, relative to the point that co-rotates with the
    /// frame: `rel_v − omega × rel_r`, still in ECI axes. A body at rest in LVLH
    /// gives zero, where the plain ECI difference gives `|omega × rel_r|` (F-35).
    pub fn relative_velocity_in_frame(&self, rel_r_eci: DVec3, rel_v_eci: DVec3) -> DVec3 {
        rel_v_eci - self.omega.cross(rel_r_eci)
    }

    /// The components of the ECI-axis vector `v_eci` along (radial, along, cross).
    pub fn to_lvlh(&self, v_eci: DVec3) -> DVec3 {
        DVec3::new(
            v_eci.dot(self.radial),
            v_eci.dot(self.along),
            v_eci.dot(self.cross),
        )
    }

    /// The ECI-axis vector with (radial, along, cross) components `v_lvlh`.
    pub fn to_eci(&self, v_lvlh: DVec3) -> DVec3 {
        self.radial * v_lvlh.x + self.along * v_lvlh.y + self.cross * v_lvlh.z
    }

    /// The frame's angular rate `|omega|`: the mean motion on a circular orbit.
    pub fn mean_motion(&self) -> f64 {
        self.omega.length()
    }
}

/// The two-body acceleration of a body at `offset_eci` from a point on an orbit
/// at `origin_r_eci`, relative to that point: `μ·(r_o/|r_o|³ − (r_o + p)/|r_o + p|³)`
/// with `μ` = brahe's `GM_EARTH`, the constant the Kepler propagators use. (The GPU
/// map shader uses `3.9860044e14`.)
pub fn differential_gravity(origin_r_eci: DVec3, offset_eci: DVec3) -> DVec3 {
    let r = origin_r_eci + offset_eci;
    GM_EARTH * (origin_r_eci / origin_r_eci.length().powi(3) - r / r.length().powi(3))
}

/// The linearised form of [`differential_gravity`], CW's gravity-gradient term:
/// `(μ/|r_o|³)·(3(p·r̂)r̂ − p)`. Tests and docs only.
pub fn cw_gravity_gradient(origin_r_eci: DVec3, offset_eci: DVec3) -> DVec3 {
    let r = origin_r_eci.length();
    let radial = origin_r_eci / r;
    GM_EARTH / r.powi(3) * (3.0 * offset_eci.dot(radial) * radial - offset_eci)
}

/// The closed-form Clohessy-Wiltshire solution about a circular orbit of mean
/// motion `n`: position and velocity at `t` from `(x0, v0)`, all as (radial,
/// along, cross) components. Tests and docs only.
pub fn cw_solution(n: f64, x0: DVec3, v0: DVec3, t: f64) -> (DVec3, DVec3) {
    let (s, c) = (n * t).sin_cos();
    let nt = n * t;
    let position = DVec3::new(
        (4.0 - 3.0 * c) * x0.x + s / n * v0.x + 2.0 / n * (1.0 - c) * v0.y,
        6.0 * (s - nt) * x0.x + x0.y - 2.0 / n * (1.0 - c) * v0.x + (4.0 * s - 3.0 * nt) / n * v0.y,
        x0.z * c + v0.z / n * s,
    );
    let velocity = DVec3::new(
        3.0 * n * s * x0.x + c * v0.x + 2.0 * s * v0.y,
        6.0 * n * (c - 1.0) * x0.x - 2.0 * s * v0.x + (4.0 * c - 3.0) * v0.y,
        -x0.z * n * s + v0.z * c,
    );
    (position, velocity)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orbit::ISS_ORBIT;
    use crate::test_support::{fixed_epoch, kepler_eci};

    fn iss_frame() -> LvlhFrame {
        LvlhFrame::from_eci(&kepler_eci(ISS_ORBIT, fixed_epoch())).expect("ISS frame")
    }

    #[test]
    fn lvlh_triad_is_orthonormal_right_handed() {
        let f = iss_frame();
        for (name, axis) in [("radial", f.radial), ("along", f.along), ("cross", f.cross)] {
            assert!(
                (axis.length() - 1.0).abs() < 1e-12,
                "|{name}| = {}",
                axis.length()
            );
        }
        for (name, dot) in [
            ("radial·along", f.radial.dot(f.along)),
            ("radial·cross", f.radial.dot(f.cross)),
            ("along·cross", f.along.dot(f.cross)),
        ] {
            assert!(dot.abs() < 1e-12, "{name} = {dot:e}");
        }
        assert!(
            (f.radial.cross(f.along) - f.cross).length() < 1e-12,
            "radial × along = {:?}, cross = {:?}",
            f.radial.cross(f.along),
            f.cross
        );
        // The round trip through LVLH components.
        let v = DVec3::new(1.0, -2.0, 3.0);
        assert!((f.to_eci(f.to_lvlh(v)) - v).length() < 1e-12);
    }

    #[test]
    fn lvlh_rest_velocity_is_zero_for_corotating_body() {
        // F-35: fixed in P4. A body 3,536 m along-track at rest in LVLH.
        let f = iss_frame();
        let rel_r = f.along * 3_536.0;
        let rel_v = f.omega.cross(rel_r);
        // The plain ECI difference: the ≈ 4 m/s artefact.
        assert!(
            rel_v.length() > 3.5,
            "ECI relative speed {}",
            rel_v.length()
        );
        let in_frame = f.relative_velocity_in_frame(rel_r, rel_v);
        assert!(
            in_frame.length() < 1e-12,
            "LVLH relative velocity {in_frame:?}"
        );
    }

    #[test]
    fn relative_gravity_matches_cw_tensor_at_100m() {
        let f = iss_frame();
        for (p, tol) in [(100.0, 1e-8), (3_500.0, 2e-5)] {
            for (name, axis) in [("radial", f.radial), ("along", f.along), ("cross", f.cross)] {
                let offset = axis * p;
                let exact = differential_gravity(f.origin_r, offset);
                let linear = cw_gravity_gradient(f.origin_r, offset);
                let diff = (exact - linear).length();
                assert!(
                    diff < tol,
                    "{name} {p} m: |differential − gradient| = {diff:e} m/s² (tol {tol:e})"
                );
            }
        }
        assert_eq!(differential_gravity(f.origin_r, DVec3::ZERO), DVec3::ZERO);
    }

    /// A free particle in the translating, ECI-axis bubble frame under
    /// [`differential_gravity`] about a circular orbit (a = 6.8e6 m), RK4 at 0.5 s
    /// for one period, starting at rest in LVLH. Returns the largest distance from
    /// the CW solution, sampled every 200 s and at the end (the planner's probe,
    /// `plans/stage-4/results/cw_check.py`).
    fn cw_max_error(x0: DVec3) -> f64 {
        const A: f64 = 6.8e6;
        const DT: f64 = 0.5;
        let n = (GM_EARTH / A.powi(3)).sqrt();
        let reference = |t: f64| {
            let (s, c) = (n * t).sin_cos();
            Vector6::new(A * c, A * s, 0.0, -A * n * s, A * n * c, 0.0)
        };
        let origin = |t: f64| {
            let r = reference(t);
            DVec3::new(r[0], r[1], r[2])
        };
        let frame0 = LvlhFrame::from_eci(&reference(0.0)).unwrap();
        let mut p = frame0.to_eci(x0);
        let mut v = frame0.omega.cross(p);

        let steps = (2.0 * std::f64::consts::PI / n / DT) as usize;
        let mut t = 0.0;
        let mut worst: f64 = 0.0;
        for k in 0..steps {
            let a = |t: f64, p: DVec3| differential_gravity(origin(t), p);
            let (k1p, k1v) = (v, a(t, p));
            let (k2p, k2v) = (v + 0.5 * DT * k1v, a(t + 0.5 * DT, p + 0.5 * DT * k1p));
            let (k3p, k3v) = (v + 0.5 * DT * k2v, a(t + 0.5 * DT, p + 0.5 * DT * k2p));
            let (k4p, k4v) = (v + DT * k3v, a(t + DT, p + DT * k3p));
            p += DT / 6.0 * (k1p + 2.0 * k2p + 2.0 * k3p + k4p);
            v += DT / 6.0 * (k1v + 2.0 * k2v + 2.0 * k3v + k4v);
            t += DT;
            if k % 400 == 0 || k == steps - 1 {
                let frame = LvlhFrame::from_eci(&reference(t)).unwrap();
                let (cw, _) = cw_solution(n, x0, DVec3::ZERO, t);
                worst = worst.max((frame.to_lvlh(p) - cw).length());
            }
        }
        worst
    }

    #[test]
    fn free_particle_follows_cw_solution_one_orbit() {
        let along = cw_max_error(DVec3::new(0.0, 100.0, 0.0));
        let cross = cw_max_error(DVec3::new(0.0, 0.0, 100.0));
        let radial = cw_max_error(DVec3::new(100.0, 0.0, 0.0));
        println!(
            "free particle vs CW, max |error| over one orbit: \
             radial {radial:.4} m, along-track {along:.4} m, cross-track {cross:.4} m"
        );
        assert!(along < 0.1, "along-track 100 m: {along} m");
        assert!(cross < 0.1, "cross-track 100 m: {cross} m");
        // CW's own linearisation error for a 100 m radial offset is ≈ 1.1 m.
        assert!(radial < 1.5, "radial 100 m: {radial} m");
    }
}
