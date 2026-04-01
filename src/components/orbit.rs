use bevy::{
    ecs::entity::Entity,
    math::DVec3,
    prelude::{Component, Quat, Vec3},
};
use nalgebra::Vector6;

// Component to query Earth.
#[derive(Component)]
pub struct Earth;

// Component for identifying a root node of a tether system.
#[derive(Component, Debug, Clone)]
pub struct TetherNode {
    pub root: Entity,
}

// True position and velocity of an orbital object.
// This is the ground truth from which all other positional data is derived from.
// Format: rx, ry, rz, vx, vy, vz
#[derive(Component, Default, Debug, Clone)]
pub struct TrueParams {
    pub rv: Vector6<f64>,
}

// Orbital parameters and state for a body approaching another object.
#[derive(Component, Debug, Clone)]
#[require(TrueParams)]
pub struct Orbital {
    pub object_id: String,
    pub parent_entity: Option<Entity>,
    pub tle: Option<TleData>,
    pub propagator_id: usize,
    pub attitude: AttitudeState,
    pub approach: ApproachMetrics,
}

// Init methods for orbital objects
#[derive(Component, Debug, Clone)]
#[require(Orbital, TrueParams)]
pub enum Orbit {
    FromTle(String),
    FromElements(Vector6<f64>),
    FromParams(TrueParams),
}

#[derive(Debug, Clone)]
pub struct TleData {
    pub line1: String,
    pub line2: String,
    pub epoch_utc: Option<String>,
    pub bstar: Option<f32>,
    pub mean_motion: Option<f32>,
}

#[derive(Debug, Clone)]
pub struct AttitudeState {
    pub orientation: Quat,
    pub angular_velocity_rad_s: Vec3,
    pub body_axes: BodyAxes,
}

#[derive(Debug, Clone)]
pub struct BodyAxes {
    pub x_body: Vec3,
    pub y_body: Vec3,
    pub z_body: Vec3,
}

#[derive(Debug, Clone)]
pub struct ApproachMetrics {
    pub relative_position_km: DVec3,
    pub relative_velocity_km_s: DVec3,
    pub range_km: f32,
    pub closing_speed_km_s: f32,
    pub time_to_closest_approach_s: Option<f32>,
    pub closest_approach_distance_km: Option<f32>,
}

impl Default for AttitudeState {
    fn default() -> Self {
        Self {
            orientation: Quat::IDENTITY,
            angular_velocity_rad_s: Vec3::ZERO,
            body_axes: BodyAxes {
                x_body: Vec3::X,
                y_body: Vec3::Y,
                z_body: Vec3::Z,
            },
        }
    }
}

impl Default for ApproachMetrics {
    fn default() -> Self {
        Self {
            relative_position_km: DVec3::ZERO,
            relative_velocity_km_s: DVec3::ZERO,
            range_km: 0.0,
            closing_speed_km_s: 0.0,
            time_to_closest_approach_s: None,
            closest_approach_distance_km: None,
        }
    }
}

impl Default for Orbital {
    fn default() -> Self {
        Self {
            object_id: String::new(),
            parent_entity: None,
            tle: None,
            propagator_id: 0,
            attitude: AttitudeState::default(),
            approach: ApproachMetrics::default(),
        }
    }
}
