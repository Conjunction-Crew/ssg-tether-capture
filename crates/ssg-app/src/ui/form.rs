use bevy::prelude::*;
use serde_json::Value;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UnitSystem {
    #[default]
    Metric,
    Imperial,
}

const M_PER_FT: f64 = 0.3048;
const N_PER_LBF: f64 = 4.44822;
const KM_PER_MI: f64 = 1.609344;

/// What a displayed quantity is in this unit system (Q10 row 16). Display only:
/// files, exports, logs and recordings stay metric.
impl UnitSystem {
    pub fn length(self, m: f64) -> (f64, &'static str) {
        match self {
            UnitSystem::Metric => (m, "m"),
            UnitSystem::Imperial => (m / M_PER_FT, "ft"),
        }
    }

    pub fn speed(self, m_s: f64) -> (f64, &'static str) {
        match self {
            UnitSystem::Metric => (m_s, "m/s"),
            UnitSystem::Imperial => (m_s / M_PER_FT, "ft/s"),
        }
    }

    pub fn force(self, n: f64) -> (f64, &'static str) {
        match self {
            UnitSystem::Metric => (n, "N"),
            UnitSystem::Imperial => (n / N_PER_LBF, "lbf"),
        }
    }

    /// A length displayed in kilometres in metric.
    pub fn long_length(self, km: f64) -> (f64, &'static str) {
        match self {
            UnitSystem::Metric => (km, "km"),
            UnitSystem::Imperial => (km / KM_PER_MI, "mi"),
        }
    }

    /// A length or speed the form shows in this system, in metres (per second).
    /// Rounded to 1e-9, so a value [`NewCapturePlanForm::set_unit_system`] converted
    /// comes back as the number the plan file holds.
    pub fn linear_to_metric(self, shown: f64) -> f64 {
        match self {
            UnitSystem::Metric => shown,
            UnitSystem::Imperial => round_to(shown * M_PER_FT, 1e9),
        }
    }

    /// A force the form shows in this system, in newtons (rounded as above).
    pub fn force_to_metric(self, shown: f64) -> f64 {
        match self {
            UnitSystem::Metric => shown,
            UnitSystem::Imperial => round_to(shown * N_PER_LBF, 1e9),
        }
    }
}

fn round_to(value: f64, scale: f64) -> f64 {
    (value * scale).round() / scale
}

/// `12.34 m`: a converted quantity at `decimals` places.
pub fn show((value, unit): (f64, &str), decimals: usize) -> String {
    format!("{value:.decimals$} {unit}")
}

/// Replaces a number in `text` with `convert(number)`; anything else stays as
/// typed (Save validates it). Ten decimals: back in metric, the error is under the
/// 1e-9 rounding of [`UnitSystem::linear_to_metric`], for forces too.
fn convert_text(text: &mut String, convert: impl Fn(f64) -> f64) {
    if let Some(value) = text.parse::<f64>().ok().filter(|value| value.is_finite()) {
        *text = serde_json::Value::from(round_to(convert(value), 1e10)).to_string();
    }
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

    // Tether
    pub tether_name: String,
    pub tether_type: String,
    /// Physical length of the tether in metres (stored as string for the input field).
    pub tether_length: String,

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
    /// A blank form, in the units the toggle is on.
    pub fn reset(&mut self) {
        let unit = self.unit_system;
        *self = NewCapturePlanForm {
            tether_type: "tether".to_string(),
            tether_name: "Tether1".to_string(),
            tether_length: "20.0".to_string(),
            ..Default::default()
        };
        self.set_unit_system(unit);
    }

    /// Shows the form in `unit`: every number keeps its physical value, so what
    /// Save writes does not depend on the toggle (OQ3).
    pub fn set_unit_system(&mut self, unit: UnitSystem) {
        let from = self.unit_system;
        if unit == from {
            return;
        }
        let linear = |text: &mut String| {
            convert_text(text, |shown| unit.length(from.linear_to_metric(shown)).0);
        };
        let force = |text: &mut String| {
            convert_text(text, |shown| unit.force(from.force_to_metric(shown)).0);
        };
        for text in [
            &mut self.tether_length,
            &mut self.approach_max_velocity,
            &mut self.terminal_max_velocity,
            &mut self.terminal_shrink_rate,
            &mut self.capture_max_velocity,
            &mut self.capture_shrink_rate,
        ] {
            linear(text);
        }
        for text in [
            &mut self.approach_max_force,
            &mut self.terminal_max_force,
            &mut self.capture_max_force,
        ] {
            force(text);
        }
        for transition in self
            .approach_transitions
            .iter_mut()
            .chain(&mut self.terminal_transitions)
        {
            linear(&mut transition.distance_value);
        }
        self.unit_system = unit;
    }
}

#[derive(Resource, Debug, Clone)]
pub struct SimPlanSyncState {
    pub in_sync: bool,
    pub restart_requested: bool,
}

impl Default for SimPlanSyncState {
    fn default() -> Self {
        Self {
            in_sync: true,
            restart_requested: false,
        }
    }
}
