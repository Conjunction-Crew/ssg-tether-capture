use astrora_core::core::{Vector3, constants::GM_EARTH, elements::rv_to_coe};
use avian3d::prelude::{LinearVelocity, Position, RigidBodyDisabled};
use bevy::{camera::visibility::RenderLayers, prelude::*};

use crate::{
    components::{
        capture_components::CaptureComponent,
        orbit::{Orbital, TrueParams},
    },
    constants::{MAP_UNITS_TO_M, SCENE_LAYER},
};

pub fn orbital_gizmos(
    orbitals: Query<(
        &TrueParams,
        &Position,
        &LinearVelocity,
        Option<&RigidBodyDisabled>,
    )>,
    camera_s: Single<&RenderLayers, (With<Camera3d>, Without<Orbital>)>,
    mut gizmos: Gizmos,
) {
    let render_layers = camera_s.into_inner();

    // Do not render orbit gizmos in scene view
    if render_layers.intersects(&RenderLayers::layer(SCENE_LAYER)) {
        return;
    }

    for (true_params, r, v, disabled) in orbitals {
        let (r_world, v_world) = if disabled.is_some() {
            (
                Vector3::new(true_params.r[0], true_params.r[1], true_params.r[2]),
                Vector3::new(true_params.v[0], true_params.v[1], true_params.v[2]),
            )
        } else {
            (
                Vector3::new(
                    true_params.r[0] + r.x as f64,
                    true_params.r[1] + r.y as f64,
                    true_params.r[2] + r.z as f64,
                ),
                Vector3::new(
                    true_params.v[0] + v.x as f64,
                    true_params.v[1] + v.y as f64,
                    true_params.v[2] + v.z as f64,
                ),
            )
        };

        if let Ok(elements) = rv_to_coe(&r_world, &v_world, GM_EARTH, 1e-8) {
            if elements.e >= 1.0 {
                continue;
            }

            let map_scale = MAP_UNITS_TO_M as f64;
            let semi_major = (elements.a / map_scale) as f32;
            let semi_minor =
                (elements.a * (1.0 - elements.e * elements.e).sqrt() / map_scale) as f32;

            let rotation = Quat::from_axis_angle(Vec3::Z, elements.raan as f32)
                * Quat::from_axis_angle(Vec3::X, elements.i as f32)
                * Quat::from_axis_angle(Vec3::Z, elements.argp as f32);

            let center_offset = rotation * Vec3::new(-semi_major * elements.e as f32, 0.0, 0.0);

            gizmos
                .ellipse(
                    Isometry3d::new(center_offset, rotation),
                    Vec2::new(semi_major, semi_minor),
                    Srgba::new(0.0, 0.0, 1.0, 0.1),
                )
                .resolution(512);
        }
    }
}
