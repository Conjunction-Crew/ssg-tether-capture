use bevy::prelude::{Quat, Vec3};

// Camera layers
pub const SCENE_LAYER: usize = 0;
pub const MAP_LAYER: usize = 1;
pub const UI_LAYER: usize = 2;

// Earth constants
pub const EARTH_RADIUS: f32 = 6_360_000.0;
pub const EARTH_ATMOSPHERE_RADIUS: f32 = 6_460_000.0;

// Map constants
pub const MAP_UNITS_TO_M: f64 = 100000.0;
pub const EARTH_TEXTURE_NORTH_AXIS: Vec3 = Vec3::Z;

pub fn orbit_frame_rotation() -> Quat {
    Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)
}

pub fn eci_to_orbit_frame(position_eci: Vec3) -> Vec3 {
    orbit_frame_rotation() * position_eci
}

pub fn eci_position_to_map(position_eci: Vec3) -> Vec3 {
    let scale = MAP_UNITS_TO_M as f32;
    eci_to_orbit_frame(position_eci) / scale
}
