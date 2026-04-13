use bevy::prelude::{Component, Resource, Vec3};

// Component to query the camera target
#[derive(Component, Debug, Clone)]
pub struct CameraTarget;

/// Marker component on the on-screen orbit-controls widget buttons.
/// Used by `orbit_camera_ui_controls` to drive camera rotation, zoom, and reset.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub enum OrbitControlButton {
    OrbitLeft,
    OrbitRight,
    OrbitUp,
    OrbitDown,
    ZoomIn,
    ZoomOut,
    ResetView,
}

/// Marker component on the orbit controls widget bounding-box container.
/// Enables left-click-drag-to-orbit within the widget area.
#[derive(Component, Debug, Clone)]
pub struct OrbitControlsDragRegion;

/// Tracks whether a left-click drag was initiated inside the orbit controls widget.
#[derive(Resource, Default)]
pub struct OrbitDragState {
    /// True while a drag that started inside the bounding box is in progress.
    pub active: bool,
}

/// Tracks which orbit control button is currently held and for how long.
#[derive(Resource, Default)]
pub struct OrbitHoldState {
    /// The button variant currently held.
    pub held: Option<OrbitControlButton>,
    /// Accumulated seconds the current button has been continuously held.
    pub hold_secs: f32,
}

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
