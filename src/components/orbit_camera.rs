use bevy::{
    ecs::entity::Entity,
    prelude::{Component, Vec3},
};

// A camera that "orbits" around a target. Hold right click to pan.
#[derive(Component, Debug, Clone)]
pub struct OrbitCamera {
    pub focus: Vec3,
    pub distance: f32,
    pub yaw: f32,
    pub pitch: f32,
    pub min_distance: f32,
    pub max_distance: f32,
    pub sensitivity: f32,
    pub max_pitch: f32,
    pub target: Option<Entity>,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            focus: Vec3::ZERO,
            distance: 30.0,
            yaw: 0.0,
            pitch: 0.0,
            min_distance: 0.5,
            max_distance: 100000.0,
            sensitivity: 0.005,
            max_pitch: 1.55,
            target: None,
        }
    }
}
