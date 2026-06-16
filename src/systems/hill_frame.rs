//! Clohessy–Wiltshire / Hill rotating-frame helpers.
//!
//! Used by the propagation sim to (a) lay out and initialize tether nodes for a
//! chosen initial orientation and (b) drive the joints+tension tether with
//! linearized relative-orbital accelerations (gravity gradient + Coriolis).
//!
//! Axis convention (the user-facing CW axes):
//!   * CW X = nadir = `-r_hat`               (toward Earth centre)
//!   * CW Y = along-track = `h_hat × r_hat`   (≈ orbital velocity direction)
//!   * CW Z = `cw_x × cw_y`                    (right-handed orthonormal triad)
//!
//! The standard Hill equations are written about the *radial-out* axis
//! (`-cw_x`), the *along-track* axis (`cw_y`) and the *cross-track* axis (the
//! orbit normal `h_hat`). [`HillBasis::hill_acceleration`] converts into those
//! axes, applies the textbook law, and converts back, so the nadir convention
//! never leaks sign errors into the force law.

use bevy::math::DVec3;
use brahe::GM_EARTH;
use nalgebra::Vector6;

/// An orthonormal Hill / Clohessy–Wiltshire basis built from a reference ECI state.
#[derive(Debug, Clone, Copy)]
pub struct HillBasis {
    /// CW X = nadir (toward Earth centre), unit vector in ECI.
    pub cw_x: DVec3,
    /// CW Y = along-track (≈ velocity direction), unit vector in ECI.
    pub cw_y: DVec3,
    /// CW Z = `cw_x × cw_y`, unit vector in ECI.
    pub cw_z: DVec3,
    /// Reference ECI position (m).
    pub r: DVec3,
    /// Reference ECI velocity (m/s).
    pub v: DVec3,
    /// Orbit normal `h_hat` (cross-track / standard Hill z), unit vector in ECI.
    pub h_hat: DVec3,
    /// Mean motion of the reference orbit (rad/s).
    pub n: f64,
    /// Orbital angular-velocity vector `ω = n · h_hat` (rad/s, ECI).
    pub omega: DVec3,
}

impl HillBasis {
    /// Build the Hill basis from a reference ECI state `[x,y,z,vx,vy,vz]` (m, m/s)
    /// and the reference semi-major axis `a` (m). Mean motion uses `a` (a constant
    /// for the run) rather than the instantaneous radius.
    pub fn from_reference(rv: Vector6<f64>, semi_major_axis_m: f64) -> Self {
        let r = DVec3::new(rv[0], rv[1], rv[2]);
        let v = DVec3::new(rv[3], rv[4], rv[5]);

        let r_hat = r.normalize_or(DVec3::X);
        let h_hat = r.cross(v).normalize_or(DVec3::Z); // orbit normal
        let along = h_hat.cross(r_hat).normalize_or(DVec3::Y); // ≈ velocity direction

        let cw_x = -r_hat; // nadir
        let cw_y = along; // along-track
        let cw_z = cw_x.cross(cw_y).normalize_or(DVec3::Z);

        let a = semi_major_axis_m.max(1.0);
        let n = (GM_EARTH / (a * a * a)).sqrt();
        let omega = h_hat * n;

        Self {
            cw_x,
            cw_y,
            cw_z,
            r,
            v,
            h_hat,
            n,
            omega,
        }
    }

    /// Unit vector (ECI) for the requested CW axis used as the tether's long axis.
    pub fn axis(&self, radial: bool) -> DVec3 {
        if radial { self.cw_x } else { self.cw_y }
    }

    /// ECI velocity of a point rigidly co-rotating with the reference orbit at
    /// the given ECI offset from the reference: `v = V + ω × offset`.
    pub fn rigid_velocity_eci(&self, offset_eci: DVec3) -> DVec3 {
        self.v + self.omega.cross(offset_eci)
    }

