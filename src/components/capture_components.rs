use bevy::math::DVec3;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::resources::space_catalog::OrbitSpec;

// Component to add to an entity to attempt a capture.
// There should only ever be 0 or 1 entity with this component at a time.
#[derive(Component, Debug, Clone)]
pub struct CaptureComponent {
    pub plan_id: String,
    pub current_phase: String,
    pub phase_enter_time_s: f64,
    pub phase_elapsed_time_s: f64,
    /// Set once when the `capture` phase first runs, after the containment sphere radius has
    /// been snapped to the tether's current orbit radius (see `capture_phase_machine_update`).
    pub capture_orbit_initialized: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Phase {
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

/// Direction in the RSO body frame the tether wraps about during capture.
/// Static for now; a future algorithm will select this dynamically.
#[derive(Component, Debug, Clone)]
pub struct CaptureAxis {
    /// Unit direction in the RSO body frame.
    pub axis: DVec3,
    /// Radius (metres) of the contact ring on the RSO where the tether first coils.
    pub contact_radius: f64,
}

impl Default for CaptureAxis {
    fn default() -> Self {
        Self {
            axis: DVec3::Z,
            contact_radius: 2.0,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct CapturePlanDevice {
    #[serde(rename = "type", default)]
    pub device_type: String,
    /// Physical length of the tether in metres.
    #[serde(default)]
    pub tether_length: f64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CapturePlan {
    /// Display name shown to the user. Not used as a lookup key.
    pub name: String,
    /// File-stem identifier used as the HashMap key in [`CapturePlanLibrary`].
    /// Not serialized to JSON — populated by the loader and [`CapturePlanLibrary::insert_plan`].
    #[serde(skip)]
    pub id: String,
    pub phases: Vec<Phase>,
    pub tether: String,
    #[serde(default)]
    pub device: Option<CapturePlanDevice>,
    /// Optional pre-specified RSO orbit. When present, opening the plan pre-populates the
    /// RSO selection so it is placed and drawn without manual catalog/custom selection.
    #[serde(default)]
    pub rso: Option<OrbitSpec>,
    /// Optional pre-specified chaser orbit. See [`CapturePlan::rso`].
    #[serde(default)]
    pub chaser: Option<OrbitSpec>,
}
