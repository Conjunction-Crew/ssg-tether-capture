//! Capture plans: the file model (schema v1), validation and the compiled form.

use bevy::{platform::collections::HashMap, prelude::*};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::capture::{CaptureComponent, INITIAL_CAPTURE_SPHERE_RADIUS_M};

/// The `schema_version` of the plan files this build reads and writes.
pub const PLAN_SCHEMA_VERSION: u32 = 1;

const DEFAULT_DEVICE_TYPE: &str = "tether";
pub(crate) const DEFAULT_TETHER_LENGTH_M: f64 = 20.0;
pub(crate) const DEFAULT_SEGMENT_SPACING_M: f64 = 0.1;
const DEFAULT_SEGMENT_MASS_KG: f64 = 0.1;
const DEFAULT_END_MASS_KG: f64 = 2.0;
const DEFAULT_JOINT_DAMPING: f64 = 1.0;
/// Radius of the root and tail spheres (the spawner's `root_tail_radius`).
const END_SPHERE_RADIUS_M: f64 = 0.5;
/// Tether body counts (interior nodes + root + tail): above the first a plan is
/// refused, above the second it warns (Q16 recording budget).
const MAX_TETHER_BODIES: u64 = 1_000;
const WARN_TETHER_BODIES: u64 = 200;

/// A capture plan file, schema v1. Lengths are metres, speeds m/s, forces N,
/// masses kg; there is no unit key.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub struct CapturePlan {
    /// File format version; `1`. A file without it is a v0 plan and is migrated on load.
    #[cfg_attr(test, schemars(range(min = 1, max = 1)))]
    pub schema_version: u32,
    /// Display name shown to the user. Not used as a lookup key. Must not be blank.
    pub name: String,
    /// File-stem identifier used as the HashMap key in [`CapturePlanLibrary`].
    /// Not serialized to JSON — populated by the loader and [`CapturePlanLibrary::insert_plan`].
    #[serde(skip)]
    pub id: String,
    /// Id the tether is registered under. Must not be blank.
    pub tether: String,
    /// The capture device. Absent: every device default applies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<CapturePlanDevice>,
    /// The capture sphere. Absent: every default applies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capture_sphere: Option<CaptureSphere>,
    /// The phases in order; a capture starts in the first. At least one.
    pub phases: Vec<Phase>,
    /// What a migrated v0 file held that v1 has no field for (written back unchanged).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub migrated_from_v0: Option<MigratedFromV0>,
}

/// The capture device. Every field is optional; an absent field takes its default.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(deny_unknown_fields)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub struct CapturePlanDevice {
    /// Device kind, free text. Default `"tether"`.
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub device_type: Option<String>,
    /// Physical length of the tether in metres. Default 20.0; greater than 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tether_length_m: Option<f64>,
    /// Distance between tether joints in metres. Default 0.1; greater than 0, and the
    /// tether may have at most 1,000 bodies (a warning above 200).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub segment_spacing_m: Option<f64>,
    /// Mass of each interior tether node in kg. Default 0.1; greater than 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub segment_mass_kg: Option<f64>,
    /// Mass of the root and of the tail in kg. Default 2.0; greater than 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_mass_kg: Option<f64>,
    /// Linear damping of each tether joint. Default 1.0; not negative.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub joint_damping_linear: Option<f64>,
    /// Angular damping of each tether joint. Default 1.0; not negative.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub joint_damping_angular: Option<f64>,
}

impl CapturePlanDevice {
    /// The device kind, `"tether"` when absent.
    pub fn device_type(&self) -> &str {
        self.device_type.as_deref().unwrap_or(DEFAULT_DEVICE_TYPE)
    }

    /// The tether length in metres, 20.0 when absent.
    pub fn tether_length_m(&self) -> f64 {
        self.tether_length_m.unwrap_or(DEFAULT_TETHER_LENGTH_M)
    }

    /// The joint spacing in metres, 0.1 when absent.
    pub fn segment_spacing_m(&self) -> f64 {
        self.segment_spacing_m.unwrap_or(DEFAULT_SEGMENT_SPACING_M)
    }

    /// The interior node mass in kg, 0.1 when absent.
    pub fn segment_mass_kg(&self) -> f64 {
        self.segment_mass_kg.unwrap_or(DEFAULT_SEGMENT_MASS_KG)
    }

