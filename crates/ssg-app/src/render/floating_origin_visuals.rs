//! Moves the Earth and the atmosphere so the camera target stays in place.

use avian3d::prelude::RigidBodyDisabled;
use bevy::camera::visibility::RenderLayers;
use bevy::pbr::Atmosphere;
use bevy::prelude::*;

use crate::render::celestial::Earth;
use crate::render::layers::{MAP_LAYER, eci_to_orbit_frame};
use crate::sim::anchor::CameraTarget;
use crate::sim::orbit::Orbital;
use crate::sim::orbit::cache::OrbitalCache;

pub fn floating_origin_update_visuals(
    target_params_s: Single<
        (Entity, &Transform),
        (
            Without<RigidBodyDisabled>,
            With<CameraTarget>,
            Without<Earth>,
        ),
    >,
    camera_s: Single<(&mut Atmosphere, &RenderLayers), (With<Camera3d>, Without<Orbital>)>,
    earth: Single<&mut Transform, (With<Earth>, Without<CameraTarget>)>,
    orbital_cache: ResMut<OrbitalCache>,
) {
    let (mut atmosphere, render_layers) = camera_s.into_inner();

    // Do not calculate floating origin if we are in map view
    if render_layers.intersects(&RenderLayers::layer(MAP_LAYER)) {
        return;
    }

    // We want to position orbital objects relative to the camera's current target
    let (target_entity, target_transform) = target_params_s.into_inner();

    // Get current cartesian state of our target
    let Some(target_rv) = orbital_cache.eci_states.get(&target_entity) else {
        return;
    };

    // Earth translation becomes new position
    let mut earth_transform = earth.into_inner();
    let target_position = eci_to_orbit_frame(Vec3::new(
        target_rv[0] as f32,
        target_rv[1] as f32,
        target_rv[2] as f32,
    ));
    let new_translation = target_transform.translation - target_position;
    earth_transform.translation = new_translation;
    atmosphere.world_position = new_translation;
}
