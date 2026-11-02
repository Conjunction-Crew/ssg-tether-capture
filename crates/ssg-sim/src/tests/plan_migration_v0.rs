//! v0 → v1 plan migration (design D4): every P0 fixture's outcome, losslessness
//! (`reconstruct_v0(migrate(v0)) == v0`) and "same simulation" (the v1 compile of
//! the migrated plan equals the v0 compile, the oracle, minus the transitions
//! that never fire), on the fixtures and on generated v0 plans.

use std::path::PathBuf;

use serde_json::{Map, Value, json};

use crate::capture::plan::{CapturePlan, CaptureSphere, CompiledCapturePlan, compile_capture_plan};
use crate::project::load_plans_from_dir_with_errors;
use crate::project::plan_files::{plan_file_reports, plan_to_json, read_plan_value};
use crate::test_support::{Lcg, repo_root};
use crate::tests::plan_v0_oracle::{CapturePlanV0, compile_v0};

const SEED: u64 = 0x0DD5_EED5;
const CASES: usize = 300;

fn fixtures_dir() -> PathBuf {
    repo_root().join("crates/ssg-sim/tests/fixtures/v0")
}

fn fixture(name: &str) -> Value {
    let raw = std::fs::read_to_string(fixtures_dir().join(name)).unwrap();
    serde_json::from_str(&raw).unwrap()
}

// ---------------------------------------------------------------------------
// Helpers: reconstruct_v0, JSON equality with numbers as f64, compiled equality
// ---------------------------------------------------------------------------

/// The reference tokens of a JSON Pointer (RFC 6901), unescaped.
fn tokens(pointer: &str) -> Vec<String> {
    pointer
        .split('/')
        .skip(1)
        .map(|t| t.replace("~1", "/").replace("~0", "~"))
        .collect()
}

fn number(value: f64) -> Value {
    json!(value)
}

/// The v0 document a migrated plan came from: the mapped fields back under their
/// v0 names, then the removed transitions re-inserted at their v0 index, then
/// every other unmapped value set at its pointer. Test code only (D4).
fn reconstruct_v0(plan: &CapturePlan) -> Value {
    let mut doc = Map::new();
    doc.insert("name".into(), json!(plan.name));
    doc.insert("tether".into(), json!(plan.tether));
    if let Some(device) = &plan.device {
        let mut d = Map::new();
        if let Some(t) = &device.device_type {
            d.insert("type".into(), json!(t));
        }
        if let Some(l) = device.tether_length_m {
            d.insert("tether_length".into(), number(l));
        }
        doc.insert("device".into(), Value::Object(d));
    }
    let bounds = |b: &crate::capture::plan::ConditionBounds| {
        let mut o = Map::new();
        if let Some(v) = b.less_than {
            o.insert("less_than".into(), number(v));
        }
        if let Some(v) = b.greater_than {
            o.insert("greater_than".into(), number(v));
        }
        Value::Object(o)
    };
    let states: Vec<Value> = plan
        .phases
        .iter()
        .map(|phase| {
            let mut parameters = Map::new();
            parameters.insert("max_velocity".into(), number(phase.max_velocity_mps));
            parameters.insert("max_force".into(), number(phase.max_force_n));
            if let Some(rate) = phase.shrink_rate_mps {
                parameters.insert("shrink_rate".into(), number(rate));
            }
            let mut state = Map::new();
            state.insert("id".into(), json!(phase.id));
            state.insert("parameters".into(), Value::Object(parameters));
            if let Some(transitions) = &phase.transitions {
                let list = transitions
                    .iter()
                    .map(|t| {
                        let mut o = Map::new();
                        o.insert("to".into(), json!(t.to));
                        if let Some(d) = &t.all_of.distance_m {
                            o.insert("distance".into(), bounds(d));
                        }
                        if let Some(v) = &t.all_of.relative_speed_mps {
                            o.insert("relative_velocity".into(), bounds(v));
                        }
                        Value::Object(o)
                    })
                    .collect();
                state.insert("transitions".into(), Value::Array(list));
            }
            Value::Object(state)
        })
        .collect();
    doc.insert("states".into(), Value::Array(states));
    let mut doc = Value::Object(doc);

    let unmapped = plan
        .migrated_from_v0
        .as_ref()
        .map(|m| m.unmapped.clone())
        .unwrap_or_default();
    let is_removed_transition = |t: &[String]| {
        t.len() == 4 && t[0] == "states" && t[2] == "transitions" && t[3].parse::<usize>().is_ok()
    };
    for entry in unmapped
        .iter()
        .filter(|e| is_removed_transition(&tokens(&e.pointer)))
    {
        let t = tokens(&entry.pointer);
        let i: usize = t[1].parse().unwrap();
        let j: usize = t[3].parse().unwrap();
        doc["states"][i]["transitions"]
            .as_array_mut()
            .expect("a removed transition's array is kept")
            .insert(j, entry.value.clone());
    }
    for entry in unmapped
        .iter()
        .filter(|e| !is_removed_transition(&tokens(&e.pointer)))
    {
        let t = tokens(&entry.pointer);
        let (last, parents) = t.split_last().expect("not the root");
        let mut target = &mut doc;
        for token in parents {
            target = match target {
                Value::Array(a) => &mut a[token.parse::<usize>().unwrap()],
                Value::Object(o) => o.entry(token.clone()).or_insert_with(|| json!({})),
                _ => panic!("pointer {} runs through a scalar", entry.pointer),
            };
        }
        target
            .as_object_mut()
            .unwrap_or_else(|| panic!("pointer {} has no object parent", entry.pointer))
            .insert(last.clone(), entry.value.clone());
    }
    doc
}

