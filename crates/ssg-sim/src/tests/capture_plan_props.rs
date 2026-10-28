//! Property-style tests for capture-plan validation and compilation (P5 S4, A-4).
//!
//! Deterministic generated inputs: a fixed-seed [`Lcg`] builds a few hundred random
//! valid plans; the first property says each validates, compiles and starts a
//! capture as its spec says; the second applies one mutation from a fixed list to
//! a valid plan and expects exactly the message the validator gives today. A
//! failure names the seed, the case and the mutation, so it reproduces from them.

use serde_json::{Value, json};

use crate::capture::plan::{
    CapturePlan, CapturePlanDevice, CapturePlanLibrary, State, build_capture_component,
    validate_capture_plan,
};
use crate::test_support::Lcg;

const SEED: u64 = 0x5EED_CA97;
const CASES: usize = 300;
const PLAN_ID: &str = "generated";

/// One transition as generated: a target state index and optional conditions.
#[derive(Clone, Debug)]
struct TransitionSpec {
    to: usize,
    distance_less_than: Option<f64>,
    distance_greater_than: Option<f64>,
    relative_velocity_less_than: Option<f64>,
    relative_velocity_greater_than: Option<f64>,
}

#[derive(Clone, Debug)]
struct StateSpec {
    max_velocity: f64,
    max_force: f64,
    shrink_rate: Option<f64>,
    transitions: Vec<TransitionSpec>,
}

#[derive(Clone, Debug)]
struct PlanSpec {
    tether: String,
    tether_length: Option<f64>,
    states: Vec<StateSpec>,
}

fn state_id(index: usize) -> String {
    format!("state_{index}")
}

fn maybe(rng: &mut Lcg, lo: f64, hi: f64) -> Option<f64> {
    rng.chance(0.4).then(|| rng.range(lo, hi))
}

fn generate_spec(rng: &mut Lcg, case: usize) -> PlanSpec {
    let state_count = 1 + rng.below(7);
    let states = (0..state_count)
        .map(|_| StateSpec {
            max_velocity: rng.range(0.01, 100.0),
            max_force: rng.range(0.01, 100.0),
            shrink_rate: rng.chance(0.5).then(|| rng.range(0.001, 0.1)),
            transitions: (0..rng.below(4))
                .map(|_| TransitionSpec {
                    to: rng.below(state_count),
                    distance_less_than: maybe(rng, 0.1, 5_000.0),
                    distance_greater_than: maybe(rng, 0.1, 5_000.0),
                    relative_velocity_less_than: maybe(rng, 0.01, 50.0),
                    relative_velocity_greater_than: maybe(rng, 0.01, 50.0),
                })
                .collect(),
        })
        .collect();
    PlanSpec {
        tether: format!("Tether{}", case % 5),
        tether_length: rng.chance(0.5).then(|| rng.range(0.1, 100.0)),
        states,
    }
}

fn transition_json(transition: &TransitionSpec) -> Value {
    let mut object = json!({ "to": state_id(transition.to) });
    let conditions = [
        ("distance", "less_than", transition.distance_less_than),
        ("distance", "greater_than", transition.distance_greater_than),
        (
            "relative_velocity",
            "less_than",
            transition.relative_velocity_less_than,
        ),
        (
            "relative_velocity",
            "greater_than",
            transition.relative_velocity_greater_than,
        ),
    ];
    for (key, nested, value) in conditions {
        if let Some(value) = value {
            object[key][nested] = json!(value);
        }
    }
    object
}

fn build_plan(spec: &PlanSpec, case: usize) -> CapturePlan {
    CapturePlan {
        name: format!("Generated plan {case}"),
        id: String::new(),
        tether: spec.tether.clone(),
        device: spec.tether_length.map(|tether_length| CapturePlanDevice {
            device_type: "tether".to_string(),
            tether_length,
        }),
        states: spec
            .states
            .iter()
            .enumerate()
            .map(|(index, state)| {
                let mut parameters = json!({
                    "max_velocity": state.max_velocity,
                    "max_force": state.max_force,
                });
                if let Some(shrink_rate) = state.shrink_rate {
                    parameters["shrink_rate"] = json!(shrink_rate);
                }
                State {
                    id: state_id(index),
                    next: None,
                    parameters: Some(parameters),
                    transitions: (!state.transitions.is_empty())
                        .then(|| state.transitions.iter().map(transition_json).collect()),
                    next_conditions: None,
                }
            })
            .collect(),
    }
}

