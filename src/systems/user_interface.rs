use crate::{
    components::orbit::{Orbital, TrueParams},
    constants::{EARTH_RADIUS, MAP_UNITS_TO_M, SCENE_LAYER},
    resources::{orbital_entities::OrbitalEntities, world_time::WorldTime},
    ui::screens::project_detail::TelemetryValue,
};

use crate::components::user_interface::OrbitLabel;

use avian3d::prelude::{RigidBodyDisabled, RigidBodyQueryReadOnly};
use bevy::{camera::visibility::RenderLayers, math::DVec3, prelude::*};
use brahe::utils::DOrbitStateProvider;

pub fn update_telemetry_values(
    bodies: Query<(RigidBodyQueryReadOnly, &TrueParams, &Orbital)>,
    mut values: Query<(&mut Text, &TelemetryValue)>,
    orbital_elements: Res<OrbitalEntities>,
    world_time: Res<WorldTime>,
) {
    for (mut text, telemetry) in &mut values {
        if let Some(entity) = &telemetry.entity {
            if let Ok((rb, true_params, orbital)) = bodies.get(*entity) {
                let propagator = orbital_elements.propagators[orbital.propagator_id].clone();
                if let Ok(elements) = propagator.state_eci(world_time.epoch) {
                    let velocity = DVec3::new(
                        true_params.rv[0],
                        true_params.rv[1],
                        true_params.rv[2],
                    )
                    .length()
                        + rb.linear_velocity.length() as f64;
                    let height = DVec3::new(
                        true_params.rv[3],
                        true_params.rv[4],
                        true_params.rv[5],
                    )
                    .length()
                        + rb.position.length() as f64
                        - EARTH_RADIUS as f64;

                    text.0 = match telemetry.field_index {
                        0 => format!("{:.2} m/s", velocity),
                        1 => format!("{:.2} m", elements.x),
                        2 => format!("{:.2}", elements.y),
                        3 => format!("{:.2} rad", elements.z),
                        4 => format!("{:.2} rad", elements.w),
                        5 => format!("{:.2} rad", elements.a),
                        6 => format!("{:.2} rad", elements.b),
                        7 => format!("{:.1} m", height),
                        _ => "--".to_string(),
                    };
                }
            }
        }
    }
}

pub fn map_orbitals(
    camera: Single<(&Camera, &GlobalTransform, &RenderLayers), With<Camera3d>>,
    mut labels: Query<(&mut Node, &OrbitLabel)>,
    true_params_query: Query<&TrueParams>,
    rigidbodies: Query<RigidBodyQueryReadOnly>,
    disabled_bodies: Query<(), With<RigidBodyDisabled>>,
) {
    let (cam, cam_transform, render_layers) = camera.into_inner();

    for (mut node, label) in &mut labels {
        if render_layers.intersects(&RenderLayers::layer(SCENE_LAYER)) {
            node.display = Display::None;
        } else {
            node.display = Display::Block;
        }
        if let Some(entity) = label.entity {
            if let Ok(true_params) = true_params_query.get(entity) {
                let mut world_position = Vec3::new(
                    true_params.rv[0] as f32 / MAP_UNITS_TO_M,
                    true_params.rv[1] as f32 / MAP_UNITS_TO_M,
                    true_params.rv[2] as f32 / MAP_UNITS_TO_M,
                );

                if let Ok(rb) = rigidbodies.get(entity)
                    && !disabled_bodies.contains(entity)
                {
                    world_position += rb.position.0 / MAP_UNITS_TO_M;
                }

                if let Ok(viewport_position) = cam.world_to_viewport(cam_transform, world_position)
                {
                    node.top = Val::Px(viewport_position.y);
                    node.left = Val::Px(viewport_position.x);
                }
            }
        }
    }
}
