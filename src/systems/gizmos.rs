use avian3d::prelude::{LinearVelocity, Position, RigidBodyDisabled};
use bevy::{camera::visibility::RenderLayers, prelude::*};
use brahe::{AngleFormat, KeplerianPropagator, utils::DOrbitStateProvider};
use nalgebra::Vector6;

use crate::{
    components::orbit::{Orbital, TrueParams},
    constants::{MAP_UNITS_TO_M, SCENE_LAYER},
    resources::world_time::WorldTime,
};

pub fn orbital_gizmos(
    orbitals: Query<(
        &TrueParams,
        &Position,
        &LinearVelocity,
        Option<&RigidBodyDisabled>,
    )>,
    camera_s: Single<&RenderLayers, (With<Camera3d>, Without<Orbital>)>,
    world_time: Res<WorldTime>,
    mut gizmos: Gizmos,
) {
    let render_layers = camera_s.into_inner();

    // Do not render orbit gizmos in scene view
    if render_layers.intersects(&RenderLayers::layer(SCENE_LAYER)) {
        return;
    }

    for (true_params, r, v, disabled) in orbitals {
        let rv_world = if disabled.is_some() {
            true_params.rv
        } else {
            Vector6::new(
                true_params.rv[0] + r.x as f64,
                true_params.rv[1] + r.y as f64,
                true_params.rv[2] + r.z as f64,
                true_params.rv[3] + v.x as f64,
                true_params.rv[4] + v.y as f64,
                true_params.rv[5] + v.z as f64,
            )
        };

        let propagator = KeplerianPropagator::from_eci(world_time.epoch, rv_world, 1.0);

        if let Ok(elements) = propagator.state_koe_osc(world_time.epoch, AngleFormat::Radians) {
            if elements.y >= 1.0 {
                continue;
            }

            let map_scale = MAP_UNITS_TO_M as f64;
            let semi_major = (elements.x / map_scale) as f32;
            let semi_minor =
                (elements.a * (1.0 - elements.y * elements.y).sqrt() / map_scale) as f32;

            let rotation = Quat::from_axis_angle(Vec3::Z, elements.w as f32)
                * Quat::from_axis_angle(Vec3::X, elements.z as f32)
                * Quat::from_axis_angle(Vec3::Z, elements.a as f32);

            let center_offset = rotation * Vec3::new(-semi_major * elements.y as f32, 0.0, 0.0);

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