    /// The root and tail mass in kg, 2.0 when absent.
    pub fn end_mass_kg(&self) -> f64 {
        self.end_mass_kg.unwrap_or(DEFAULT_END_MASS_KG)
    }

    /// The joints' linear damping, 1.0 when absent.
    pub fn joint_damping_linear(&self) -> f64 {
        self.joint_damping_linear.unwrap_or(DEFAULT_JOINT_DAMPING)
    }

    /// The joints' angular damping, 1.0 when absent.
    pub fn joint_damping_angular(&self) -> f64 {
        self.joint_damping_angular.unwrap_or(DEFAULT_JOINT_DAMPING)
    }

    /// Root + tail + interior nodes, as the spawner counts them.
    fn tether_body_count(&self) -> u64 {
        let interior = ((self.tether_length_m() - 2.0 * END_SPHERE_RADIUS_M)
            / self.segment_spacing_m())
        .max(0.0) as u64;
        interior.saturating_add(2)
    }
}

/// The capture sphere around the target.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(deny_unknown_fields)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub struct CaptureSphere {
    /// Radius in metres at the start of every capture. Default 25.0; greater than 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_radius_m: Option<f64>,
}

impl CapturePlan {
    /// The capture sphere's initial radius in metres, 25.0 when absent.
    pub fn initial_capture_radius_m(&self) -> f64 {
        self.capture_sphere
            .as_ref()
            .and_then(|sphere| sphere.initial_radius_m)
            .unwrap_or(INITIAL_CAPTURE_SPHERE_RADIUS_M)
    }
}

/// One phase of the plan: the controller's limits while in it and its transitions.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub struct Phase {
    /// Phase id, unique in the plan and not blank.
    pub id: String,
    /// Speed limit of the tether nodes in m/s; not negative.
    pub max_velocity_mps: f64,
    /// Force applied to each tether node in N; not negative.
    pub max_force_n: f64,
    /// Rate at which the capture sphere shrinks in m/s; not negative. Absent: no shrink.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shrink_rate_mps: Option<f64>,
    /// Checked in order every step; the first whose conditions all hold fires.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transitions: Option<Vec<Transition>>,
    /// Marks an end phase (`success` or `abort`); an end phase has no transitions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<PhaseEnd>,
}

/// How an end phase ends the capture.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub enum PhaseEnd {
    Success,
    Abort,
}

/// A transition to another phase.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub struct Transition {
    /// Id of the phase to enter.
    pub to: String,
    /// The conditions; all must hold (AND). At least one.
    pub all_of: TransitionConditions,
}

/// The conditions of a transition.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(deny_unknown_fields)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub struct TransitionConditions {
    /// Range between the tether root and the target in metres.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub distance_m: Option<ConditionBounds>,
    /// Magnitude of the LVLH relative velocity in m/s.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relative_speed_mps: Option<ConditionBounds>,
}

/// Bounds on one quantity; at least one, neither negative. Both strict.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, Default, PartialEq)]
#[serde(deny_unknown_fields)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub struct ConditionBounds {
    /// Holds while the quantity is below this value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub less_than: Option<f64>,
    /// Holds while the quantity is above this value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub greater_than: Option<f64>,
}

/// The v0 content a migration could not map, so the file can be written back
/// without losing it.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(deny_unknown_fields)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub struct MigratedFromV0 {
    /// Every unmapped v0 value with its JSON Pointer into the v0 document.
    pub unmapped: Vec<UnmappedValue>,
}

/// One unmapped v0 value.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub struct UnmappedValue {
    /// JSON Pointer into the v0 document, e.g. `/states/2/transitions/0/distance/units`.
    pub pointer: String,
    /// The value as the v0 file had it.
    pub value: Value,
}

