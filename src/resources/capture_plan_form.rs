use bevy::prelude::*;
use serde_json::Value;

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
}

#[derive(Resource, Debug, Default, Clone)]
pub struct NewCapturePlanForm {
    pub open: bool,

    // General
    pub plan_name: String,
    /// "capture" or "propagation".
    pub sim_type: String,

    // Tether
    pub tether_name: String,
    pub tether_type: String,
    /// Physical length of the tether in metres (stored as string for the input field).
    pub tether_length: String,

    // Propagation sim
    /// "cw_radial" or "cw_along_track".
    pub prop_orientation: String,
    /// "joints_tension" or "separate_bodies".
    pub prop_node_mode: String,
    pub prop_max_tension_n: String,
    pub prop_speedup: String,
    /// Preserved orbital defaults (not editable in the form) so a save round-trips
    /// the plan's quick-start orbits. Populated when opening an existing plan.
    pub default_target_json: Option<Value>,
    pub default_chaser_json: Option<Value>,

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
    /// Set to true after saving an edited plan in the sim screen.
    /// A poll system reads this, spawns the restart prompt, and clears it.
    pub show_restart_prompt: bool,
    /// When true, the form is in view-only mode (e.g. viewing an example plan).
    pub read_only: bool,
    /// Snapshot of the serialized plan taken when the form is opened for editing.
    /// Used to detect whether any fields were actually changed before saving.
    pub original_json: Option<Value>,
}

impl NewCapturePlanForm {
    pub fn reset(&mut self) {
        *self = NewCapturePlanForm {
            sim_type: "capture".to_string(),
            tether_type: "tether".to_string(),
            tether_name: "Tether1".to_string(),
            tether_length: "20.0".to_string(),
            prop_orientation: "cw_radial".to_string(),
            prop_node_mode: "joints_tension".to_string(),
            prop_speedup: "1".to_string(),
            ..Default::default()
        };
    }
}

#[derive(Resource, Debug, Clone)]
pub struct SimPlanSyncState {
    pub in_sync: bool,
    pub restart_requested: bool,
    /// When a restart is requested, remembers whether the camera was in
    /// detail view so `setup_camera` can restore it instead of defaulting
    /// to map view.
    pub restart_to_detail_view: bool,
}

impl Default for SimPlanSyncState {
    fn default() -> Self {
        Self {
            in_sync: true,
            restart_requested: false,
            restart_to_detail_view: false,
        }
    }
}
