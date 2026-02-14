use crate::components::orbit::Orbital;
use crate::systems::setup::EARTH_Y_OFFSET;
use crate::systems::setup::CELESTIAL_UNITS_TO_M;

use astrora_core::core::constants::GM_EARTH;
use astrora_core::core::elements::coe_to_rv;
use astrora_core::propagators::keplerian::propagate_keplerian;
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

                let (new_position, _new_velocity) = coe_to_rv(&new_elements, GM_EARTH);
                transform.translation = Vec3::new(
                    (new_position.x / CELESTIAL_UNITS_TO_M) as f32,
                    (new_position.y / CELESTIAL_UNITS_TO_M) as f32 - EARTH_Y_OFFSET,
                    (new_position.z / CELESTIAL_UNITS_TO_M) as f32,
                );
            }
        }
    }
}
