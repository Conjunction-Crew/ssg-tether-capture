//! The v0 capture-plan model and its compile, as they were before schema v1
//! (`capture::plan` at v0.3.3), kept as the test oracle for the v0 → v1 migration:
//! `compile_capture_plan(migrate(v0))` must equal [`compile_v0`] of the same file,
//! minus the transitions that never fire.

use bevy::platform::collections::HashMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::capture::plan::{
    CompiledCapturePlan, CompiledCaptureState, CompiledCaptureStateParameters,
    CompiledCaptureTransition,
};

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
pub struct CapturePlanV0 {
    /// Display name shown to the user. Not used as a lookup key.
    pub name: String,
    /// File-stem identifier used as the HashMap key in `CapturePlanLibrary`.
    /// Not serialized to JSON — populated by the loader and `CapturePlanLibrary::insert_plan`.
    #[serde(skip)]
    #[allow(dead_code)] // kept as the v0 model had it; the oracle never reads it
    pub id: String,
    pub states: Vec<State>,
    pub tether: String,
    #[serde(default)]
    pub device: Option<CapturePlanDevice>,
}

pub fn compile_v0(plan: &CapturePlanV0) -> CompiledCapturePlan {
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
                tail_actuation: false,
                velocity_scales_with_sphere: false,
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
            // v0 has no end phases.
            end: None,
        });
    }

    CompiledCapturePlan {
        tether: plan.tether.clone(),
        states,
        state_indices,
        // v0 has no floor key: the default.
        min_capture_radius_m: crate::capture::MIN_CAPTURE_SPHERE_RADIUS_M,
    }
}

pub fn parameter_value(parameters: &Option<Value>, key: &str) -> Option<f64> {
    parameters.as_ref()?.get(key)?.as_f64()
}

pub fn nested_value(value: &Value, key: &str, nested_key: &str) -> Option<f64> {
    value.get(key)?.get(nested_key)?.as_f64()
}

#[test]
fn parameter_value_extracts_f64() {
    let params = Some(serde_json::json!({ "max_velocity": 2.5 }));
    assert_eq!(parameter_value(&params, "max_velocity"), Some(2.5));
}

#[test]
fn parameter_value_returns_none_for_missing_key() {
    let params = Some(serde_json::json!({ "max_force": 1.0 }));
    assert_eq!(parameter_value(&params, "max_velocity"), None);
}

#[test]
fn parameter_value_returns_none_for_none_params() {
    assert_eq!(parameter_value(&None, "max_velocity"), None);
}
