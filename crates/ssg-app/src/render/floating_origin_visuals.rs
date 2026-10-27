//! Moves the Earth and the atmosphere so the camera target stays in place.

use avian3d::prelude::RigidBodyDisabled;
use bevy::camera::visibility::RenderLayers;
use bevy::pbr::Atmosphere;
use bevy::prelude::*;

use crate::render::celestial::Earth;
use crate::render::layers::{MAP_LAYER, eci_to_orbit_frame};
use ssg_sim::anchor::CameraTarget;
use ssg_sim::orbit::Orbital;
use ssg_sim::orbit::cache::OrbitalCache;

/// The camera target's transform, while its physics is enabled.
type TargetTransform<'w, 's> = Single<
    'w,
    's,
    (Entity, &'static Transform),
    (
        Without<RigidBodyDisabled>,
        With<CameraTarget>,
        Without<Earth>,
    ),
>;

/// The scene camera's atmosphere and render layers.
type CameraAtmosphere<'w, 's> = Single<
    'w,
    's,
    (&'static mut Atmosphere, &'static RenderLayers),
    (With<Camera3d>, Without<Orbital>),
>;

pub fn floating_origin_update_visuals(
    target_params_s: TargetTransform,
    camera_s: CameraAtmosphere,
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
