use astrora_core::core::{constants::GM_EARTH, elements::coe_to_rv};
use bevy::{
    color::palettes::css::{BLUE, GREEN},
    math::VectorSpace,
    prelude::*,
};

use crate::{components::orbit::Orbital, constants::MAP_UNITS_TO_M};

pub fn orbital_gizmos(orbitals: Query<&Orbital>, mut gizmos: Gizmos) {
    for orbital in orbitals {
        if let Some(elements) = orbital.elements {
            // Semi-major axis
            let semi_major = elements.a / MAP_UNITS_TO_M as f64;
            // Semi-minor axis
            let semi_minor =
                (elements.periapsis() * elements.apoapsis()).sqrt() / MAP_UNITS_TO_M as f64;
            let (r, v) = coe_to_rv(&elements, GM_EARTH);
            let r_vec = Vec3::new(r.x as f32, r.y as f32, r.z as f32);
            let v_vec = Vec3::new(v.x as f32, v.y as f32, v.z as f32);

            let x_axis = r_vec.normalize_or_zero();
            if x_axis == Vec3::ZERO {
                continue;
            }

            let z_axis = r_vec.cross(v_vec).normalize_or_zero();
            if z_axis == Vec3::ZERO {
                continue;
            }

            let y_axis = z_axis.cross(x_axis).normalize_or_zero();
            if y_axis == Vec3::ZERO {
                continue;
            }

            let rotation = Quat::from_mat3(&Mat3::from_cols(x_axis, y_axis, z_axis));

            // Draw orbital ellipse
            gizmos
                .ellipse(
                    Isometry3d {
                        rotation: rotation,
                        translation: rotation
                            * Vec3A::new(-semi_major as f32 * elements.e as f32, 0.0, 0.0),
                    },
                    Vec2::new(semi_major as f32, semi_minor as f32),
                    Srgba::new(0.0, 0.0, 1.0, 0.1),
                )
                .resolution(512);
        }
    }
}
