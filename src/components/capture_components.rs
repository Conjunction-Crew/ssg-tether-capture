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

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CapturePlan {
    pub name: String,
    pub states: Vec<State>,
    pub tether: String,
}
