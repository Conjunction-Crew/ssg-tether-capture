use std::char::MAX;

use crate::components::orbit::{Earth, Orbital, TetherNode};
use crate::components::orbit_camera::OrbitCamera;
use crate::constants::MAP_LAYER;

use astrora_core::core::constants::GM_EARTH;
use astrora_core::core::elements::{coe_to_rv, rv_to_coe};
use astrora_core::core::linalg::Vector3;
use astrora_core::propagators::keplerian::propagate_keplerian;
use avian3d::prelude::{LinearVelocity, Position, RigidBody};
use bevy::camera::visibility::RenderLayers;
use bevy::pbr::Atmosphere;
use bevy::prelude::*;

const MAX_ORIGIN_OFFSET: f32 = 1000.0;
const MAX_LINVEL: f32 = 1000.0;

pub fn ssg_propagate_keplerian(mut orbitals: Query<&mut Orbital>, time: Res<Time>) {
    let dt = time.delta_secs_f64();

    for mut orbital in &mut orbitals {
        if let Some(elements) = &mut orbital.elements {
            if let Ok(new_elements) = propagate_keplerian(&elements, dt, GM_EARTH) {
                *elements = new_elements;
            }
        }
    }
}

pub fn floating_origin(
    orbitals: Query<(&mut Orbital, &mut Transform), With<RigidBody>>,
    s: Single<
        (&mut OrbitCamera, &mut Atmosphere, &RenderLayers),
        (With<Camera3d>, Without<Orbital>),
    >,
    earth: Single<&mut Transform, (With<Earth>, Without<RigidBody>)>,
) {
    let (cam, mut atmosphere, render_layers) = s.into_inner();

    // Do not calculate floating origin if we are in map view
    if render_layers.intersects(&RenderLayers::layer(MAP_LAYER)) {
        return;
    }

    // We want to position orbital objects relative to the camera's current target
    if let Some(target_entity) = cam.scene_params.target {
        let target_orbital_result = orbitals.get(target_entity);

        if target_orbital_result.is_err() {
            return;
        }

        let (target_orbital_ref, target_transform_ref) = target_orbital_result.unwrap();
        let target_orbital = target_orbital_ref.clone();
        let target_transform = target_transform_ref.clone();

        if let Some(target_elements) = &target_orbital.elements {
            // Calculate the position of the target based on orbital elements
            let (new_position, _new_velocity) = coe_to_rv(&target_elements, GM_EARTH);

            // Earth translation becomes new position
            let mut earth_transform = earth.into_inner();
            let new_translation = Vec3::new(
                (new_position.x) as f32,
                (new_position.y) as f32,
                (new_position.z) as f32,
            ) - target_transform.translation;
            earth_transform.translation = new_translation;
            atmosphere.world_position = new_translation;
        }
    }
}

pub fn target_entity_reset_origin(
    mut orbitals: Query<(&mut Orbital, &mut Position, &mut LinearVelocity)>,
    mut nodes: Query<(&mut Position, &TetherNode), Without<Orbital>>,
    s: Single<&mut OrbitCamera, (With<Camera3d>, Without<Orbital>)>,
) {
    let cam = s.into_inner();

    if let Some(target_entity) = cam.scene_params.target {
        if let Ok((mut target_orbital, mut target_position, mut target_linvel)) =
            orbitals.get_mut(target_entity)
        {
            // Check if we are too far from the origin, or if linvel is too high
            if target_position.x > MAX_ORIGIN_OFFSET
                || target_position.x < -MAX_ORIGIN_OFFSET
                || target_position.y > MAX_ORIGIN_OFFSET
                || target_position.y < -MAX_ORIGIN_OFFSET
                || target_position.z > MAX_ORIGIN_OFFSET
                || target_position.z < -MAX_ORIGIN_OFFSET
                || target_linvel.x > MAX_LINVEL
                || target_linvel.x < -MAX_LINVEL
                || target_linvel.y > MAX_LINVEL
                || target_linvel.y < -MAX_LINVEL
                || target_linvel.z > MAX_LINVEL
                || target_linvel.z < -MAX_LINVEL
            {
                // Recalculate orbital elements
                if let Some(elements) = &mut target_orbital.elements {
                    let (current_position, current_velocity) = coe_to_rv(&elements, GM_EARTH);

                    // Add target relative position and velocity to calculated orbital position and velocity
                    let new_position: Vector3 = Vector3::new(
                        current_position.x - target_position.x as f64,
                        current_position.y - target_position.y as f64,
                        current_position.z - target_position.z as f64,
                    );
                    let new_velocity: Vector3 = Vector3::new(
                        current_velocity.x - target_linvel.x as f64,
                        current_velocity.y - target_linvel.y as f64,
                        current_velocity.z - target_linvel.z as f64,
                    );

                    // Reset root node to 0 and offset child node positions by the same amount
                    if let Ok(new_elements) =
                        rv_to_coe(&new_position, &new_velocity, GM_EARTH, 1e-8)
                    {
                        *elements = new_elements;

                        // *target_linvel = LinearVelocity::ZERO;

                        let target_position_offset = target_position.0.clone();
                        target_position.0 = Vec3::ZERO;

                        for (mut node_position, node) in nodes {
                            if node.root == target_entity {
                                node_position.0 -= target_position_offset;
                            }
                        }
                    }
                }
            }
        }
    }
}
