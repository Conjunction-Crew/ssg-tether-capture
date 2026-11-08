use bevy::prelude::*;
use ssg_sim::capture::plan::{
    CapturePlan, ConditionBounds, PLAN_SCHEMA_VERSION, TransitionConditions,
};

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
    /// Rounded to 1e-9, so a value the form converted comes back as the number
    /// the plan file holds.
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

/// What a number of the plan form measures, for the unit toggle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quantity {
    Length,
    Speed,
    Force,
    /// Seconds: the same in both systems.
    Seconds,
}

impl Quantity {
    /// The unit the form labels this quantity with in `unit`.
    pub fn unit_label(self, unit: UnitSystem) -> &'static str {
        match self {
            Quantity::Length => unit.length(0.0).1,
            Quantity::Speed => unit.speed(0.0).1,
            Quantity::Force => unit.force(0.0).1,
            Quantity::Seconds => "s",
        }
    }

    fn shown_value(self, unit: UnitSystem, metric: f64) -> f64 {
        match self {
            Quantity::Length => unit.length(metric).0,
            Quantity::Speed => unit.speed(metric).0,
            Quantity::Force => unit.force(metric).0,
            Quantity::Seconds => metric,
        }
    }

    fn to_metric(self, unit: UnitSystem, shown: f64) -> f64 {
        match self {
            Quantity::Length | Quantity::Speed => unit.linear_to_metric(shown),
            Quantity::Force => unit.force_to_metric(shown),
            Quantity::Seconds => shown,
        }
    }

    /// `metric` as the form shows it in `unit`: as the plan file writes it in
    /// metric, at ten decimals otherwise (under the 1e-9 rounding back).
    fn text(self, unit: UnitSystem, metric: f64) -> String {
        let shown = self.shown_value(unit, metric);
        let shown = if shown == metric {
            shown
        } else {
            round_to(shown, 1e10)
        };
        serde_json::Value::from(shown).to_string()
    }
}

/// Why a number field does not give a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumberError {
    NotANumber,
}

/// A number of the plan form: the text the user sees and types, and the metric
/// value the form wrote it from. While the text is unchanged that value is used
/// as it is, so loading, toggling the units and saving loses nothing (F-15).
#[derive(Debug, Default, Clone, PartialEq)]
pub struct NumberField {
    pub text: String,
    /// The metric value and the text the form showed for it (on load or a unit
    /// toggle); `None` once the user has typed something else.
    shown: Option<(f64, String)>,
}

impl NumberField {
    /// `value` (metric) shown in `unit`; empty when `None`.
    pub fn shown(value: Option<f64>, quantity: Quantity, unit: UnitSystem) -> Self {
        match value {
            None => Self::default(),
            Some(metric) => {
                let text = quantity.text(unit, metric);
                Self {
                    shown: Some((metric, text.clone())),
                    text,
                }
            }
        }
    }

    /// The metric value: `None` when the field is empty.
    pub fn metric(&self, quantity: Quantity, unit: UnitSystem) -> Result<Option<f64>, NumberError> {
        if let Some((metric, text)) = &self.shown
            && *text == self.text
        {
            return Ok(Some(*metric));
        }
        let typed = self.text.trim();
        if typed.is_empty() {
            return Ok(None);
        }
        match typed.parse::<f64>() {
            Ok(value) if value.is_finite() => Ok(Some(quantity.to_metric(unit, value))),
            _ => Err(NumberError::NotANumber),
        }
    }

    /// Shows the same physical value in `to`; text that is not a number stays as
    /// typed (Save reports it).
    fn set_unit(&mut self, quantity: Quantity, from: UnitSystem, to: UnitSystem) {
        if let Ok(Some(metric)) = self.metric(quantity, from) {
            *self = Self::shown(Some(metric), quantity, to);
        }
    }
}

/// A transition condition the form shows (Q10 rows 2, 3, 5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConditionKind {
    Distance,
    RelativeSpeed,
    TimeInPhase,
}

impl ConditionKind {
    pub const ALL: [ConditionKind; 3] = [
        ConditionKind::Distance,
        ConditionKind::RelativeSpeed,
        ConditionKind::TimeInPhase,
    ];

    /// The key under `all_of`.
    pub fn key(self) -> &'static str {
        match self {
            ConditionKind::Distance => "distance_m",
            ConditionKind::RelativeSpeed => "relative_speed_mps",
            ConditionKind::TimeInPhase => "time_in_phase_s",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ConditionKind::Distance => "Distance",
            ConditionKind::RelativeSpeed => "Relative Speed",
            ConditionKind::TimeInPhase => "Time in Phase",
        }
    }

    pub fn quantity(self) -> Quantity {
        match self {
            ConditionKind::Distance => Quantity::Length,
            ConditionKind::RelativeSpeed => Quantity::Speed,
            ConditionKind::TimeInPhase => Quantity::Seconds,
        }
    }

    pub fn bounds(self, all_of: &TransitionConditions) -> Option<ConditionBounds> {
        match self {
            ConditionKind::Distance => all_of.distance_m,
            ConditionKind::RelativeSpeed => all_of.relative_speed_mps,
            ConditionKind::TimeInPhase => all_of.time_in_phase_s,
        }
    }

    pub fn bounds_mut(self, all_of: &mut TransitionConditions) -> &mut Option<ConditionBounds> {
        match self {
            ConditionKind::Distance => &mut all_of.distance_m,
            ConditionKind::RelativeSpeed => &mut all_of.relative_speed_mps,
            ConditionKind::TimeInPhase => &mut all_of.time_in_phase_s,
        }
    }
}

