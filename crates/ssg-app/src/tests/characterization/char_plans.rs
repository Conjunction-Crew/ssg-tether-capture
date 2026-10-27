//! Characterization of capture-plan loading, validation, compilation and the
//! serde round trip, for every plan in `assets/example_capture_plans`.

use std::path::PathBuf;

use serde_json::Value;

use crate::components::capture_components::CapturePlan;
use crate::resources::capture_plans::{
    CompiledCapturePlan, compile_capture_plan, load_plans_from_dir_with_errors,
    validate_capture_plan,
};
use crate::tests::test_support::repo_root;

fn example_plans_dir() -> PathBuf {
    repo_root().join("assets/example_capture_plans")
}

/// Every key path that differs between `file` (the plan as written) and `round`
/// (the plan after `to_value`). `+path` was added by serialization, `-path` was
/// removed, `~path` has a different value. Array elements are `[i]`.
fn key_diff(path: &str, file: &Value, round: &Value, out: &mut Vec<String>) {
    match (file, round) {
        (Value::Object(f), Value::Object(r)) => {
            for (key, fv) in f {
                let child = format!("{path}.{key}");
                match r.get(key) {
                    Some(rv) => key_diff(&child, fv, rv, out),
                    None => out.push(format!("-{child}")),
                }
            }
            for key in r.keys() {
                if !f.contains_key(key) {
                    out.push(format!("+{path}.{key}"));
                }
            }
        }
        (Value::Array(f), Value::Array(r)) => {
            if f.len() != r.len() {
                out.push(format!("~{path} (length {} vs {})", f.len(), r.len()));
            }
            for (i, (fv, rv)) in f.iter().zip(r).enumerate() {
                key_diff(&format!("{path}[{i}]"), fv, rv, out);
            }
        }
        _ => {
            if file != round {
                out.push(format!("~{path}"));
            }
        }
    }
}

/// Field-by-field equality of two compiled plans (`CompiledCapturePlan` has no
/// `PartialEq`).
fn assert_same_compiled(a: &CompiledCapturePlan, b: &CompiledCapturePlan, what: &str) {
    assert_eq!(a.tether, b.tether, "{what}: tether");
    assert_eq!(a.states.len(), b.states.len(), "{what}: state count");
    for (sa, sb) in a.states.iter().zip(&b.states) {
        assert_eq!(sa.id, sb.id, "{what}: state id");
        assert_eq!(
            format!("{:?}", sa.parameters),
            format!("{:?}", sb.parameters),
            "{what}: parameters of {}",
            sa.id
        );
        assert_eq!(
            format!("{:?}", sa.transitions),
            format!("{:?}", sb.transitions),
            "{what}: transitions of {}",
            sa.id
        );
    }
    let sorted = |p: &CompiledCapturePlan| {
        let mut v: Vec<(String, usize)> = p
            .state_indices
            .iter()
            .map(|(k, &i)| (k.clone(), i))
            .collect();
        v.sort();
        v
    };
    assert_eq!(sorted(a), sorted(b), "{what}: state_indices");
}