#[derive(Debug, Clone, Default)]
pub struct CompiledCaptureTransition {
    pub to: String,
    pub distance_less_than: Option<f64>,
    pub distance_greater_than: Option<f64>,
    pub relative_velocity_less_than: Option<f64>,
    pub relative_velocity_greater_than: Option<f64>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct CompiledCaptureStateParameters {
    pub max_velocity: f64,
    pub max_force: f64,
    pub shrink_rate: Option<f64>,
}

#[derive(Debug, Clone, Default)]
pub struct CompiledCaptureState {
    pub id: String,
    pub parameters: CompiledCaptureStateParameters,
    pub transitions: Vec<CompiledCaptureTransition>,
}

#[derive(Debug, Clone, Default)]
pub struct CompiledCapturePlan {
    pub tether: String,
    pub states: Vec<CompiledCaptureState>,
    pub state_indices: HashMap<String, usize>,
}

impl CompiledCapturePlan {
    pub fn state(&self, state_id: &str) -> Option<&CompiledCaptureState> {
        self.state_indices
            .get(state_id)
            .and_then(|&index| self.states.get(index))
    }
}

#[derive(Resource, Debug, Default)]
pub struct CapturePlanLibrary {
    /// Union of user_plans and example_plans — used by simulation systems.
    pub plans: HashMap<String, CapturePlan>,
    /// Plans loaded from the user's working directory.
    pub user_plans: HashMap<String, CapturePlan>,
    /// Plans loaded from assets/example_capture_plans.
    pub example_plans: HashMap<String, CapturePlan>,
    /// Plans compiled into a structure more optimized for algorithms
    pub compiled_plans: HashMap<String, CompiledCapturePlan>,
}

impl CapturePlanLibrary {
    pub fn insert_plan(&mut self, plan_id: String, mut plan: CapturePlan) {
        plan.id = plan_id.clone();
        let compiled_plan = compile_capture_plan(&plan);
        self.plans.insert(plan_id.clone(), plan);
        self.compiled_plans.insert(plan_id, compiled_plan);
    }
}

/// Validates a [`CapturePlan`] against the v1 rules, returning one message per
/// broken rule, in field order, as `[<plan-id>] <json-pointer>: <message>`. An
/// empty vec means the plan is valid.
pub fn validate_capture_plan(plan_id: &str, plan: &CapturePlan) -> Vec<String> {
    let mut errors = Vec::new();
    let mut error = |pointer: &str, message: &str| {
        errors.push(format!("[{plan_id}] {pointer}: {message}"));
    };
    const POSITIVE: &str = "must be greater than 0";
    const NOT_NEGATIVE: &str = "must not be negative";

    if plan.schema_version != PLAN_SCHEMA_VERSION {
        error("/schema_version", "must be 1");
    }
    if plan.name.trim().is_empty() {
        error("/name", "must not be blank");
    }
    // F-43: a blank tether id would make the sidebar and the spawner disagree.
    if plan.tether.trim().is_empty() {
        error("/tether", "must not be blank");
    }
    if let Some(device) = &plan.device {
        let length_ok = device.tether_length_m.is_none_or(|l| l > 0.0);
        let spacing_ok = device.segment_spacing_m.is_none_or(|s| s > 0.0);
        if !length_ok {
            error("/device/tether_length_m", POSITIVE);
        }
        if !spacing_ok {
            error("/device/segment_spacing_m", POSITIVE);
        }
        if length_ok && spacing_ok && device.tether_body_count() > MAX_TETHER_BODIES {
            error(
                "/device/segment_spacing_m",
                &format!(
                    "gives {} tether bodies; at most {MAX_TETHER_BODIES} are allowed",
                    device.tether_body_count()
                ),
            );
        }
        for (pointer, value) in [
            ("/device/segment_mass_kg", device.segment_mass_kg),
            ("/device/end_mass_kg", device.end_mass_kg),
        ] {
            if value.is_some_and(|v| v <= 0.0) {
                error(pointer, POSITIVE);
            }
        }
        for (pointer, value) in [
            ("/device/joint_damping_linear", device.joint_damping_linear),
            (
                "/device/joint_damping_angular",
                device.joint_damping_angular,
            ),
        ] {
            if value.is_some_and(|v| v < 0.0) {
                error(pointer, NOT_NEGATIVE);
            }
        }
    }
    if let Some(sphere) = &plan.capture_sphere
        && sphere.initial_radius_m.is_some_and(|r| r <= 0.0)
    {
        error("/capture_sphere/initial_radius_m", POSITIVE);
    }
    if plan.phases.is_empty() {
        error("/phases", "at least one phase is required");
        return errors;
    }

    let phase_ids: std::collections::HashSet<&str> =
        plan.phases.iter().map(|p| p.id.as_str()).collect();
    let mut seen_ids: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for (i, phase) in plan.phases.iter().enumerate() {
        let at = |field: &str| format!("/phases/{i}/{field}");
        if phase.id.trim().is_empty() {
            error(&at("id"), "must not be blank");
        } else if !seen_ids.insert(phase.id.as_str()) {
            error(&at("id"), &format!("duplicate phase id '{}'", phase.id));
        }
        for (field, value) in [
            ("max_velocity_mps", Some(phase.max_velocity_mps)),
            ("max_force_n", Some(phase.max_force_n)),
            ("shrink_rate_mps", phase.shrink_rate_mps),
        ] {
            if value.is_some_and(|v| v < 0.0) {
                error(&at(field), NOT_NEGATIVE);
            }
        }
        let transitions = phase.transitions.as_deref().unwrap_or_default();
        for (j, transition) in transitions.iter().enumerate() {
            let at = |field: &str| format!("/phases/{i}/transitions/{j}/{field}");
            if !phase_ids.contains(transition.to.as_str()) {
                error(&at("to"), &format!("unknown phase '{}'", transition.to));
            }
            let conditions = [
                ("distance_m", transition.all_of.distance_m),
                ("relative_speed_mps", transition.all_of.relative_speed_mps),
            ];
            if conditions.iter().all(|(_, bounds)| bounds.is_none()) {
                error(&at("all_of"), "at least one condition is required");
            }
            for (name, bounds) in conditions {
                let Some(bounds) = bounds else { continue };
                if bounds.less_than.is_none() && bounds.greater_than.is_none() {
                    error(
                        &at(&format!("all_of/{name}")),
                        "at least one of less_than, greater_than is required",
                    );
                }
                for (bound, value) in [
                    ("less_than", bounds.less_than),
                    ("greater_than", bounds.greater_than),
                ] {
                    if value.is_some_and(|v| v < 0.0) {
                        error(&at(&format!("all_of/{name}/{bound}")), NOT_NEGATIVE);
                    }
                }
            }
        }
        if phase.end.is_some() && !transitions.is_empty() {
            error(&at("transitions"), "an end phase has no transitions");
        }
    }

    errors
}

/// What is valid but worth a warning, as `[<plan-id>] <json-pointer>: <message>`:
/// a tether of more than 200 bodies.
pub(crate) fn capture_plan_warnings(plan_id: &str, plan: &CapturePlan) -> Vec<String> {
    let mut warnings = Vec::new();
    if let Some(device) = &plan.device {
        let bodies = device.tether_body_count();
        if bodies > WARN_TETHER_BODIES && bodies <= MAX_TETHER_BODIES {
            warnings.push(format!(
                "[{plan_id}] /device/segment_spacing_m: gives {bodies} tether bodies; \
                 more than {WARN_TETHER_BODIES} make recordings large"
            ));
        }
    }
    warnings
}

/// Builds a [`CaptureComponent`] from a plan, entering its first phase at
/// `sim_time_s` (sim seconds since the start epoch). Returns `None` if the plan
/// has no phases (should have been caught by validation).
pub fn build_capture_component(
    plan_id: &str,
    plan: &CapturePlan,
    sim_time_s: f64,
) -> Option<CaptureComponent> {
    let first_phase = plan.phases.first()?;
    Some(CaptureComponent {
        plan_id: plan_id.to_string(),
        current_state: first_phase.id.clone(),
        state_enter_time_s: sim_time_s,
        state_elapsed_time_s: 0.0,
    })
}

pub(crate) fn compile_capture_plan(plan: &CapturePlan) -> CompiledCapturePlan {
    let mut state_indices = HashMap::default();
    let mut states = Vec::with_capacity(plan.phases.len());

    for (index, phase) in plan.phases.iter().enumerate() {
        state_indices.insert(phase.id.clone(), index);
        states.push(CompiledCaptureState {
            id: phase.id.clone(),
            parameters: CompiledCaptureStateParameters {
                max_velocity: phase.max_velocity_mps,
                max_force: phase.max_force_n,
                shrink_rate: phase.shrink_rate_mps,
            },
            transitions: phase
                .transitions
                .iter()
                .flatten()
                .map(|transition| {
                    let distance = transition.all_of.distance_m.unwrap_or_default();
                    let speed = transition.all_of.relative_speed_mps.unwrap_or_default();
                    CompiledCaptureTransition {
                        to: transition.to.clone(),
                        distance_less_than: distance.less_than,
                        distance_greater_than: distance.greater_than,
                        relative_velocity_less_than: speed.less_than,
                        relative_velocity_greater_than: speed.greater_than,
                    }
                })
                .collect(),
        });
    }

    CompiledCapturePlan {
        tether: plan.tether.clone(),
        states,
        state_indices,
    }
}