#[test]
fn generated_valid_plans_validate_compile_and_start() {
    let mut rng = Lcg::new(SEED);
    for case in 0..CASES {
        let spec = generate_spec(&mut rng, case);
        let ctx = format!("seed {SEED:#x} case {case}: {spec:?}");
        let plan = build_plan(&spec, case);

        assert_eq!(
            validate_capture_plan(PLAN_ID, &plan),
            Vec::<String>::new(),
            "{ctx}"
        );

        let component = build_capture_component(PLAN_ID, &plan, 12.5)
            .unwrap_or_else(|| panic!("no capture component: {ctx}"));
        assert_eq!(component.plan_id, PLAN_ID, "{ctx}");
        assert_eq!(component.current_state, "state_0", "{ctx}");
        assert_eq!(component.state_enter_time_s, 12.5, "{ctx}");
        assert_eq!(component.state_elapsed_time_s, 0.0, "{ctx}");

        let mut library = CapturePlanLibrary::default();
        library.insert_plan(PLAN_ID.to_string(), plan);
        assert_eq!(library.plans[PLAN_ID].id, PLAN_ID, "{ctx}");
        let compiled = &library.compiled_plans[PLAN_ID];
        assert_eq!(compiled.tether, spec.tether, "{ctx}");
        assert_eq!(compiled.states.len(), spec.states.len(), "{ctx}");
        for (index, state) in spec.states.iter().enumerate() {
            let compiled_state = compiled
                .state(&state_id(index))
                .unwrap_or_else(|| panic!("state {index} is not indexed: {ctx}"));
            assert_eq!(compiled_state.id, state_id(index), "{ctx}");
            let parameters = compiled_state.parameters;
            assert_eq!(parameters.max_velocity, state.max_velocity, "{ctx}");
            assert_eq!(parameters.max_force, state.max_force, "{ctx}");
            assert_eq!(parameters.shrink_rate, state.shrink_rate, "{ctx}");
            assert_eq!(
                compiled_state.transitions.len(),
                state.transitions.len(),
                "{ctx}"
            );
            for (compiled_transition, transition) in
                compiled_state.transitions.iter().zip(&state.transitions)
            {
                assert_eq!(compiled_transition.to, state_id(transition.to), "{ctx}");
                assert_eq!(
                    compiled_transition.distance_less_than, transition.distance_less_than,
                    "{ctx}"
                );
                assert_eq!(
                    compiled_transition.distance_greater_than, transition.distance_greater_than,
                    "{ctx}"
                );
                assert_eq!(
                    compiled_transition.relative_velocity_less_than,
                    transition.relative_velocity_less_than,
                    "{ctx}"
                );
                assert_eq!(
                    compiled_transition.relative_velocity_greater_than,
                    transition.relative_velocity_greater_than,
                    "{ctx}"
                );
            }
        }
        assert!(compiled.state("no_such_state").is_none(), "{ctx}");
    }
}

#[derive(Clone, Copy, Debug)]
enum Mutation {
    BlankName,
    BlankTether,
    NonPositiveTetherLength,
    NoStates,
    DuplicateStateId,
    EmptyStateId,
    MissingMaxVelocity,
    MissingMaxForce,
    NoParameters,
    TransitionToUnknownState,
    TransitionWithoutTo,
}

const MUTATIONS: [Mutation; 11] = [
    Mutation::BlankName,
    Mutation::BlankTether,
    Mutation::NonPositiveTetherLength,
    Mutation::NoStates,
    Mutation::DuplicateStateId,
    Mutation::EmptyStateId,
    Mutation::MissingMaxVelocity,
    Mutation::MissingMaxForce,
    Mutation::NoParameters,
    Mutation::TransitionToUnknownState,
    Mutation::TransitionWithoutTo,
];

fn blank(rng: &mut Lcg) -> String {
    ["", " ", "   ", "\t"][rng.below(4)].to_string()
}

