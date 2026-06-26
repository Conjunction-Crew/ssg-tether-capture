use bevy::prelude::{Component, Vec3};

// Marks the floating-origin / physics-bubble anchor body (the chaser). The world is kept
// centered on this entity; it must stay an enabled, non-disabled rigid body.
#[derive(Component, Debug, Clone)]
pub struct CameraTarget;

// Marks the body the camera looks at. Decoupled from `CameraTarget` so the camera can focus
// on the RSO while the floating-origin anchor stays on the chaser. Exactly one entity should
// have this at a time; it is what the "Cycle Target" control / Tab cycles.
#[derive(Component, Debug, Clone)]
pub struct CameraFocus;

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
            #[cfg(windows)] // Decreased default sensitivity on windows due to quirks.
            sensitivity: 0.002,
            #[cfg(not(windows))]
            sensitivity: 0.005,
            max_pitch: 1.55,
            up: Vec3::Y,
        }
    }
}
