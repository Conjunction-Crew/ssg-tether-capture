use std::char::MAX;

use crate::components::orbit::Orbital;
use crate::components::orbit_camera::OrbitCamera;

use astrora_core::core::constants::GM_EARTH;
use astrora_core::core::elements::{coe_to_rv, rv_to_coe};
use astrora_core::core::linalg::Vector3;
use astrora_core::propagators::keplerian::propagate_keplerian;
use avian3d::prelude::LinearVelocity;
use bevy::pbr::Atmosphere;
use bevy::prelude::*;

const MAX_ORIGIN_OFFSET: f32 = 1000.0;
const MAX_LINVEL: f32 = 1000.0;

pub fn ssg_propagate_keplerian(
    mut orbitals: Query<(&mut Orbital, &mut Transform)>,
    time: Res<Time>,
) {
    let dt = time.delta_secs_f64() * 10.0;

    for (mut orbital, mut transform) in &mut orbitals {
        if let Some(elements) = &mut orbital.elements {
            if let Ok(new_elements) = propagate_keplerian(&elements, dt, GM_EARTH) {
                *elements = new_elements;

                // Don't propagate transform. Let floating origin determine new positions.
                // let (new_position, _new_velocity) = coe_to_rv(&new_elements, GM_EARTH);
                // transform.translation = Vec3::new(
                //     (new_position.x) as f32,
                //     (new_position.y) as f32,
                //     (new_position.z) as f32,
                // );
            }
        }
    }
}

pub fn floating_origin(
    mut orbitals: Query<(&mut Orbital, &mut Transform)>,
    s: Single<(&mut OrbitCamera, &mut Atmosphere), (With<Camera3d>, Without<Orbital>)>,
) {
    let (cam, mut atmosphere) = s.into_inner();

    // We want to position orbital objects relative to the camera's current target
    if let Some(target_entity) = cam.target {
        let target_orbital_result = orbitals.get(target_entity);

        if target_orbital_result.is_err() {
            return;
        }

        let (target_orbital_ref, target_transform_ref) = target_orbital_result.unwrap();
        let target_orbital = target_orbital_ref.clone();
        let target_transform = target_transform_ref.clone();

        if let (Some(target_elements), Some(parent_entity)) =
            (&target_orbital.elements, &target_orbital.parent_entity)
        {
            // Calculate the position of the target based on orbital elements
            let (new_position, _new_velocity) = coe_to_rv(&target_elements, GM_EARTH);

            // Parent translation becomes new position
            if let Ok((_parent_orbital, mut parent_transform)) = orbitals.get_mut(*parent_entity) {
                let new_translation = Vec3::new(
                    (new_position.x) as f32,
                    (new_position.y) as f32,
                    (new_position.z) as f32,
                ) - target_transform.translation;
                parent_transform.translation = new_translation;
                atmosphere.world_position = new_translation;
            }
        }
    }
}

pub fn target_entity_reset_origin(
    mut orbitals: Query<(&mut Orbital, &mut Transform, &mut LinearVelocity)>,
    s: Single<(&mut OrbitCamera), (With<Camera3d>, Without<Orbital>)>,
) {
    let (cam) = s.into_inner();

    // We want to position orbital objects relative to the camera's current target
    if let Some(target_entity) = cam.target {
        if let Ok((mut target_orbital, mut target_transform, mut target_linvel)) =
            orbitals.get_mut(target_entity)
        {
            // Check if we are too far from the origin, or if linvel is too high
            if target_transform.translation.x > MAX_ORIGIN_OFFSET
                || target_transform.translation.x < -MAX_ORIGIN_OFFSET
                || target_transform.translation.y > MAX_ORIGIN_OFFSET
                || target_transform.translation.y < -MAX_ORIGIN_OFFSET
                || target_transform.translation.z > MAX_ORIGIN_OFFSET
                || target_transform.translation.z < -MAX_ORIGIN_OFFSET
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
                        current_position.x + target_transform.translation.x as f64,
                        current_position.y + target_transform.translation.y as f64,
                        current_position.z + target_transform.translation.z as f64,
                    );
                    let new_velocity: Vector3 = Vector3::new(
                        current_velocity.x + target_linvel.x as f64,
                        current_velocity.y + target_linvel.y as f64,
                        current_velocity.z + target_linvel.z as f64,
                    );

                    if let Ok(new_elements) =
                        rv_to_coe(&new_position, &new_velocity, GM_EARTH, 1e-8)
                    {
                        *target_linvel = LinearVelocity::ZERO;
                        target_transform.translation = Vec3::ZERO;
                        *elements = new_elements;
                    }
                }
            }
        }
    }
}
