use crate::{
    components::{
        orbit::{Orbital, TrueParams},
        user_interface::{OrbitLabel, TrackObject},
    },
    constants::{EARTH_RADIUS, MAP_UNITS_TO_M, SCENE_LAYER},
    resources::world_time::WorldTime,
};

use avian3d::prelude::{RigidBodyDisabled, RigidBodyQueryReadOnly};
use bevy::{camera::visibility::RenderLayers, math::DVec3, prelude::*};

pub fn track_objects(
    bodies: Query<(RigidBodyQueryReadOnly, &TrueParams, &Orbital)>,
    mut trackers: Query<(&mut Text, &TrackObject)>,
    time_warp: Res<WorldTime>,
) {
    for (mut text, tracker) in &mut trackers {
        if let Some(entity) = &tracker.entity {
            if let Ok((rb, true_params, orbital)) = bodies.get(*entity) {
                if let Some(elements) = orbital.elements {
                    text.0 = format!(
                        concat!(
                            "Velocity: {:.2}m/s\n",
                            "Semi-major axis (m): {:.2}\n",
                            "Eccentricity: {:.2}\n",
                            "Inclination (radians): {:.2}\n",
                            "RAAN (radians): {:.2}\n",
                            "Argument of periapsis (radians): {:.2}\n",
                            "True anomaly (radians): {:.2}\n",
                            "Time warp: {}x\n",
                            "Height: {:.1}m\n",
                        ),
                        DVec3::new(true_params.rv[0], true_params.rv[1], true_params.rv[2]).length()
                            + rb.linear_velocity.length() as f64,
                        elements.x,
                        elements.y,
                        elements.z,
                        elements.w,
                        elements.a,
                        elements.b,
                        time_warp.multiplier,
                        DVec3::new(true_params.rv[3], true_params.rv[4], true_params.rv[5]).length()
                            + rb.position.length() as f64
                            - EARTH_RADIUS as f64
                    );
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
