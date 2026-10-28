use bevy::prelude::{Quat, Vec3};

// Camera layers
pub const SCENE_LAYER: usize = 0;
pub const MAP_LAYER: usize = 1;
pub const UI_LAYER: usize = 2;

// Earth constants. A render radius for the meshes and the atmosphere; the HUD
// altitude uses `ssg_sim::orbit::EARTH_EQUATORIAL_RADIUS_M` (F-28).
pub const EARTH_RADIUS: f32 = 6_360_000.0;
pub const EARTH_ATMOSPHERE_RADIUS: f32 = 6_460_000.0;

// Map constants
pub const MAP_UNITS_TO_M: f64 = 100000.0;
/// The Earth texture's north pole in mesh space: Bevy's `Sphere::mesh().uv()`
/// puts the pole (texture row 0) on mesh +Z.
pub const EARTH_TEXTURE_NORTH_AXIS: Vec3 = Vec3::Z;

/// The scene and the map are drawn in ECI axes (F-08); positions are never
/// rotated. This orients only the two Earth meshes so the texture's north pole
/// lies on ECI +Z. The UV sphere's pole is already on +Z, so it is the identity.
pub fn earth_mesh_rotation() -> Quat {
    Quat::IDENTITY
}

/// Map position of an ECI position: the same axes, scaled to map units.
pub fn eci_position_to_map(position_eci: Vec3) -> Vec3 {
    position_eci / MAP_UNITS_TO_M as f32
}
