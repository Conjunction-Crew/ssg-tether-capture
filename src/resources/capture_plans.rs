use bevy::{platform::collections::HashMap, prelude::*};
use serde_json::Value;

use crate::components::capture_components::CapturePlan;

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
    pub plans: HashMap<String, CapturePlan>,
    pub compiled_plans: HashMap<String, CompiledCapturePlan>,
}

impl CapturePlanLibrary {
    pub fn insert_plan(&mut self, plan_id: String, plan: CapturePlan) {
        let compiled_plan = compile_capture_plan(&plan);
        self.plans.insert(plan_id.clone(), plan);
        self.compiled_plans.insert(plan_id, compiled_plan);
    }
}

#[derive(Resource, Debug, Default)]
pub struct CaptureSphereRadius {
    pub radius: f64,
}

fn compile_capture_plan(plan: &CapturePlan) -> CompiledCapturePlan {
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

fn parameter_value(parameters: &Option<Value>, key: &str) -> Option<f64> {
    parameters.as_ref()?.get(key)?.as_f64()
}

fn nested_value(value: &Value, key: &str, nested_key: &str) -> Option<f64> {
    value.get(key)?.get(nested_key)?.as_f64()
}
