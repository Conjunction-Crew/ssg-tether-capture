//! Capture plans: the file model, validation and the compiled form.

use bevy::{platform::collections::HashMap, prelude::*};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::sim::capture::CaptureComponent;

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

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CapturePlan {
    /// Display name shown to the user. Not used as a lookup key.
    pub name: String,
    /// File-stem identifier used as the HashMap key in [`CapturePlanLibrary`].
    /// Not serialized to JSON — populated by the loader and [`CapturePlanLibrary::insert_plan`].
    #[serde(skip)]
    pub id: String,
    pub states: Vec<State>,
    pub tether: String,
    #[serde(default)]
    pub device: Option<CapturePlanDevice>,
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
    if let Some(device) = &plan.device
        && device.tether_length <= 0.0
    {
        errors.push(format!(
            "[{plan_id}] 'device.tether_length' must be greater than zero."
        ));
    }
    if plan.states.is_empty() {
        errors.push(format!(
            "[{plan_id}] 'states' array is empty — at least one state is required."
        ));
        return errors;
    }

    // Detect duplicate state IDs
    let mut seen_ids: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for state in &plan.states {
        if state.id.trim().is_empty() {
            errors.push(format!("[{plan_id}] A state has an empty 'id' field."));
        } else if !seen_ids.insert(state.id.as_str()) {
            errors.push(format!("[{plan_id}] Duplicate state id '{}'.", state.id));
        }
    }

    let state_ids: std::collections::HashSet<&str> =
        plan.states.iter().map(|s| s.id.as_str()).collect();

    for state in &plan.states {
        if parameter_value(&state.parameters, "max_velocity").is_none() {
            errors.push(format!(
                "[{plan_id}] State '{}' is missing required 'max_velocity' parameter.",
                state.id
            ));
        }
        if parameter_value(&state.parameters, "max_force").is_none() {
            errors.push(format!(
                "[{plan_id}] State '{}' is missing required 'max_force' parameter.",
                state.id
            ));
        }
        if let Some(transitions) = &state.transitions {
            for transition in transitions {
                match transition.get("to").and_then(Value::as_str) {
                    None => errors.push(format!(
                        "[{plan_id}] State '{}' has a transition missing a 'to' field.",
                        state.id
                    )),
                    Some(to) if !state_ids.contains(to) => errors.push(format!(
                        "[{plan_id}] State '{}' has a transition to unknown state '{to}'.",
                        state.id
                    )),
                    _ => {}
                }
            }
        }
    }

    errors
}

/// Builds a [`CaptureComponent`] from a plan and the current physics clock. Returns
/// `None` if the plan has no states (should have been caught by validation).
pub fn build_capture_component(
    plan_id: &str,
    plan: &CapturePlan,
    physics_time_secs: f64,
) -> Option<CaptureComponent> {
    let first_state = plan.states.first()?;
    Some(CaptureComponent {
        plan_id: plan_id.to_string(),
        current_state: first_state.id.clone(),
        state_enter_time_s: physics_time_secs,
        state_elapsed_time_s: 0.0,
    })
}

pub(crate) fn compile_capture_plan(plan: &CapturePlan) -> CompiledCapturePlan {
    let mut state_indices = HashMap::default();
    let mut states = Vec::with_capacity(plan.states.len());

    for (index, state) in plan.states.iter().enumerate() {
        state_indices.insert(state.id.clone(), index);
        states.push(CompiledCaptureState {
            id: state.id.clone(),
            parameters: CompiledCaptureStateParameters {
                max_velocity: parameter_value(&state.parameters, "max_velocity").unwrap_or(0.0),
                max_force: parameter_value(&state.parameters, "max_force").unwrap_or(0.0),
                shrink_rate: parameter_value(&state.parameters, "shrink_rate"),
            },
            transitions: state
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
                    })
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

pub(crate) fn parameter_value(parameters: &Option<Value>, key: &str) -> Option<f64> {
    parameters.as_ref()?.get(key)?.as_f64()
}

pub(crate) fn nested_value(value: &Value, key: &str, nested_key: &str) -> Option<f64> {
    value.get(key)?.get(nested_key)?.as_f64()
}
