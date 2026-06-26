use std::fs;
use std::path::Path;

use bevy::{platform::collections::HashMap, prelude::*};
use serde_json::Value;

use crate::components::capture_components::{CaptureComponent, CapturePlan};

#[derive(Debug, Clone, Default)]
pub struct CompiledCaptureTransition {
    pub to: String,
    pub distance_less_than: Option<f64>,
    pub distance_greater_than: Option<f64>,
    pub relative_velocity_less_than: Option<f64>,
    pub relative_velocity_greater_than: Option<f64>,
    /// Maximum normalized tether straightness (0 = perfectly straight) below which
    /// this transition fires. Used by the `terminal` phase to wait until the tether
    /// is straight before advancing to `capture`.
    pub straightness_less_than: Option<f64>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct CompiledCapturePhaseParameters {
    pub max_velocity: f64,
    pub max_force: f64,
    pub shrink_rate: Option<f64>,
}

#[derive(Debug, Clone, Default)]
pub struct CompiledCapturePhase {
    pub id: String,
    pub parameters: CompiledCapturePhaseParameters,
    pub transitions: Vec<CompiledCaptureTransition>,
}

#[derive(Debug, Clone, Default)]
pub struct CompiledCapturePlan {
    pub tether: String,
    pub phases: Vec<CompiledCapturePhase>,
    pub phase_indices: HashMap<String, usize>,
}

impl CompiledCapturePlan {
    pub fn phase(&self, phase_id: &str) -> Option<&CompiledCapturePhase> {
        self.phase_indices
            .get(phase_id)
            .and_then(|&index| self.phases.get(index))
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

/// Stores per-file validation errors for capture plans that failed to load.
#[derive(Resource, Debug, Default)]
pub struct CapturePlanLoadErrors {
    pub errors: HashMap<String, Vec<String>>,
}

#[derive(Resource, Debug, Default)]
pub struct CaptureSphereRadius {
    pub radius: f64,
}

/// Validates a deserialized [`CapturePlan`], returning a list of human-readable error
/// messages. An empty vec means the plan is valid.
pub fn validate_capture_plan(plan_id: &str, plan: &CapturePlan) -> Vec<String> {
    let mut errors = Vec::new();

    if plan.name.trim().is_empty() {
        errors.push(format!("[{plan_id}] 'name' field is empty."));
    }
    if plan.tether.trim().is_empty() {
        errors.push(format!("[{plan_id}] 'tether' field is empty."));
    }
    if let Some(device) = &plan.device {
        if device.tether_length <= 0.0 {
            errors.push(format!(
                "[{plan_id}] 'device.tether_length' must be greater than zero."
            ));
        }
    }
    for (field, orbit) in [("rso", &plan.rso), ("chaser", &plan.chaser)] {
        if let Some(orbit) = orbit {
            if orbit.semi_major_axis_m <= 0.0 {
                errors.push(format!(
                    "[{plan_id}] '{field}.semi_major_axis_m' must be greater than zero."
                ));
            }
            if !(0.0..1.0).contains(&orbit.eccentricity) {
                errors.push(format!(
                    "[{plan_id}] '{field}.eccentricity' must be in the range [0.0, 1.0)."
                ));
            }
        }
    }
    if plan.phases.is_empty() {
        errors.push(format!(
            "[{plan_id}] 'phases' array is empty — at least one phase is required."
        ));
        return errors;
    }

    // Detect duplicate phase IDs
    let mut seen_ids: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for phase in &plan.phases {
        if phase.id.trim().is_empty() {
            errors.push(format!("[{plan_id}] A phase has an empty 'id' field."));
        } else if !seen_ids.insert(phase.id.as_str()) {
            errors.push(format!("[{plan_id}] Duplicate phase id '{}'.", phase.id));
        }
    }

    let phase_ids: std::collections::HashSet<&str> =
        plan.phases.iter().map(|p| p.id.as_str()).collect();

    for phase in &plan.phases {
        if parameter_value(&phase.parameters, "max_velocity").is_none() {
            errors.push(format!(
                "[{plan_id}] Phase '{}' is missing required 'max_velocity' parameter.",
                phase.id
            ));
        }
        if parameter_value(&phase.parameters, "max_force").is_none() {
            errors.push(format!(
                "[{plan_id}] Phase '{}' is missing required 'max_force' parameter.",
                phase.id
            ));
        }
        if let Some(transitions) = &phase.transitions {
            for transition in transitions {
                match transition.get("to").and_then(Value::as_str) {
                    None => errors.push(format!(
                        "[{plan_id}] Phase '{}' has a transition missing a 'to' field.",
                        phase.id
                    )),
                    Some(to) if !phase_ids.contains(to) => errors.push(format!(
                        "[{plan_id}] Phase '{}' has a transition to unknown phase '{to}'.",
                        phase.id
                    )),
                    _ => {}
                }
            }
        }
    }

    errors
}

/// Builds a [`CaptureComponent`] from a plan and the current physics clock. Returns
/// `None` if the plan has no phases (should have been caught by validation).
pub fn build_capture_component(
    plan_id: &str,
    plan: &CapturePlan,
    physics_time_secs: f64,
) -> Option<CaptureComponent> {
    let first_phase = plan.phases.first()?;
    Some(CaptureComponent {
        plan_id: plan_id.to_string(),
        current_phase: first_phase.id.clone(),
        phase_enter_time_s: physics_time_secs,
        phase_elapsed_time_s: 0.0,
    })
}

/// Loads all valid `.json` capture plan files from `dir` and returns them as a map
/// keyed by file stem. Returns an empty map if the directory does not exist or cannot
/// be read.
pub fn load_plans_from_dir(dir: &Path) -> HashMap<String, CapturePlan> {
    let (plans, _) = load_plans_from_dir_with_errors(dir);
    plans
}

/// Like [`load_plans_from_dir`] but also returns a map of file stems to validation
/// error messages for any files that could not be parsed or failed validation. The
/// first return value contains only the successfully loaded plans.
pub fn load_plans_from_dir_with_errors(
    dir: &Path,
) -> (HashMap<String, CapturePlan>, HashMap<String, Vec<String>>) {
    let mut plans = HashMap::new();
    let mut errors: HashMap<String, Vec<String>> = HashMap::new();

    if !dir.exists() {
        return (plans, errors);
    }
    let read_dir = match fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return (plans, errors),
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        if !path.extension().is_some_and(|ext| ext == "json") {
            continue;
        }
        let stem = match path.file_stem().and_then(|s| s.to_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };
        let raw_json = match fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                errors.insert(stem, vec![format!("Could not read file: {e}")]);
                continue;
            }
        };
        let mut plan = match serde_json::from_str::<CapturePlan>(&raw_json) {
            Ok(p) => p,
            Err(e) => {
                errors.insert(stem, vec![format!("Invalid JSON: {e}")]);
                continue;
            }
        };
        let validation_errors = validate_capture_plan(&stem, &plan);
        if validation_errors.is_empty() {
            plan.id = stem.clone();
            plans.insert(stem, plan);
        } else {
            errors.insert(stem, validation_errors);
        }
    }
    (plans, errors)
}