    /// Full ECI state `[x,y,z,vx,vy,vz]` for a node placed `distance` metres along
    /// `axis_hat` (one of [`Self::cw_x`]/[`Self::cw_y`]), rigidly co-rotating with
    /// the reference orbit. This is the clean "set the orientation, then evolve"
    /// initial condition (zero relative velocity in the rotating Hill frame).
    pub fn node_initial_eci(&self, axis_hat: DVec3, distance: f64) -> Vector6<f64> {
        let offset = axis_hat * distance;
        let r = self.r + offset;
        let vel = self.rigid_velocity_eci(offset);
        Vector6::new(r.x, r.y, r.z, vel.x, vel.y, vel.z)
    }

    /// Linearized Clohessy–Wiltshire relative acceleration (m/s²) for a node whose
    /// position/velocity *relative to the reference* are `rel_pos`/`rel_vel`
    /// (ECI/local-frame vectors, as read from Avian3D `LinearVelocity`). Returned
    /// as an ECI vector.
    ///
    /// In standard Hill axes (x = radial-out, y = along-track, z = cross-track):
    /// `ẍ = 3n²x + 2n·ẏ`, `ÿ = −2n·ẋ`, `z̈ = −n²z`.
    ///
    /// The CW equations are written in the *rotating* Hill frame, so `rel_vel`
    /// (an inertial-frame vector) is converted to the rotating frame by subtracting
    /// the frame-drag term `ω × rel_pos` before projecting onto the Hill axes.
    pub fn hill_acceleration(&self, rel_pos: DVec3, rel_vel: DVec3) -> DVec3 {
        let radial_out = -self.cw_x;
        let along = self.cw_y;
        let cross = self.h_hat;

        let x = rel_pos.dot(radial_out);
        let z = rel_pos.dot(cross);

        // Convert inertial relative velocity to rotating-frame (Hill) velocity.
        let rel_vel_hill = rel_vel - self.omega.cross(rel_pos);
        let xd = rel_vel_hill.dot(radial_out);
        let yd = rel_vel_hill.dot(along);

        let n = self.n;
        let ax = 3.0 * n * n * x + 2.0 * n * yd;
        let ay = -2.0 * n * xd;
        let az = -n * n * z;

        ax * radial_out + ay * along + az * cross
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A circular equatorial reference orbit: R along +x, V along +y.
    fn circular_equatorial(a: f64) -> HillBasis {
        let speed = (GM_EARTH / a).sqrt();
        HillBasis::from_reference(Vector6::new(a, 0.0, 0.0, 0.0, speed, 0.0), a)
    }

    #[test]
    fn axes_point_where_expected() {
        let b = circular_equatorial(6_799_130.0);
        // Nadir points back toward Earth (−x), along-track follows velocity (+y).
        assert!((b.cw_x - DVec3::new(-1.0, 0.0, 0.0)).length() < 1e-9);
        assert!((b.cw_y - DVec3::new(0.0, 1.0, 0.0)).length() < 1e-9);
        assert!((b.h_hat - DVec3::new(0.0, 0.0, 1.0)).length() < 1e-9);
        // Orthonormal, right-handed triad.
        assert!((b.cw_x.cross(b.cw_y) - b.cw_z).length() < 1e-9);
    }

    #[test]
    fn mean_motion_matches_period() {
        let a = 6_799_130.0;
        let b = circular_equatorial(a);
        let expected = (GM_EARTH / (a * a * a)).sqrt();
        assert!((b.n - expected).abs() < 1e-12);
    }

    #[test]
    fn hill_acceleration_matches_textbook() {
        let b = circular_equatorial(6_799_130.0);
        let n = b.n;
        let radial_out = -b.cw_x; // +x
        let cross = b.h_hat; // +z

        // 1 m radially out, at Hill-frame rest (v_inertial = ω × offset): ẍ = 3n²x.
        let v_hill_rest_radial = b.omega.cross(radial_out);
        let a_radial = b.hill_acceleration(radial_out, v_hill_rest_radial);
        assert!((a_radial - radial_out * (3.0 * n * n)).length() < 1e-12);

        // 1 m cross-track, at Hill-frame rest: z̈ = −n²z.
        let v_hill_rest_cross = b.omega.cross(cross);
        let a_cross = b.hill_acceleration(cross, v_hill_rest_cross);
        assert!((a_cross - cross * (-(n * n))).length() < 1e-12);
    }
}
