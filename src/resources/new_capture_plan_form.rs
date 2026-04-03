use bevy::prelude::*;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum UnitSystem {
    #[default]
    Metric,
    Imperial,
}

#[derive(Debug, Default, Clone)]
pub struct TransitionForm {
    pub to: String,
    /// "less_than" or "greater_than"
    pub distance_kind: String,
    pub distance_value: String,
    /// Optional units string, e.g. "m"
    pub units: String,
}

#[derive(Resource, Debug, Default, Clone)]
pub struct NewCapturePlanForm {
    pub open: bool,

    // General
    pub plan_name: String,

    // Tether
    pub tether_name: String,
    pub tether_type: String,
    pub num_joints: String,

    // Approach state
    pub approach_max_velocity: String,
    pub approach_max_force: String,
    pub approach_transitions: Vec<TransitionForm>,

    // Terminal state
    pub terminal_max_velocity: String,
    pub terminal_max_force: String,
    pub terminal_shrink_rate: String,
    pub terminal_transitions: Vec<TransitionForm>,

    // Capture state
    pub capture_max_velocity: String,
    pub capture_max_force: String,
    pub capture_shrink_rate: String,

    // UI state
    /// Non-empty when the user tried to save and a conflict was found.
    /// Contains the full path of the conflicting file.
    pub overwrite_conflict_path: Option<String>,
    pub validation_errors: Vec<String>,
    pub unit_system: UnitSystem,
    /// Some(plan_id) when editing an existing plan; None when creating new.
    pub editing_plan_id: Option<String>,
}

impl NewCapturePlanForm {
    pub fn reset(&mut self) {
        *self = NewCapturePlanForm::default();
    }
}
