use bevy::{
    prelude::{Component, Vec3},
};

// Component to query the camera target
#[derive(Component, Debug, Clone)]
pub struct CameraTarget;

// A camera that "orbits" around a target. Hold right click to pan.
#[derive(Component, Debug, Clone)]
pub struct OrbitCamera {
    pub scene_params: OrbitCameraParams,
    pub map_params: OrbitCameraParams,
}

#[derive(Debug, Clone)]
pub struct OrbitCameraParams {
    pub focus: Vec3,
    pub distance: f32,
    pub yaw: f32,
    pub pitch: f32,
    pub min_distance: f32,
    pub max_distance: f32,
    pub sensitivity: f32,
    pub max_pitch: f32,
    pub up: Vec3,
}

impl Default for OrbitCameraParams {
    fn default() -> Self {
        Self {
            focus: Vec3::ZERO,
            distance: 30.0,
            yaw: 0.0,
            pitch: 0.0,
            min_distance: 0.5,
            max_distance: 10000.0,
            sensitivity: 0.005,
            max_pitch: 1.55,
            up: Vec3::Y,
        }
    }
}
