use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;

// Component to add to an entity to attempt a capture.
// There should only ever be 0 or 1 entity with this component at a time.
#[derive(Component, Debug, Clone)]
pub struct CaptureComponent {
    pub plan_id: String,
    pub current_state: String,
    pub state_enter_time_s: f64,
    pub state_elapsed_time_s: f64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct State {
    pub id: String,
    #[serde(default)]
    pub next: Option<String>,
    #[serde(default)]
    pub parameters: Option<Value>,
    #[serde(default)]
    pub transitions: Option<Vec<Value>>,
    #[serde(default)]
    pub next_conditions: Option<Value>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct CapturePlanDevice {
    #[serde(rename = "type", default)]
    pub device_type: String,
    /// Physical length of the tether in metres.
    #[serde(default)]
    pub tether_length: f64,
}

/// Keplerian orbital elements embedded in a capture plan as optional defaults.
/// All angles are in radians; semi-major axis is in metres.
#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct PlanDefaultOrbitalElements {
    pub semi_major_axis_m: f64,
    pub eccentricity: f64,
    pub inclination_rad: f64,
    pub raan_rad: f64,
    pub arg_perigee_rad: f64,
    pub mean_anomaly_rad: f64,
    #[serde(default)]
    pub epoch_offset_seconds: f64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CapturePlan {
    /// Display name shown to the user. Not used as a lookup key.
    pub name: String,
    /// File-stem identifier used as the HashMap key in [`CapturePlanLibrary`].
    /// Not serialized to JSON — populated by the loader and [`CapturePlanLibrary::insert_plan`].
    #[serde(skip)]
    pub id: String,
    pub states: Vec<State>,
    pub tether: String,
    #[serde(default)]
    pub device: Option<CapturePlanDevice>,
    /// Pre-populated target (debris) orbital elements for quick-start.
    #[serde(default)]
    pub default_target: Option<PlanDefaultOrbitalElements>,
    /// Pre-populated chaser (tether spacecraft) orbital elements for quick-start.
    #[serde(default)]
    pub default_chaser: Option<PlanDefaultOrbitalElements>,
}
