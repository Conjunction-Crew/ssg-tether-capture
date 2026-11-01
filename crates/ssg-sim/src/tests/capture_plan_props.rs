//! Property-style tests for capture-plan validation and compilation (P5 S4, A-4).
//!
//! Deterministic generated inputs: a fixed-seed [`Lcg`] builds a few hundred random
//! valid plans; the first property says each validates, compiles and starts a
//! capture as its spec says; the second applies one mutation from a fixed list to
//! a valid plan and expects exactly the message reading it gives. A failure names
//! the seed, the case and the mutation, so it reproduces from them.
//!
//! F-15 (P6): the plans are the typed v1 model and the mutations are applied to
//! the v1 document, so a mutation the v1 type cannot hold (a missing limit, a
//! transition without `to`) is checked as the reader's error.

use serde::Deserialize;
use serde_json::{Value, json};

use crate::capture::plan::{
    CapturePlan, CapturePlanDevice, CapturePlanLibrary, ConditionBounds, Phase, Transition,
    TransitionConditions, build_capture_component, validate_capture_plan,
};
use crate::project::plan_files::read_plan_value;
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
                .map(|_| {
                    let mut transition = TransitionSpec {
                        to: rng.below(state_count),
                        distance_less_than: maybe(rng, 0.1, 5_000.0),
                        distance_greater_than: maybe(rng, 0.1, 5_000.0),
                        relative_velocity_less_than: maybe(rng, 0.01, 50.0),
                        relative_velocity_greater_than: maybe(rng, 0.01, 50.0),
                    };
                    // F-15 (P6): a v1 transition has at least one condition.
                    if transition.distance_less_than.is_none()
                        && transition.distance_greater_than.is_none()
                        && transition.relative_velocity_less_than.is_none()
                        && transition.relative_velocity_greater_than.is_none()
                    {
                        transition.distance_less_than = Some(rng.range(0.1, 5_000.0));
                    }
                    transition
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

fn bounds(less_than: Option<f64>, greater_than: Option<f64>) -> Option<ConditionBounds> {
    (less_than.is_some() || greater_than.is_some()).then_some(ConditionBounds {
        less_than,
        greater_than,
    })
}

fn transition(transition: &TransitionSpec) -> Transition {
    Transition {
        to: state_id(transition.to),
        all_of: TransitionConditions {
            distance_m: bounds(
                transition.distance_less_than,
                transition.distance_greater_than,
            ),
            relative_speed_mps: bounds(
                transition.relative_velocity_less_than,
                transition.relative_velocity_greater_than,
            ),
        },
    }
}

fn build_plan(spec: &PlanSpec, case: usize) -> CapturePlan {
    CapturePlan {
        schema_version: 1,
        name: format!("Generated plan {case}"),
        id: String::new(),
        tether: spec.tether.clone(),
        device: spec.tether_length.map(|tether_length| CapturePlanDevice {
            device_type: Some("tether".to_string()),
            tether_length_m: Some(tether_length),
            ..CapturePlanDevice::default()
        }),
        capture_sphere: None,
        phases: spec
            .states
            .iter()
            .enumerate()
            .map(|(index, state)| Phase {
                id: state_id(index),
                max_velocity_mps: state.max_velocity,
                max_force_n: state.max_force,
                shrink_rate_mps: state.shrink_rate,
                transitions: (!state.transitions.is_empty())
                    .then(|| state.transitions.iter().map(transition).collect()),
                end: None,
                tail_actuation: None,
                velocity_scales_with_sphere: None,
            })
            .collect(),
        migrated_from_v0: None,
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

/// Applies `mutation` to the valid v1 document `doc` and returns the messages
/// reading it gives (in their order).
fn mutate(doc: &mut Value, mutation: Mutation, rng: &mut Lcg, case: usize) -> Vec<String> {
    let phases = doc["phases"].as_array_mut().expect("phases");
    let pick = rng.below(phases.len());
    let picked = phases[pick]["id"].as_str().expect("id").to_string();
    let next = phases.len();
    let tag = format!("[{PLAN_ID}]");
    let push_transition = |doc: &mut Value, transition: Value| -> usize {
        let phase = &mut doc["phases"][pick];
        if phase.get("transitions").is_none() {
            phase["transitions"] = json!([]);
        }
        let transitions = phase["transitions"].as_array_mut().expect("transitions");
        transitions.push(transition);
        transitions.len() - 1
    };
    match mutation {
        Mutation::BlankName => {
            doc["name"] = json!(blank(rng));
            vec![format!("{tag} /name: must not be blank")]
        }
        Mutation::BlankTether => {
            doc["tether"] = json!(blank(rng));
            vec![format!("{tag} /tether: must not be blank")]
        }
        Mutation::NonPositiveTetherLength => {
            let tether_length = if rng.chance(0.5) {
                0.0
            } else {
                -rng.range(0.001, 100.0)
            };
            doc["device"] = json!({ "type": "tether", "tether_length_m": tether_length });
            vec![format!(
                "{tag} /device/tether_length_m: must be greater than 0"
            )]
        }
        Mutation::NoStates => {
            doc["phases"] = json!([]);
            vec![format!("{tag} /phases: at least one phase is required")]
        }
        Mutation::DuplicateStateId => {
            let copy = doc["phases"][pick].clone();
            doc["phases"].as_array_mut().unwrap().push(copy);
            vec![format!(
                "{tag} /phases/{next}/id: duplicate phase id '{picked}'"
            )]
        }
        Mutation::EmptyStateId => {
            let mut extra = doc["phases"][pick].clone();
            extra["id"] = json!(blank(rng));
            doc["phases"].as_array_mut().unwrap().push(extra);
            vec![format!("{tag} /phases/{next}/id: must not be blank")]
        }
        Mutation::MissingMaxVelocity | Mutation::MissingMaxForce => {
            let (key, other) = match mutation {
                Mutation::MissingMaxVelocity => ("max_velocity_mps", "max_force_n"),
                _ => ("max_force_n", "max_velocity_mps"),
            };
            let phase = &mut doc["phases"][pick];
            // Absent, or present but not a number (F-32: no longer read as "missing").
            let message = if rng.chance(0.5) {
                phase.as_object_mut().expect("object").remove(key);
                format!("missing field `{key}`")
            } else {
                phase[key] = json!("fast");
                "invalid type: string \"fast\", expected f64".to_string()
            };
            assert!(phase.get(other).is_some_and(Value::is_number));
            vec![format!("{tag} /phases/{pick}: {message}")]
        }
        Mutation::NoParameters => {
            let phase = doc["phases"][pick].as_object_mut().expect("object");
            phase.remove("max_velocity_mps");
            phase.remove("max_force_n");
            // serde names the first missing field of the phase.
            vec![format!(
                "{tag} /phases/{pick}: missing field `max_velocity_mps`"
            )]
        }
        Mutation::TransitionToUnknownState => {
            let unknown = format!("nowhere_{case}");
            let index = push_transition(
                doc,
                json!({ "to": unknown, "all_of": { "distance_m": { "less_than": 1.0 } } }),
            );
            vec![format!(
                "{tag} /phases/{pick}/transitions/{index}/to: unknown phase '{unknown}'"
            )]
        }
        Mutation::TransitionWithoutTo => {
            let (transition, message) = if rng.chance(0.5) {
                (
                    json!({ "all_of": { "distance_m": { "less_than": 1.0 } } }),
                    "missing field `to`",
                )
            } else {
                (
                    json!({ "to": 5, "all_of": { "distance_m": { "less_than": 1.0 } } }),
                    "invalid type: integer `5`, expected a string",
                )
            };
            let index = push_transition(doc, transition);
            vec![format!(
                "{tag} /phases/{pick}/transitions/{index}: {message}"
            )]
        }
    }
}

/// The mutations the v1 type can hold (the validator reports them); the others
/// are refused when the document is read.
fn validator_level(mutation: Mutation) -> bool {
    !matches!(
        mutation,
        Mutation::MissingMaxVelocity
            | Mutation::MissingMaxForce
            | Mutation::NoParameters
            | Mutation::TransitionWithoutTo
    )
}

#[test]
fn validator_reports_exactly_the_message_of_each_single_mutation() {
    let mut rng = Lcg::new(SEED ^ 0xA5A5);
    for case in 0..CASES {
        let spec = generate_spec(&mut rng, case);
        let mut doc = serde_json::to_value(build_plan(&spec, case)).unwrap();
        let mutation = MUTATIONS[case % MUTATIONS.len()];
        let expected = mutate(&mut doc, mutation, &mut rng, case);
        let ctx = format!(
            "seed {:#x} case {case} mutation {mutation:?}: {spec:?}",
            SEED ^ 0xA5A5
        );

        let report = read_plan_value(PLAN_ID, &doc);
        assert_eq!(report.errors, expected, "{ctx}");
        assert!(report.plan.is_none(), "{ctx}");

        let parsed = CapturePlan::deserialize(&doc);
        assert_eq!(parsed.is_ok(), validator_level(mutation), "{ctx}");
        let Ok(plan) = parsed else { continue };
        assert_eq!(validate_capture_plan(PLAN_ID, &plan), expected, "{ctx}");

        // A plan the validator rejects still compiles, phase for phase, and only an
        // empty plan has no capture component.
        let mut library = CapturePlanLibrary::default();
        library.insert_plan(PLAN_ID.to_string(), plan.clone());
        assert_eq!(
            library.compiled_plans[PLAN_ID].states.len(),
            plan.phases.len(),
            "{ctx}"
        );
        assert_eq!(
            build_capture_component(PLAN_ID, &plan, 0.0).is_none(),
            plan.phases.is_empty(),
            "{ctx}"
        );
    }
}

#[test]
fn validator_reports_independent_errors_in_field_order() {
    let mut rng = Lcg::new(SEED ^ 0x5A5A);
    for case in 0..CASES {
        let spec = generate_spec(&mut rng, case);
        let mut doc = serde_json::to_value(build_plan(&spec, case)).unwrap();
        let ctx = format!("seed {:#x} case {case}: {spec:?}", SEED ^ 0x5A5A);

        // name, tether, device length: always reported in that order, before any
        // phase error.
        let mut expected = Vec::new();
        expected.extend(mutate(&mut doc, Mutation::BlankName, &mut rng, case));
        expected.extend(mutate(&mut doc, Mutation::BlankTether, &mut rng, case));
        expected.extend(mutate(
            &mut doc,
            Mutation::NonPositiveTetherLength,
            &mut rng,
            case,
        ));
        // F-15 (P6): v1 phases always have both limits, so the two phase errors
        // are two negative limits of one phase (F-32 range rule).
        let picked = rng.below(spec.states.len());
        doc["phases"][picked]["max_velocity_mps"] = json!(-rng.range(0.001, 10.0));
        doc["phases"][picked]["max_force_n"] = json!(-rng.range(0.001, 10.0));
        for field in ["max_velocity_mps", "max_force_n"] {
            expected.push(format!(
                "[{PLAN_ID}] /phases/{picked}/{field}: must not be negative"
            ));
        }
        assert_eq!(
            expected.len(),
            5,
            "three field errors and two parameter errors: {ctx}"
        );
        // The parameter errors name the phase that broke them.
        assert!(expected[3].contains(&format!("/phases/{picked}/")), "{ctx}");

        let plan = CapturePlan::deserialize(&doc).unwrap();
        assert_eq!(validate_capture_plan(PLAN_ID, &plan), expected, "{ctx}");
        assert_eq!(read_plan_value(PLAN_ID, &doc).errors, expected, "{ctx}");
    }
}
