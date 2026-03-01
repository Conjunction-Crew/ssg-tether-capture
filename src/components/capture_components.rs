use astrora_core::core::Duration;
use bevy::{prelude::*};
use serde::{Deserialize, Serialize};

// Component to add to an entity to attempt a capture.
// There should only ever be 0 or 1 entity with this component at a time.
#[derive(Component, Debug, Clone)]
pub struct CaptureComponent {
    pub plan_id: String,
    pub current_state: String,
    pub state_enter_time_s: f64,
    pub state_elapsed_time_s: f64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct State {
    pub id: String,
    pub action: String,
    pub next: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CapturePlan {
    pub name: String,
    pub states: Vec<State>,
    pub tether: String,
}
