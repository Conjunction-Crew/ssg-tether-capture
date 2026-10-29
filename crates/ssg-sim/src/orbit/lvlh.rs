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
#[path = "../tests/orbit_lvlh.rs"]
mod tests;