/// Applies `mutation` to the valid `plan` and returns the messages the validator
/// gives for it today (in its order).
fn mutate(plan: &mut CapturePlan, mutation: Mutation, rng: &mut Lcg, case: usize) -> Vec<String> {
    let pick = rng.below(plan.states.len());
    let picked = plan.states[pick].id.clone();
    let tag = format!("[{PLAN_ID}]");
    match mutation {
        Mutation::BlankName => {
            plan.name = blank(rng);
            vec![format!("{tag} 'name' field is empty.")]
        }
        Mutation::BlankTether => {
            plan.tether = blank(rng);
            vec![format!("{tag} 'tether' field is empty.")]
        }
        Mutation::NonPositiveTetherLength => {
            let tether_length = if rng.chance(0.5) {
                0.0
            } else {
                -rng.range(0.001, 100.0)
            };
            plan.device = Some(CapturePlanDevice {
                device_type: "tether".to_string(),
                tether_length,
            });
            vec![format!(
                "{tag} 'device.tether_length' must be greater than zero."
            )]
        }
        Mutation::NoStates => {
            plan.states.clear();
            vec![format!(
                "{tag} 'states' array is empty — at least one state is required."
            )]
        }
        Mutation::DuplicateStateId => {
            let copy = plan.states[pick].clone();
            plan.states.push(copy);
            vec![format!("{tag} Duplicate state id '{picked}'.")]
        }
        Mutation::EmptyStateId => {
            let mut extra = plan.states[pick].clone();
            extra.id = blank(rng);
            plan.states.push(extra);
            vec![format!("{tag} A state has an empty 'id' field.")]
        }
        Mutation::MissingMaxVelocity | Mutation::MissingMaxForce => {
            let (key, other) = match mutation {
                Mutation::MissingMaxVelocity => ("max_velocity", "max_force"),
                _ => ("max_force", "max_velocity"),
            };
            let parameters = plan.states[pick].parameters.as_mut().expect("parameters");
            // Absent, or present but not a number: both are "missing" today.
            if rng.chance(0.5) {
                parameters.as_object_mut().expect("object").remove(key);
            } else {
                parameters[key] = json!("fast");
            }
            assert!(parameters.get(other).is_some_and(Value::is_number));
            vec![format!(
                "{tag} State '{picked}' is missing required '{key}' parameter."
            )]
        }
        Mutation::NoParameters => {
            plan.states[pick].parameters = None;
            vec![
                format!("{tag} State '{picked}' is missing required 'max_velocity' parameter."),
                format!("{tag} State '{picked}' is missing required 'max_force' parameter."),
            ]
        }
        Mutation::TransitionToUnknownState => {
            let unknown = format!("nowhere_{case}");
            plan.states[pick]
                .transitions
                .get_or_insert_with(Vec::new)
                .push(json!({ "to": unknown, "distance": { "less_than": 1.0 } }));
            vec![format!(
                "{tag} State '{picked}' has a transition to unknown state '{unknown}'."
            )]
        }
        Mutation::TransitionWithoutTo => {
            let transition = if rng.chance(0.5) {
                json!({ "distance": { "less_than": 1.0 } })
            } else {
                // A `to` that is not a string is as good as none.
                json!({ "to": 5, "distance": { "less_than": 1.0 } })
            };
            plan.states[pick]
                .transitions
                .get_or_insert_with(Vec::new)
                .push(transition);
            vec![format!(
                "{tag} State '{picked}' has a transition missing a 'to' field."
            )]
        }
    }
}

#[test]
fn validator_reports_exactly_the_message_of_each_single_mutation() {
    let mut rng = Lcg::new(SEED ^ 0xA5A5);
    for case in 0..CASES {
        let spec = generate_spec(&mut rng, case);
        let mut plan = build_plan(&spec, case);
        let mutation = MUTATIONS[case % MUTATIONS.len()];
        let expected = mutate(&mut plan, mutation, &mut rng, case);
        let ctx = format!(
            "seed {:#x} case {case} mutation {mutation:?}: {spec:?}",
            SEED ^ 0xA5A5
        );

        assert_eq!(validate_capture_plan(PLAN_ID, &plan), expected, "{ctx}");

        // A plan the validator rejects still compiles, state for state, and only an
        // empty plan has no capture component.
        let mut library = CapturePlanLibrary::default();
        library.insert_plan(PLAN_ID.to_string(), plan.clone());
        assert_eq!(
            library.compiled_plans[PLAN_ID].states.len(),
            plan.states.len(),
            "{ctx}"
        );
        assert_eq!(
            build_capture_component(PLAN_ID, &plan, 0.0).is_none(),
            plan.states.is_empty(),
            "{ctx}"
        );
    }
}

#[test]
fn validator_reports_independent_errors_in_field_order() {
    let mut rng = Lcg::new(SEED ^ 0x5A5A);
    for case in 0..CASES {
        let spec = generate_spec(&mut rng, case);
        let mut plan = build_plan(&spec, case);
        let ctx = format!("seed {:#x} case {case}: {spec:?}", SEED ^ 0x5A5A);

        // name, tether, device length: always reported in that order, before any
        // state error.
        let mut expected = Vec::new();
        expected.extend(mutate(&mut plan, Mutation::BlankName, &mut rng, case));
        expected.extend(mutate(&mut plan, Mutation::BlankTether, &mut rng, case));
        expected.extend(mutate(
            &mut plan,
            Mutation::NonPositiveTetherLength,
            &mut rng,
            case,
        ));
        expected.extend(mutate(&mut plan, Mutation::NoParameters, &mut rng, case));
        let picked = plan
            .states
            .iter()
            .position(|state| state.parameters.is_none())
            .expect("a state lost its parameters");
        assert_eq!(
            expected.len(),
            5,
            "three field errors and two parameter errors: {ctx}"
        );
        // The parameter errors name the state that lost them.
        assert!(
            expected[3].contains(&format!("'{}'", state_id(picked))),
            "{ctx}"
        );

        assert_eq!(validate_capture_plan(PLAN_ID, &plan), expected, "{ctx}");
    }
}