/// JSON equality with every number compared as `f64` and key order ignored.
fn same_json(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(x, y)| same_json(x, y))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .all(|(k, v)| y.get(k).is_some_and(|w| same_json(v, w)))
        }
        _ => a == b,
    }
}

/// The v0 oracle's compile with the transitions that never fire removed (no
/// numeric bound: `capture/mod.rs`, "one without a condition never fires").
fn oracle_compile(v0: &Value) -> CompiledCapturePlan {
    let plan: CapturePlanV0 = serde_json::from_value(v0.clone()).expect("v0 parses");
    let mut compiled = compile_v0(&plan);
    for state in &mut compiled.states {
        state.transitions.retain(|t| {
            t.distance_less_than.is_some()
                || t.distance_greater_than.is_some()
                || t.relative_velocity_less_than.is_some()
                || t.relative_velocity_greater_than.is_some()
        });
    }
    compiled
}

/// Field-by-field (bit-exact via `Debug`) equality of two compiled plans.
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

/// Migrates `v0`, checks it loads, is lossless (also after the v1 writer and
/// reader) and compiles like the oracle. Returns the number of transitions the
/// migration removed.
fn check_migration(id: &str, v0: &Value, ctx: &str) -> usize {
    let report = read_plan_value(id, v0);
    assert_eq!(report.errors, Vec::<String>::new(), "{ctx}: loads");
    let plan = report.plan.expect("loads");
    assert_eq!(plan.id, id, "{ctx}");

    let back = reconstruct_v0(&plan);
    assert!(
        same_json(&back, v0),
        "{ctx}: lossless\nv0:   {v0}\nback: {back}"
    );
    let reread = read_plan_value(id, &serde_json::from_str(&plan_to_json(&plan)).unwrap())
        .plan
        .expect("the written v1 file loads");
    assert_eq!(reread, plan, "{ctx}: v1 write/read");
    assert!(
        same_json(&reconstruct_v0(&reread), v0),
        "{ctx}: lossless after save"
    );

    assert_same_compiled(&compile_capture_plan(&plan), &oracle_compile(v0), ctx);
    let compiled_v0 = compile_v0(&serde_json::from_value(v0.clone()).unwrap());
    let before: usize = compiled_v0.states.iter().map(|s| s.transitions.len()).sum();
    let after: usize = compile_capture_plan(&plan)
        .states
        .iter()
        .map(|s| s.transitions.len())
        .sum();
    before - after
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// Every fixture's outcome: `Ok(warnings)` when it loads, `Err(errors)` when it
/// is refused. Before P6 the same files loaded / were refused the same way,
/// except `r09_negative_force`, which loaded (F-32 range rule, D4).
fn expected_outcomes() -> Vec<(&'static str, Result<usize, Vec<&'static str>>)> {
    vec![
        ("example_plan", Ok(1)),
        ("f15_roundtrip", Ok(1)),
        ("q10_and_condition", Ok(0)),
        ("q10_custom_states", Ok(0)),
        ("q10_ignored_fields", Ok(10)),
        ("q10_rel_velocity", Ok(0)),
        (
            "r01_bad_syntax",
            Err(vec![
                "Invalid JSON: EOF while parsing a list at line 2 column 0",
            ]),
        ),
        (
            "r02_missing_states",
            Err(vec!["[r02_missing_states] /states: missing"]),
        ),
        (
            "r03_empty_states",
            Err(vec![
                "[r03_empty_states] /phases: at least one phase is required",
            ]),
        ),
        (
            "r04_unknown_transition",
            Err(vec![
                "[r04_unknown_transition] /phases/0/transitions/0/to: unknown phase 'nowhere'",
            ]),
        ),
        (
            "r05_duplicate_state",
            Err(vec![
                "[r05_duplicate_state] /phases/1/id: duplicate phase id 'approach'",
            ]),
        ),
        (
            "r06_missing_max_force",
            Err(vec![
                "[r06_missing_max_force] /states/0/parameters/max_force: missing",
            ]),
        ),
        (
            "r07_string_number",
            Err(vec![
                "[r07_string_number] /states/0/parameters/max_velocity: must be a number",
            ]),
        ),
        (
            "r08_zero_tether",
            Err(vec![
                "[r08_zero_tether] /device/tether_length_m: must be greater than 0",
            ]),
        ),
        (
            "r09_negative_force",
            Err(vec![
                "[r09_negative_force] /phases/0/max_force_n: must not be negative",
                "[r09_negative_force] /phases/1/max_force_n: must not be negative",
            ]),
        ),
        (
            "r11_empty",
            Err(vec![
                "Invalid JSON: EOF while parsing a value at line 1 column 0",
            ]),
        ),
        (
            "r12_missing_name",
            Err(vec!["[r12_missing_name] /name: missing"]),
        ),
        (
            "r13_blank_name",
            Err(vec!["[r13_blank_name] /name: must not be blank"]),
        ),
        (
            "r14_transition_missing_to",
            Err(vec![
                "[r14_transition_missing_to] /states/0/transitions/0/to: missing",
            ]),
        ),
    ]
}

#[test]
fn every_v0_fixture_has_its_expected_outcome() {
    let reports = plan_file_reports(&fixtures_dir());
    let stems: Vec<&str> = reports.iter().map(|(stem, _)| stem.as_str()).collect();
    let expected = expected_outcomes();
    assert_eq!(
        stems,
        expected.iter().map(|(stem, _)| *stem).collect::<Vec<_>>(),
        "every .json fixture is reported once, in file order (r10_notes.txt is not a plan)"
    );
    for ((stem, report), (_, outcome)) in reports.iter().zip(&expected) {
        match outcome {
            Ok(warnings) => {
                assert_eq!(report.errors, Vec::<String>::new(), "{stem}");
                assert!(report.plan.is_some(), "{stem} loads");
                assert_eq!(
                    report.warnings.len(),
                    *warnings,
                    "{stem}: {:?}",
                    report.warnings
                );
            }
            Err(errors) => {
                assert_eq!(&report.errors, errors, "{stem}");
                assert!(report.plan.is_none(), "{stem} is refused");
            }
        }
    }
    // The directory loader agrees.
    let (plans, errors) = load_plans_from_dir_with_errors(&fixtures_dir());
    assert_eq!(plans.len(), 6);
    assert_eq!(errors.len(), 13);
}

#[test]
fn every_loading_v0_fixture_migrates_losslessly_and_compiles_like_v0() {
    let mut removed = Vec::new();
    for (stem, outcome) in expected_outcomes() {
        if outcome.is_err() {
            continue;
        }
        let v0 = fixture(&format!("{stem}.json"));
        removed.push((stem, check_migration(stem, &v0, stem)));
    }
    // Only q10_ignored_fields has transitions without a numeric bound (its
    // `time_in_state` and `closing_rate` conditions): both are removed.
    assert_eq!(
        removed,
        [
            ("example_plan", 0),
            ("f15_roundtrip", 0),
            ("q10_and_condition", 0),
            ("q10_custom_states", 0),
            ("q10_ignored_fields", 2),
            ("q10_rel_velocity", 0),
        ]
    );
}

#[test]
fn ignored_v0_values_are_kept_with_a_warning_naming_the_v1_field() {
    let report = read_plan_value("q10_ignored_fields", &fixture("q10_ignored_fields.json"));
    let kept = "not used by v0 or v1; kept under migrated_from_v0.unmapped";
    let tag = "[q10_ignored_fields]";
    assert_eq!(
        report.warnings,
        [
            format!("{tag} /chaser: {kept}; set it in the project manifest"),
            format!(
                "{tag} /device/damping: {kept}; set device.joint_damping_linear and \
                 joint_damping_angular instead"
            ),
            format!("{tag} /device/segment_mass: {kept}; set device.segment_mass_kg instead"),
            format!("{tag} /device/segments: {kept}; set device.segment_spacing_m instead"),
            format!("{tag} /start_epoch: {kept}; set it in the project manifest"),
            format!("{tag} /states/0/parameters/kp: {kept}"),
            format!(
                "{tag} /states/0/transitions/0: never fires in v0 (no numeric distance or \
                 relative_velocity bound); removed from the transitions and kept under \
                 migrated_from_v0.unmapped"
            ),
            format!(
                "{tag} /states/0/transitions/1: never fires in v0 (no numeric distance or \
                 relative_velocity bound); removed from the transitions and kept under \
                 migrated_from_v0.unmapped"
            ),
            format!(
                "{tag} /states/1/transitions/0/distance/units: {kept}; v1 values are always \
                 metric (m, m/s, N)"
            ),
            format!("{tag} /target: {kept}; set it in the project manifest"),
        ]
    );
    let plan = report.plan.unwrap();
    // A.3: v0 ignored these, so v1 does not apply them: the device keeps only
    // what v0 read, and "units": "ft" leaves the 50.0 as metres.
    let device = plan.device.as_ref().unwrap();
    assert_eq!(device.segment_mass_kg, None);
    assert_eq!(device.segment_spacing_m, None);
    assert_eq!(device.joint_damping_linear, None);
    assert_eq!(
        plan.phases[1].transitions.as_ref().unwrap()[0]
            .all_of
            .distance_m
            .unwrap()
            .greater_than,
        Some(50.0)
    );
    assert_eq!(plan.phases[0].transitions.as_deref(), Some(&[][..]));
}

#[test]
fn the_v1_example_plan_compiles_like_the_v0_original() {
    let v0 = fixture("example_plan.json");
    let (plans, errors) =
        load_plans_from_dir_with_errors(&repo_root().join("assets/example_capture_plans"));
    assert!(errors.is_empty(), "{errors:?}");
    let v1 = &plans["example_plan"];
    assert_eq!(v1.schema_version, 1);
    // P7.5 S6 (E8, E9, E13b): the v1 example differs from the v0 original in the
    // five values the O2 revision set, and in nothing else: with them reverted it
    // compiles like the v0 file.
    let mut as_v0 = v1.clone();
    assert_eq!(as_v0.phases[0].max_velocity_mps, 18.0);
    as_v0.phases[0].max_velocity_mps = 25.0;
    assert_eq!(as_v0.phases[2].tail_actuation, Some(true));
    as_v0.phases[2].tail_actuation = None;
    assert_eq!(as_v0.phases[2].velocity_scales_with_sphere, Some(true));
    as_v0.phases[2].velocity_scales_with_sphere = None;
    assert_eq!(as_v0.phases[3].tail_actuation, Some(true));
    as_v0.phases[3].tail_actuation = None;
    assert_eq!(
        as_v0.capture_sphere,
        Some(CaptureSphere {
            initial_radius_m: None,
            min_radius_m: Some(4.5),
        })
    );
    as_v0.capture_sphere = None;
    assert_same_compiled(
        &compile_capture_plan(&as_v0),
        &oracle_compile(&v0),
        "example_plan",
    );
    assert_eq!(compile_capture_plan(v1).min_capture_radius_m, 4.5);
    assert_eq!(oracle_compile(&v0).min_capture_radius_m, 0.1);
    // And the migrated v0 file is the v1 file apart from what v1 adds or drops:
    // the `end` marker and the unmapped `units`.
    let migrated = read_plan_value("example_plan", &v0).plan.unwrap();
    let mut expected = as_v0.clone();
    expected.phases[3].end = None;
    expected.migrated_from_v0 = migrated.migrated_from_v0.clone();
    assert_eq!(migrated, expected);
}

// ---------------------------------------------------------------------------
// Generated v0 plans
// ---------------------------------------------------------------------------

/// `x` as a file holds it: at most 15 significant digits. serde_json (without its
/// `float_roundtrip` feature) reads some 16- and 17-digit numbers one ULP off, so
/// only such numbers survive a write and a read bit for bit (Run 2 finding).
fn short(x: f64) -> f64 {
    format!("{x:.6e}").parse().unwrap()
}

/// A random JSON value (an extra key's value).
fn any_value(rng: &mut Lcg, depth: usize) -> Value {
    match rng.below(if depth == 0 { 4 } else { 6 }) {
        0 => Value::Null,
        1 => json!(rng.chance(0.5)),
        2 => json!(short(rng.range(-1e6, 1e6))),
        3 => json!(["m", "ft", "s", "", "x/y~z"][rng.below(5)]),
        4 => {
            let n = rng.below(3);
            Value::Array((0..n).map(|_| any_value(rng, depth - 1)).collect())
        }
        _ => {
            let mut o = Map::new();
            for k in 0..rng.below(3) {
                o.insert(format!("k{k}"), any_value(rng, depth - 1));
            }
            Value::Object(o)
        }
    }
}

/// Adds 0–2 random extra keys to `object`, never one of `mapped`.
fn extra_keys(rng: &mut Lcg, object: &mut Map<String, Value>, mapped: &[&str]) {
    const NAMES: [&str; 8] = [
        "units", "kp", "next", "a/b", "t~1", "notes", "x", "segments",
    ];
    for _ in 0..rng.below(3) {
        let name = NAMES[rng.below(NAMES.len())];
        if !mapped.contains(&name) {
            object.insert(name.to_string(), any_value(rng, 2));
        }
    }
}

/// A non-negative number, sometimes an integer, as a v0 file may hold it.
fn amount(rng: &mut Lcg, hi: f64) -> Value {
    if rng.chance(0.2) {
        json!(rng.below(hi as usize + 1) as u64)
    } else {
        json!(short(rng.range(0.0, hi)))
    }
}

/// A v0 condition object: numeric bounds or not, maybe non-numeric ones, extras.
fn condition(rng: &mut Lcg, hi: f64) -> Value {
    let mut o = Map::new();
    for bound in ["less_than", "greater_than"] {
        match rng.below(4) {
            0 | 1 => {
                o.insert(bound.into(), amount(rng, hi));
            }
            2 => {
                o.insert(bound.into(), json!("10"));
            }
            _ => {}
        }
    }
    extra_keys(rng, &mut o, &["less_than", "greater_than"]);
    Value::Object(o)
}

fn generate_v0(rng: &mut Lcg, case: usize) -> Value {
    let state_count = 1 + rng.below(6);
    let id = |i: usize| format!("s{i}");
    let mut doc = Map::new();
    doc.insert("name".into(), json!(format!("Generated v0 plan {case}")));
    doc.insert("tether".into(), json!(format!("Tether{}", case % 3)));
    match rng.below(4) {
        0 => {}
        1 => {
            doc.insert("device".into(), Value::Null);
        }
        _ => {
            let mut d = Map::new();
            if rng.chance(0.7) {
                d.insert("type".into(), json!("tether"));
            }
            d.insert("tether_length".into(), json!(short(rng.range(0.5, 100.0))));
            extra_keys(rng, &mut d, &["type", "tether_length"]);
            doc.insert("device".into(), Value::Object(d));
        }
    }
    let states: Vec<Value> = (0..state_count)
        .map(|i| {
            let mut parameters = Map::new();
            parameters.insert("max_velocity".into(), amount(rng, 50.0));
            parameters.insert("max_force".into(), amount(rng, 50.0));
            match rng.below(4) {
                0 => {
                    parameters.insert("shrink_rate".into(), amount(rng, 0.1));
                }
                1 => {
                    // Not a number: v0 read no shrink rate, so it is unmapped.
                    let value = any_value(rng, 1);
                    let value = if value.is_number() {
                        json!("fast")
                    } else {
                        value
                    };
                    parameters.insert("shrink_rate".into(), value);
                }
                _ => {}
            }
            extra_keys(
                rng,
                &mut parameters,
                &["max_velocity", "max_force", "shrink_rate"],
            );
            let mut state = Map::new();
            state.insert("id".into(), json!(id(i)));
            state.insert("parameters".into(), Value::Object(parameters));
            match rng.below(5) {
                0 => {}
                1 => {
                    state.insert("transitions".into(), Value::Null);
                }
                _ => {
                    let transitions = (0..rng.below(4))
                        .map(|_| {
                            let mut t = Map::new();
                            t.insert("to".into(), json!(id(rng.below(state_count))));
                            if rng.chance(0.6) {
                                t.insert("distance".into(), condition(rng, 5_000.0));
                            }
                            if rng.chance(0.4) {
                                t.insert("relative_velocity".into(), condition(rng, 50.0));
                            }
                            if rng.chance(0.1) {
                                t.insert("distance".into(), json!(5));
                            }
                            extra_keys(rng, &mut t, &["to", "distance", "relative_velocity"]);
                            Value::Object(t)
                        })
                        .collect();
                    state.insert("transitions".into(), Value::Array(transitions));
                }
            }
            if rng.chance(0.3) {
                state.insert("next".into(), json!(id(rng.below(state_count))));
            }
            if rng.chance(0.2) {
                state.insert("next_conditions".into(), any_value(rng, 2));
            }
            extra_keys(
                rng,
                &mut state,
                &["id", "parameters", "transitions", "next"],
            );
            Value::Object(state)
        })
        .collect();
    doc.insert("states".into(), Value::Array(states));
    extra_keys(rng, &mut doc, &["name", "tether", "device", "states"]);
    Value::Object(doc)
}

#[test]
fn generated_v0_plans_migrate_losslessly_and_compile_like_v0() {
    let mut rng = Lcg::new(SEED);
    let (mut removed, mut with_unmapped) = (0, 0);
    for case in 0..CASES {
        let v0 = generate_v0(&mut rng, case);
        let ctx = format!("seed {SEED:#x} case {case}: {v0}");
        removed += check_migration("generated", &v0, &ctx);
        let plan = read_plan_value("generated", &v0).plan.unwrap();
        with_unmapped += usize::from(plan.migrated_from_v0.is_some());
    }
    println!(
        "seed {SEED:#x}: {CASES} generated v0 plans, lossless and compile-equal; \
         {removed} never-firing transitions removed; {with_unmapped} plans with unmapped values"
    );
    // The generator exercises what the properties are about.
    assert!(removed > 50, "{removed}");
    assert!(with_unmapped > 200, "{with_unmapped}");
}

/// Negative control: the oracle comparison sees a one-ULP change of one limit, so
/// the equivalence above cannot pass by comparing a plan with itself.
#[test]
#[should_panic(expected = "parameters of terminal")]
fn the_oracle_comparison_catches_a_changed_limit() {
    let v0 = fixture("example_plan.json");
    let mut plan = read_plan_value("example_plan", &v0).plan.unwrap();
    plan.phases[2].max_force_n = f64::from_bits(plan.phases[2].max_force_n.to_bits() + 1);
    assert_same_compiled(
        &compile_capture_plan(&plan),
        &oracle_compile(&v0),
        "changed",
    );
}
