//! The map label of a body (`map_orbitals`) sits at the body's ECI position (#123,
//! F-49): a disabled body at its cached state, an enabled one at the cached state
//! plus its bubble position.

use bevy::math::DVec3;
use bevy::prelude::*;
use nalgebra::Vector6;

use crate::render::layers::eci_position_to_map;
use crate::ui::overlays::orbit_label_map_position;
use ssg_sim::orbit::cache::OrbitalCache;

const CACHED: [f64; 3] = [6_771_000.0, 1_000.0, -2_000.0];

fn cache_with(entity: Entity) -> OrbitalCache {
    let mut cache = OrbitalCache::default();
    cache.eci_states.insert(
        entity,
        Vector6::new(CACHED[0], CACHED[1], CACHED[2], 0.0, 7_660.0, 0.0),
    );
    cache
}

fn map(x: f64, y: f64, z: f64) -> Vec3 {
    eci_position_to_map(Vec3::new(x as f32, y as f32, z as f32))
}

#[test]
fn a_disabled_bodys_label_sits_at_its_cached_state() {
    let entity = World::new().spawn_empty().id();
    let cache = cache_with(entity);
    // A disabled body's bubble position is stale: 50 km off.
    let at = orbit_label_map_position(
        &cache,
        entity,
        DVec3::new(50_000.0, 0.0, 0.0),
        DVec3::ZERO,
        true,
    );
    assert_eq!(at, Some(map(CACHED[0], CACHED[1], CACHED[2])));
}

#[test]
fn an_enabled_bodys_label_sits_at_its_cached_state_plus_its_bubble_position() {
    let entity = World::new().spawn_empty().id();
    let cache = cache_with(entity);
    let at = orbit_label_map_position(
        &cache,
        entity,
        DVec3::new(30.0, -4.0, 2.0),
        DVec3::ZERO,
        false,
    );
    assert_eq!(
        at,
        Some(map(CACHED[0] + 30.0, CACHED[1] - 4.0, CACHED[2] + 2.0))
    );
}

#[test]
fn a_body_without_a_cached_state_has_no_label_position() {
    let entity = World::new().spawn_empty().id();
    let at = orbit_label_map_position(
        &OrbitalCache::default(),
        entity,
        DVec3::ZERO,
        DVec3::ZERO,
        false,
    );
    assert_eq!(at, None);
}