/// One condition of a transition: a quantity and its bounds (empty = absent).
#[derive(Debug, Clone, PartialEq)]
pub struct ConditionForm {
    pub kind: ConditionKind,
    pub less_than: NumberField,
    pub greater_than: NumberField,
}

/// One transition: the phase to enter and its conditions, all of which must hold
/// (AND, Q10 row 4).
#[derive(Debug, Clone, PartialEq)]
pub struct TransitionForm {
    pub to: String,
    pub conditions: Vec<ConditionForm>,
    /// The loaded transition's conditions: those the form does not show are
    /// written back unchanged.
    pub base: TransitionConditions,
}

impl Default for TransitionForm {
    /// A new transition: no phase chosen, one distance condition.
    fn default() -> Self {
        Self {
            to: String::new(),
            conditions: vec![ConditionForm {
                kind: ConditionKind::Distance,
                less_than: NumberField::default(),
                greater_than: NumberField::default(),
            }],
            base: TransitionConditions::default(),
        }
    }
}

/// One phase of the plan. Its id is not editable (A-1).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PhaseForm {
    pub id: String,
    pub max_velocity: NumberField,
    pub max_force: NumberField,
    /// Empty = no shrink (the key absent).
    pub shrink_rate: NumberField,
    pub transitions: Vec<TransitionForm>,
}

/// A message of Save's validation and the JSON Pointer of the plan value it is
/// about (`validate_capture_plan`'s pointers, Q53).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormError {
    pub pointer: String,
    pub message: String,
}

#[derive(Resource, Debug, Clone)]
pub struct NewCapturePlanForm {
    pub open: bool,

    /// The plan the form edits: a clone of the loaded plan (of the example plan
    /// for + New Plan). Save writes what the form shows into a clone of it, so
    /// everything the form does not show is kept (F-15).
    pub draft: CapturePlan,

    // General
    pub plan_name: String,

    // Tether
    pub tether_name: String,
    /// `device.type`; empty = absent (`tether`).
    pub tether_type: String,
    /// `device.tether_length_m`; empty = absent (20 m).
    pub tether_length: NumberField,

    /// Every phase of the plan, in file order.
    pub phases: Vec<PhaseForm>,

    // UI state
    /// Non-empty when the user tried to save and a conflict was found.
    /// Contains the full path of the conflicting file.
    pub overwrite_conflict_path: Option<String>,
    pub validation_errors: Vec<FormError>,
    pub unit_system: UnitSystem,
    /// Some(plan_id) when editing an existing plan; None when creating new.
    pub editing_plan_id: Option<String>,
    /// Set to true after saving an edited plan in the sim screen.
    /// A poll system reads this, spawns the restart prompt, and clears it.
    pub show_restart_prompt: bool,
    /// When true, the form is in view-only mode (e.g. viewing an example plan).
    pub read_only: bool,
}

impl Default for NewCapturePlanForm {
    fn default() -> Self {
        Self {
            open: false,
            draft: CapturePlan {
                schema_version: PLAN_SCHEMA_VERSION,
                name: String::new(),
                id: String::new(),
                tether: String::new(),
                device: None,
                capture_sphere: None,
                phases: Vec::new(),
                migrated_from_v0: None,
            },
            plan_name: String::new(),
            tether_name: String::new(),
            tether_type: String::new(),
            tether_length: NumberField::default(),
            phases: Vec::new(),
            overwrite_conflict_path: None,
            validation_errors: Vec::new(),
            unit_system: UnitSystem::default(),
            editing_plan_id: None,
            show_restart_prompt: false,
            read_only: false,
        }
    }
}

impl NewCapturePlanForm {
    /// An empty, closed form, in the units the toggle is on.
    pub fn reset(&mut self) {
        *self = NewCapturePlanForm {
            unit_system: self.unit_system,
            ..Default::default()
        };
    }

    /// Shows the form in `unit`: every number keeps its physical value, so what
    /// Save writes does not depend on the toggle (OQ3).
    pub fn set_unit_system(&mut self, unit: UnitSystem) {
        let from = self.unit_system;
        if unit == from {
            return;
        }
        self.tether_length.set_unit(Quantity::Length, from, unit);
        for phase in &mut self.phases {
            phase.max_velocity.set_unit(Quantity::Speed, from, unit);
            phase.max_force.set_unit(Quantity::Force, from, unit);
            phase.shrink_rate.set_unit(Quantity::Speed, from, unit);
            for condition in phase
                .transitions
                .iter_mut()
                .flat_map(|transition| &mut transition.conditions)
            {
                let quantity = condition.kind.quantity();
                condition.less_than.set_unit(quantity, from, unit);
                condition.greater_than.set_unit(quantity, from, unit);
            }
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