pub(crate) fn compile_capture_plan(plan: &CapturePlan) -> CompiledCapturePlan {
    let mut phase_indices = HashMap::default();
    let mut phases = Vec::with_capacity(plan.phases.len());

    for (index, phase) in plan.phases.iter().enumerate() {
        phase_indices.insert(phase.id.clone(), index);
        phases.push(CompiledCapturePhase {
            id: phase.id.clone(),
            parameters: CompiledCapturePhaseParameters {
                max_velocity: parameter_value(&phase.parameters, "max_velocity").unwrap_or(0.0),
                max_force: parameter_value(&phase.parameters, "max_force").unwrap_or(0.0),
                shrink_rate: parameter_value(&phase.parameters, "shrink_rate"),
            },
            transitions: phase
                .transitions
                .as_ref()
                .into_iter()
                .flatten()
                .filter_map(|transition| {
                    let to = transition.get("to").and_then(Value::as_str)?;
                    Some(CompiledCaptureTransition {
                        to: to.to_string(),
                        distance_less_than: nested_value(transition, "distance", "less_than"),
                        distance_greater_than: nested_value(transition, "distance", "greater_than"),
                        relative_velocity_less_than: nested_value(
                            transition,
                            "relative_velocity",
                            "less_than",
                        ),
                        relative_velocity_greater_than: nested_value(
                            transition,
                            "relative_velocity",
                            "greater_than",
                        ),
                        straightness_less_than: nested_value(
                            transition,
                            "straightness",
                            "less_than",
                        ),
                    })
                })
                .collect(),
        });
    }

    CompiledCapturePlan {
        tether: plan.tether.clone(),
        phases,
        phase_indices,
    }
}

pub(crate) fn parameter_value(parameters: &Option<Value>, key: &str) -> Option<f64> {
    parameters.as_ref()?.get(key)?.as_f64()
}

pub(crate) fn nested_value(value: &Value, key: &str, nested_key: &str) -> Option<f64> {
    value.get(key)?.get(nested_key)?.as_f64()
}
