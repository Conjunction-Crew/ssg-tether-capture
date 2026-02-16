use crate::components::orbit::Orbital;
use crate::components::orbit_camera::OrbitCamera;

use astrora_core::core::constants::GM_EARTH;
use astrora_core::core::elements::coe_to_rv;
use astrora_core::propagators::keplerian::propagate_keplerian;
use bevy::pbr::Atmosphere;
use bevy::prelude::*;

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
    s: Single<(&mut OrbitCamera, &mut Transform, &mut Atmosphere), (With<Camera3d>, Without<Orbital>)>,
) {
    let (cam, mut transform, mut atmosphere) = s.into_inner();

    // We want to position orbital objects relative to the camera's current target
    if let Some(target_entity) = cam.target {
        if let Ok((target_orbital, target_transform)) = orbitals.get(target_entity) {
            if let (Some(elements), Some(parent_entity)) =
                (&target_orbital.elements, &target_orbital.parent_entity)
            {
                let (new_position, _new_velocity) = coe_to_rv(&elements, GM_EARTH);

                // Parent orbital becomes new position
                if let Ok((parent_orbital, mut parent_transform)) = orbitals.get_mut(*parent_entity) {
                    let new_translation = Vec3::new(
                        (new_position.x) as f32,
                        (new_position.y) as f32,
                        (new_position.z) as f32,
                    );
                    parent_transform.translation = new_translation;
                    atmosphere.world_position = new_translation;
                }
            }
        }
    }
}