#[test]
fn char_plans_every_example_plan_round_trips() {
    let dir = example_plans_dir();
    let (plans, errors) = load_plans_from_dir_with_errors(&dir);
    assert!(
        errors.is_empty(),
        "example plans failed to load: {errors:?}"
    );

    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("example plan directory is readable")
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no example plans found in {dir:?}");
    assert!(
        files
            .iter()
            .any(|f| f.file_stem().unwrap() == "example_plan"),
        "example_plan.json is missing from {dir:?}"
    );
    assert_eq!(plans.len(), files.len());

    for file in &files {
        let stem = file.file_stem().unwrap().to_str().unwrap().to_string();
        let raw = std::fs::read_to_string(file).unwrap();
        let file_value: Value = serde_json::from_str(&raw).unwrap();
        let plan: CapturePlan = serde_json::from_str(&raw).unwrap();

        assert!(
            validate_capture_plan(&stem, &plan).is_empty(),
            "{stem}: validation errors"
        );
        let compiled = compile_capture_plan(&plan);
        assert!(
            !compiled.states.is_empty(),
            "{stem}: compiled plan is empty"
        );
        assert_eq!(compiled.states.len(), plan.states.len());

        // The loader returned the same plan.
        let loaded = plans.get(&stem).expect("loader returned the plan");
        assert_eq!(loaded.id, stem);
        assert_same_compiled(&compile_capture_plan(loaded), &compiled, &stem);

        // serde round trip: the re-parsed plan compiles to the same plan.
        let round_value = serde_json::to_value(&plan).unwrap();
        let reparsed: CapturePlan = serde_json::from_value(round_value.clone()).unwrap();
        assert_same_compiled(&compile_capture_plan(&reparsed), &compiled, &stem);
        assert_eq!(reparsed.name, plan.name);
        assert_eq!(reparsed.tether, plan.tether);
        assert_eq!(reparsed.states.len(), plan.states.len());

        // Pin the serde key differences between the file and its round trip.
        let mut diff = Vec::new();
        key_diff("", &file_value, &round_value, &mut diff);
        diff.sort();
        if stem == "example_plan" {
            // Observed: every state gains `next: null` and `next_conditions: null`,
            // and the last state (`capture`, which has no transitions in the file)
            // also gains `transitions: null`. Nothing is removed and no value changes.
            assert_eq!(
                diff,
                vec![
                    "+.states[0].next",
                    "+.states[0].next_conditions",
                    "+.states[1].next",
                    "+.states[1].next_conditions",
                    "+.states[2].next",
                    "+.states[2].next_conditions",
                    "+.states[3].next",
                    "+.states[3].next_conditions",
                    "+.states[3].transitions",
                ],
                "{stem}: serde key differences"
            );
        }
        assert!(
            diff.iter().all(|d| d.starts_with('+')),
            "{stem}: round trip removed or changed keys: {diff:?}"
        );
    }
}

#[test]
fn char_plans_example_compiled_shape() {
    let (plans, errors) = load_plans_from_dir_with_errors(&example_plans_dir());
    assert!(errors.is_empty(), "{errors:?}");
    let plan = plans.get("example_plan").expect("example_plan is loaded");
    let compiled = compile_capture_plan(plan);

    assert_eq!(compiled.tether, "Tether1");
    let ids: Vec<&str> = compiled.states.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, ["rendezvous", "approach", "terminal", "capture"]);
    for (i, id) in ids.iter().enumerate() {
        assert_eq!(compiled.state_indices[*id], i, "state index of {id}");
    }
    assert_eq!(compiled.state_indices.len(), 4);

    // (max_velocity, max_force, shrink_rate) per state.
    let params: Vec<(f64, f64, Option<f64>)> = compiled
        .states
        .iter()
        .map(|s| {
            (
                s.parameters.max_velocity,
                s.parameters.max_force,
                s.parameters.shrink_rate,
            )
        })
        .collect();
    assert_eq!(
        params,
        [
            (25.0, 25.0, None),
            (2.0, 3.0, None),
            (0.5, 2.0, Some(0.01)),
            (0.5, 2.0, Some(0.005)),
        ]
    );

    // (to, distance <, distance >, rel velocity <, rel velocity >) per transition.
    type Limits = (String, Option<f64>, Option<f64>, Option<f64>, Option<f64>);
    let transitions: Vec<Vec<Limits>> = compiled
        .states
        .iter()
        .map(|s| {
            s.transitions
                .iter()
                .map(|t| {
                    (
                        t.to.clone(),
                        t.distance_less_than,
                        t.distance_greater_than,
                        t.relative_velocity_less_than,
                        t.relative_velocity_greater_than,
                    )
                })
                .collect()
        })
        .collect();
    let limits = |to: &str, d_lt, d_gt| -> Limits { (to.to_string(), d_lt, d_gt, None, None) };
    // F-15: current behaviour, expected to change in P6
    // (`units: "m"` on the terminal -> capture transition is ignored: only
    // `distance_less_than` is set.)
    assert_eq!(
        transitions,
        vec![
            vec![limits("approach", Some(2000.0), None)],
            vec![limits("terminal", Some(50.0), None)],
            vec![
                limits("capture", Some(5.0), None),
                limits("approach", None, Some(50.0)),
            ],
            vec![],
        ]
    );
}
