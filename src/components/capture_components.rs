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

/// Which kind of simulation a plan describes.
///
/// `Capture` is the original behaviour (run the capture state machine).
/// `Propagation` studies how a tether's initial orientation affects its
/// orbit/attitude over many orbits, using the [`PropagationConfig`] block.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SimType {
    #[default]
    Capture,
    Propagation,
}

/// Initial orientation of the tether in the Clohessy–Wiltshire / Hill frame.
/// Set once at spawn; physics then evolves it freely.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PropagationOrientation {
    /// Aligned with CW X = nadir (pointing toward Earth centre).
    #[default]
    CwRadial,
    /// Aligned with CW Y = along-track (orbital velocity direction).
    CwAlongTrack,
}

/// How the tether nodes are simulated in a propagation sim.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PropagationNodeMode {
    /// Keep the `DistanceJoint` chain and drive it with linearized
    /// Clohessy–Wiltshire / Hill rotating-frame accelerations.
    #[default]
    JointsTension,
    /// Each node is an independent two-body Keplerian propagator (no joints,
    /// no forces); the nodes drift apart on their own orbits.
    SeparateBodies,
}

/// Parameters specific to a [`SimType::Propagation`] plan.
#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct PropagationConfig {
    #[serde(default)]
    pub orientation: PropagationOrientation,
    #[serde(default)]
    pub node_mode: PropagationNodeMode,
    /// Max allowable joint tension (N). Log-only — no breaking/clamping.
    #[serde(default)]
    pub max_tension_n: Option<f64>,
    /// Time speedup applied to `WorldTime.multiplier` when the sim starts.
    #[serde(default = "default_speedup")]
    pub speedup: u32,
}

fn default_speedup() -> u32 {
    1
}

impl Default for PropagationConfig {
    fn default() -> Self {
        Self {
            orientation: PropagationOrientation::default(),
            node_mode: PropagationNodeMode::default(),
            max_tension_n: None,
            speedup: default_speedup(),
        }
    }
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
    /// Which kind of simulation this plan describes. Defaults to `Capture` so
    /// existing plans (which omit the field) deserialize unchanged.
    #[serde(default)]
    pub sim_type: SimType,
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
    /// Configuration for a [`SimType::Propagation`] plan. `None` for capture plans.
    #[serde(default)]
    pub propagation: Option<PropagationConfig>,
}
