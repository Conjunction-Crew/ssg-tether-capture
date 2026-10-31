//! Capture plan schema v1 (design D3, D5, D6, D10): the reader and writer, one
//! test per validation rule, the compatibility rule (unknown keys and newer
//! versions refused, with the key named), the v1 round trip on generated plans
//! and the published JSON Schema file.

use serde_json::{Value, json};

use crate::capture::plan::{
    CapturePlan, CapturePlanDevice, CaptureSphere, ConditionBounds, MigratedFromV0, Phase,
    PhaseEnd, Transition, TransitionConditions, UnmappedValue, validate_capture_plan,
};
use crate::project::plan_files::{plan_file_reports, plan_to_json, read_plan_value, save_plan};
use crate::test_support::{Lcg, TempDir, repo_root};

const SEED: u64 = 0x5C4E_3A01;
const CASES: usize = 300;
const ID: &str = "p";

/// A valid one-phase v1 document to break.
fn doc() -> Value {
    json!({
        "schema_version": 1,
        "name": "Plan",
        "tether": "Tether1",
        "phases": [
            {"id": "a", "max_velocity_mps": 1.0, "max_force_n": 2.0,
             "transitions": [{"to": "b", "all_of": {"distance_m": {"less_than": 5.0}}}]},
            {"id": "b", "max_velocity_mps": 1.0, "max_force_n": 2.0}
        ]
    })
}

fn errors_of(doc: &Value) -> Vec<String> {
    read_plan_value(ID, doc).errors
}

#[test]
fn a_v1_document_loads_with_its_id_and_no_warnings() {
    let report = read_plan_value(ID, &doc());
    assert_eq!(report.errors, Vec::<String>::new());
    assert_eq!(report.warnings, Vec::<String>::new());
    let plan = report.plan.unwrap();
    assert_eq!(plan.id, ID);
    assert_eq!(plan.phases.len(), 2);
    assert_eq!(plan.migrated_from_v0, None);
}

// ---------------------------------------------------------------------------
// Compatibility (D6): versions and unknown keys
// ---------------------------------------------------------------------------

#[test]
fn a_newer_or_unknown_schema_version_is_refused() {
    for (version, message) in [
        (
            json!(2),
            "[p] /schema_version: 2 is not supported; this version reads schema_version 1 \
             (the file was written by a newer version)",
        ),
        (
            json!(0),
            "[p] /schema_version: 0 is not supported; this version reads schema_version 1",
        ),
        (
            json!("1"),
            "[p] /schema_version: \"1\" is not supported; this version reads schema_version 1",
        ),
    ] {
        let mut d = doc();
        d["schema_version"] = version;
        assert_eq!(errors_of(&d), [message]);
    }
}

#[test]
fn an_unknown_key_is_refused_with_the_key_and_where_it_is() {
    let cases: [(&str, fn(&mut Value)); 7] = [
        ("/", |d| d["units"] = json!("m")),
        ("/device", |d| d["device"] = json!({"tether_length": 20.0})),
        ("/capture_sphere", |d| {
            d["capture_sphere"] = json!({"radius_m": 3.0})
        }),
        ("/phases/1", |d| d["phases"][1]["parameters"] = json!({})),
        ("/phases/0/transitions/0", |d| {
            d["phases"][0]["transitions"][0]["distance_m"] = json!({"less_than": 1.0})
        }),
        ("/phases/0/transitions/0/all_of", |d| {
            d["phases"][0]["transitions"][0]["all_of"]["time_in_state_s"] = json!({})
        }),
        ("/phases/0/transitions/0/all_of/distance_m", |d| {
            d["phases"][0]["transitions"][0]["all_of"]["distance_m"]["units"] = json!("ft")
        }),
    ];
    for (pointer, break_it) in cases {
        let mut d = doc();
        break_it(&mut d);
        let errors = errors_of(&d);
        assert_eq!(errors.len(), 1, "{pointer}: {errors:?}");
        assert!(
            errors[0].starts_with(&format!("[p] {pointer}: unknown field `")),
            "{pointer}: {errors:?}"
        );
    }
    let mut d = doc();
    d["units"] = json!("m");
    assert!(errors_of(&d)[0].starts_with("[p] /: unknown field `units`, expected one of"));
}

#[test]
fn a_wrong_type_is_reported_as_such_with_where_it_is() {
    // F-32: a string number was reported as a missing parameter in v0.
    let mut d = doc();
    d["phases"][1]["max_force_n"] = json!("2.0");
    assert_eq!(
        errors_of(&d),
        ["[p] /phases/1: invalid type: string \"2.0\", expected f64"]
    );
    let mut d = doc();
    d["phases"][0]["end"] = json!("done");
    assert_eq!(
        errors_of(&d),
        ["[p] /phases/0: unknown variant `done`, expected `success` or `abort`"]
    );
    let mut d = doc();
    d["name"] = json!(7);
    assert_eq!(
        errors_of(&d),
        ["[p] /: invalid type: integer `7`, expected a string"]
    );
}

// ---------------------------------------------------------------------------
// Validation rules (D3, D5): one row per rule, exact messages
// ---------------------------------------------------------------------------

#[test]
fn each_validation_rule_reports_its_message() {
    let rows: Vec<(&str, fn(&mut Value), Vec<&str>)> = vec![
        (
            "name blank",
            |d| d["name"] = json!(" "),
            vec!["[p] /name: must not be blank"],
        ),
        (
            "tether blank",
            |d| d["tether"] = json!(""),
            vec!["[p] /tether: must not be blank"],
        ),
        (
            "tether whitespace (F-43)",
            |d| d["tether"] = json!(" \t"),
            vec!["[p] /tether: must not be blank"],
        ),
        (
            "tether length",
            |d| d["device"] = json!({"tether_length_m": 0.0}),
            vec!["[p] /device/tether_length_m: must be greater than 0"],
        ),
        (
            "segment spacing",
            |d| d["device"] = json!({"segment_spacing_m": -0.1}),
            vec!["[p] /device/segment_spacing_m: must be greater than 0"],
        ),
        (
            "body count",
            |d| d["device"] = json!({"tether_length_m": 101.0, "segment_spacing_m": 0.1}),
            vec![
                "[p] /device/segment_spacing_m: gives 1002 tether bodies; at most 1000 are allowed",
            ],
        ),
        (
            "segment mass",
            |d| d["device"] = json!({"segment_mass_kg": 0.0}),
            vec!["[p] /device/segment_mass_kg: must be greater than 0"],
        ),
        (
            "end mass",
            |d| d["device"] = json!({"end_mass_kg": -2.0}),
            vec!["[p] /device/end_mass_kg: must be greater than 0"],
        ),
        (
            "damping",
            |d| d["device"] = json!({"joint_damping_linear": -1.0, "joint_damping_angular": -1.0}),
            vec![
                "[p] /device/joint_damping_linear: must not be negative",
                "[p] /device/joint_damping_angular: must not be negative",
            ],
        ),
        (
            "capture sphere",
            |d| d["capture_sphere"] = json!({"initial_radius_m": 0.0}),
            vec!["[p] /capture_sphere/initial_radius_m: must be greater than 0"],
        ),
        (
            "no phases",
            |d| d["phases"] = json!([]),
            vec!["[p] /phases: at least one phase is required"],
        ),
        (
            "phase id blank",
            |d| d["phases"][1]["id"] = json!(""),
            vec![
                "[p] /phases/0/transitions/0/to: unknown phase 'b'",
                "[p] /phases/1/id: must not be blank",
            ],
        ),
        (
            "phase id duplicate",
            |d| d["phases"][1]["id"] = json!("a"),
            vec![
                "[p] /phases/0/transitions/0/to: unknown phase 'b'",
                "[p] /phases/1/id: duplicate phase id 'a'",
            ],
        ),
        (
            "negative limits (F-32)",
            |d| {
                d["phases"][1]["max_velocity_mps"] = json!(-1.0);
                d["phases"][1]["max_force_n"] = json!(-5.0);
                d["phases"][1]["shrink_rate_mps"] = json!(-0.1);
            },
            vec![
                "[p] /phases/1/max_velocity_mps: must not be negative",
                "[p] /phases/1/max_force_n: must not be negative",
                "[p] /phases/1/shrink_rate_mps: must not be negative",
            ],
        ),
        (
            "unknown target phase",
            |d| d["phases"][0]["transitions"][0]["to"] = json!("z"),
            vec!["[p] /phases/0/transitions/0/to: unknown phase 'z'"],
        ),
        (
            "no condition",
            |d| d["phases"][0]["transitions"][0]["all_of"] = json!({}),
            vec!["[p] /phases/0/transitions/0/all_of: at least one condition is required"],
        ),
        (
            "no bound",
            |d| d["phases"][0]["transitions"][0]["all_of"] = json!({"relative_speed_mps": {}}),
            vec![
                "[p] /phases/0/transitions/0/all_of/relative_speed_mps: at least one of \
                 less_than, greater_than is required",
            ],
        ),
        (
            "negative bounds",
            |d| {
                d["phases"][0]["transitions"][0]["all_of"] =
                    json!({"distance_m": {"less_than": -1.0, "greater_than": -2.0}})
            },
            vec![
                "[p] /phases/0/transitions/0/all_of/distance_m/less_than: must not be negative",
                "[p] /phases/0/transitions/0/all_of/distance_m/greater_than: must not be negative",
            ],
        ),
        (
            "end phase with transitions",
            |d| d["phases"][0]["end"] = json!("abort"),
            vec!["[p] /phases/0/transitions: an end phase has no transitions"],
        ),
    ];
    for (rule, break_it, expected) in rows {
        let mut d = doc();
        break_it(&mut d);
        assert_eq!(errors_of(&d), expected, "{rule}");
        let plan: CapturePlan = serde_json::from_value(d).unwrap();
        assert_eq!(
            validate_capture_plan(ID, &plan),
            expected,
            "{rule}: validator"
        );
    }
}

#[test]
fn a_blank_tether_is_refused_in_v0_and_v1_files() {
    for tether in ["", " ", "\t "] {
        let v0 = json!({"name": "p", "tether": tether, "states": [
            {"id": "a", "parameters": {"max_velocity": 1.0, "max_force": 1.0}}]});
        let mut v1 = doc();
        v1["tether"] = json!(tether);
        for d in [v0, v1] {
            assert_eq!(errors_of(&d), ["[p] /tether: must not be blank"], "{d}");
        }
    }
}

#[test]
fn an_end_phase_and_a_tether_between_200_and_1000_bodies_load() {
    let mut d = doc();
    d["phases"][1]["end"] = json!("success");
    // Bodies = floor((length - 2 x 0.5 m end spheres) / spacing) + 2.
    d["device"] = json!({"tether_length_m": 500.0, "segment_spacing_m": 0.5});
    let report = read_plan_value(ID, &d);
    assert_eq!(report.errors, Vec::<String>::new());
    assert_eq!(
        report.warnings,
        [
            "[p] /device/segment_spacing_m: gives 1000 tether bodies; more than 200 make \
          recordings large"
        ]
    );
    assert_eq!(report.plan.unwrap().phases[1].end, Some(PhaseEnd::Success));

    d["device"] = json!({"tether_length_m": 500.5, "segment_spacing_m": 0.5});
    assert_eq!(
        read_plan_value(ID, &d).errors,
        ["[p] /device/segment_spacing_m: gives 1001 tether bodies; at most 1000 are allowed"]
    );
    d["device"] = json!({"tether_length_m": 100.0, "segment_spacing_m": 0.5});
    assert_eq!(
        read_plan_value(ID, &d).warnings,
        Vec::<String>::new(),
        "200 bodies"
    );
    d["device"] = json!({"tether_length_m": 100.5, "segment_spacing_m": 0.5});
    assert_eq!(read_plan_value(ID, &d).warnings.len(), 1, "201 bodies");
    // The default 20 m tether at 0.1 m spacing (191 bodies) does not warn.
    d["device"] = json!({});
    assert_eq!(read_plan_value(ID, &d).warnings, Vec::<String>::new());
}

#[test]
fn absent_optional_fields_take_the_documented_defaults() {
    let device = CapturePlanDevice::default();
    assert_eq!(device.device_type(), "tether");
    assert_eq!(device.tether_length_m(), 20.0);
    assert_eq!(device.segment_spacing_m(), 0.1);
    assert_eq!(device.segment_mass_kg(), 0.1);
    assert_eq!(device.end_mass_kg(), 2.0);
    assert_eq!(device.joint_damping_linear(), 1.0);
    assert_eq!(device.joint_damping_angular(), 1.0);
    let set = CapturePlanDevice {
        device_type: Some("net".into()),
        tether_length_m: Some(5.0),
        segment_spacing_m: Some(0.5),
        segment_mass_kg: Some(0.2),
        end_mass_kg: Some(3.0),
        joint_damping_linear: Some(0.0),
        joint_damping_angular: Some(0.5),
    };
    assert_eq!(
        (
            set.device_type(),
            set.tether_length_m(),
            set.segment_spacing_m(),
            set.segment_mass_kg(),
            set.end_mass_kg(),
            set.joint_damping_linear(),
            set.joint_damping_angular()
        ),
        ("net", 5.0, 0.5, 0.2, 3.0, 0.0, 0.5)
    );

    let mut plan: CapturePlan = serde_json::from_value(doc()).unwrap();
    assert_eq!(plan.initial_capture_radius_m(), 25.0);
    plan.capture_sphere = Some(CaptureSphere::default());
    assert_eq!(plan.initial_capture_radius_m(), 25.0);
    plan.capture_sphere = Some(CaptureSphere {
        initial_radius_m: Some(7.5),
    });
    assert_eq!(plan.initial_capture_radius_m(), 7.5);
}

// ---------------------------------------------------------------------------
// Writer, files and the round trip
// ---------------------------------------------------------------------------

#[test]
fn the_writer_puts_schema_version_first_and_only_the_keys_the_plan_has() {
    let plan: CapturePlan = serde_json::from_value(doc()).unwrap();
    let text = plan_to_json(&plan);
    assert!(
        text.starts_with("{\n  \"schema_version\": 1,\n  \"name\": \"Plan\","),
        "{text}"
    );
    for absent in [
        "device",
        "capture_sphere",
        "shrink_rate_mps",
        "end",
        "migrated_from_v0",
        "\"id\": \"\"",
    ] {
        assert!(!text.contains(absent), "{absent} in\n{text}");
    }
    let example = repo_root().join("assets/example_capture_plans/example_plan.json");
    // A Windows checkout may turn the file's line feeds into CR LF.
    let raw = std::fs::read_to_string(&example)
        .unwrap()
        .replace("\r\n", "\n");
    let loaded = read_plan_value("example_plan", &serde_json::from_str(&raw).unwrap());
    assert_eq!(
        plan_to_json(&loaded.plan.unwrap()),
        raw,
        "the example file is exactly what the v1 writer writes"
    );
}

#[test]
fn save_plan_writes_the_v1_file_and_the_loader_skips_project_json() {
    let dir = TempDir::new("plan-v1-save");
    let plan: CapturePlan = serde_json::from_value(doc()).unwrap();
    save_plan(dir.path(), "mine.json", &plan).unwrap();
    dir.write(
        "project.json",
        r#"{"schema_version": 1, "name": "A project"}"#,
    );
    dir.write("notes.txt", "not a plan");

    assert_eq!(
        std::fs::read_to_string(dir.path().join("mine.json")).unwrap(),
        plan_to_json(&plan)
    );
    let reports = plan_file_reports(dir.path());
    assert_eq!(reports.len(), 1, "only mine.json is a plan file");
    assert_eq!(reports[0].0, "mine");
    let mut expected = plan.clone();
    expected.id = "mine".into();
    assert_eq!(reports[0].1.plan.as_ref(), Some(&expected));

    let missing = dir.path().join("no_such_dir");
    assert_eq!(
        save_plan(&missing, "p.json", &plan).unwrap_err().kind(),
        std::io::ErrorKind::NotFound
    );
    assert!(plan_file_reports(&missing).is_empty());
}

/// A number with at most 15 significant digits (see `plan_migration_v0::short`).
fn short(x: f64) -> f64 {
    format!("{x:.6e}").parse().unwrap()
}

fn maybe(rng: &mut Lcg, lo: f64, hi: f64) -> Option<f64> {
    rng.chance(0.5).then(|| short(rng.range(lo, hi)))
}

fn bounds(rng: &mut Lcg, hi: f64) -> Option<ConditionBounds> {
    rng.chance(0.6).then(|| {
        let less_than = maybe(rng, 0.0, hi);
        let greater_than = if less_than.is_none() {
            Some(short(rng.range(0.0, hi)))
        } else {
            maybe(rng, 0.0, hi)
        };
        ConditionBounds {
            less_than,
            greater_than,
        }
    })
}

fn generate_v1(rng: &mut Lcg, case: usize) -> CapturePlan {
    let count = 1 + rng.below(6);
    let id = |i: usize| format!("phase_{i}");
    let phases = (0..count)
        .map(|i| {
            let end = (i == count - 1 && rng.chance(0.5))
                .then(|| [PhaseEnd::Success, PhaseEnd::Abort][rng.below(2)]);
            let transitions = if end.is_some() || rng.chance(0.2) {
                None
            } else {
                Some(
                    (0..rng.below(4))
                        .map(|_| {
                            let mut all_of = TransitionConditions {
                                distance_m: bounds(rng, 5_000.0),
                                relative_speed_mps: bounds(rng, 50.0),
                            };
                            if all_of.distance_m.is_none() && all_of.relative_speed_mps.is_none() {
                                all_of.distance_m = Some(ConditionBounds {
                                    less_than: Some(short(rng.range(0.0, 5_000.0))),
                                    greater_than: None,
                                });
                            }
                            Transition {
                                to: id(rng.below(count)),
                                all_of,
                            }
                        })
                        .collect(),
                )
            };
            Phase {
                id: id(i),
                max_velocity_mps: short(rng.range(0.0, 50.0)),
                max_force_n: short(rng.range(0.0, 50.0)),
                shrink_rate_mps: maybe(rng, 0.0, 0.1),
                transitions,
                end,
                tail_actuation: rng.chance(0.5).then(|| rng.chance(0.5)),
            }
        })
        .collect();
    let device = rng.chance(0.7).then(|| CapturePlanDevice {
        device_type: rng.chance(0.5).then(|| "tether".to_string()),
        tether_length_m: maybe(rng, 1.0, 50.0),
        segment_spacing_m: maybe(rng, 0.1, 1.0),
        segment_mass_kg: maybe(rng, 0.01, 1.0),
        end_mass_kg: maybe(rng, 0.5, 5.0),
        joint_damping_linear: maybe(rng, 0.0, 2.0),
        joint_damping_angular: maybe(rng, 0.0, 2.0),
    });
    CapturePlan {
        schema_version: 1,
        name: format!("Generated v1 plan {case}"),
        id: String::new(),
        tether: format!("Tether{}", case % 4),
        device,
        capture_sphere: rng.chance(0.3).then(|| CaptureSphere {
            initial_radius_m: maybe(rng, 1.0, 50.0),
        }),
        phases,
        migrated_from_v0: rng.chance(0.2).then(|| MigratedFromV0 {
            unmapped: vec![UnmappedValue {
                pointer: "/states/0/next".into(),
                value: json!(["s1", null, short(rng.range(-1.0, 1.0)), {"units": "m"}]),
            }],
        }),
    }
}

#[test]
fn generated_v1_plans_survive_write_and_read() {
    let mut rng = Lcg::new(SEED);
    for case in 0..CASES {
        let plan = generate_v1(&mut rng, case);
        let ctx = format!("seed {SEED:#x} case {case}: {plan:?}");
        assert_eq!(
            validate_capture_plan(ID, &plan),
            Vec::<String>::new(),
            "{ctx}"
        );
        let text = plan_to_json(&plan);
        let report = read_plan_value(ID, &serde_json::from_str(&text).unwrap());
        assert_eq!(report.errors, Vec::<String>::new(), "{ctx}");
        let mut read = report.plan.expect("loads");
        assert_eq!(read.id, ID);
        read.id.clear();
        assert_eq!(read, plan, "{ctx}");
        assert_eq!(plan_to_json(&read), text, "{ctx}: written twice, same text");
    }
    println!("seed {SEED:#x}: {CASES} generated v1 plans written and read back equal");
}

// ---------------------------------------------------------------------------
// JSON Schema (D10)
// ---------------------------------------------------------------------------

const SCHEMA_BASE: &str = "https://conjunction-crew.github.io/ssg-tether-capture/schemas";

/// Compares the schema generated from `T` with the committed file; with
/// `SSG_UPDATE_SCHEMAS=1` it rewrites the file first.
pub(crate) fn check_schema<T: schemars::JsonSchema>(file: &str) {
    let mut schema = serde_json::to_value(schemars::schema_for!(T)).unwrap();
    schema["$id"] = json!(format!("{SCHEMA_BASE}/{file}"));
    let generated = serde_json::to_string_pretty(&schema).unwrap() + "\n";
    let path = repo_root().join("docs_site/static/schemas").join(file);
    if std::env::var_os("SSG_UPDATE_SCHEMAS").is_some() {
        std::fs::write(&path, &generated).unwrap();
    }
    // A Windows checkout may turn the file's line feeds into CR LF.
    let committed = std::fs::read_to_string(&path)
        .unwrap_or_default()
        .replace("\r\n", "\n");
    if committed != generated {
        let diff: Vec<String> = committed
            .lines()
            .zip(generated.lines())
            .enumerate()
            .filter(|(_, (c, g))| c != g)
            .take(10)
            .map(|(n, (c, g))| format!("line {}:\n- {c}\n+ {g}", n + 1))
            .collect();
        panic!(
            "{file} is out of date ({} committed lines, {} generated); regenerate with \
             `SSG_UPDATE_SCHEMAS=1 cargo test -p ssg-sim schema_files`\n{}",
            committed.lines().count(),
            generated.lines().count(),
            diff.join("\n")
        );
    }
}

#[test]
fn schema_files_are_current() {
    check_schema::<CapturePlan>("capture-plan.v1.schema.json");
}

#[test]
fn the_schema_requires_the_v1_keys_and_refuses_unknown_ones() {
    let schema = serde_json::to_value(schemars::schema_for!(CapturePlan)).unwrap();
    let mut required: Vec<&str> = schema["required"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    required.sort();
    assert_eq!(required, ["name", "phases", "schema_version", "tether"]);
    assert_eq!(schema["additionalProperties"], json!(false));
    assert!(
        schema["properties"].get("id").is_none(),
        "the file stem is not a key"
    );
    assert_eq!(schema["properties"]["schema_version"]["minimum"], json!(1));
    assert_eq!(schema["properties"]["schema_version"]["maximum"], json!(1));
}

#[test]
fn tail_actuation_is_a_phase_key_that_round_trips_and_not_a_device_key() {
    // P7.5 S1 (C7): `phases[].tail_actuation` is optional, written only when set,
    // compiled per phase; the device block has no such key.
    let mut value = doc();
    value["phases"][1]["tail_actuation"] = json!(true);
    let plan = read_plan_value(ID, &value).plan.expect("loads");
    assert_eq!(plan.phases[0].tail_actuation, None);
    assert_eq!(plan.phases[1].tail_actuation, Some(true));
    let text = plan_to_json(&plan);
    assert_eq!(text.matches("tail_actuation").count(), 1, "{text}");
    let reread = read_plan_value(ID, &serde_json::from_str(&text).unwrap())
        .plan
        .expect("reloads");
    assert_eq!(reread, plan);
    let compiled = crate::capture::plan::compile_capture_plan(&plan);
    assert!(!compiled.states[0].parameters.tail_actuation);
    assert!(compiled.states[1].parameters.tail_actuation);

    let mut value = doc();
    value["device"] = json!({ "tail_actuation": true });
    let errors = errors_of(&value);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(
        errors[0].contains("/device") && errors[0].contains("tail_actuation"),
        "{errors:?}"
    );

    let schema = serde_json::to_value(schemars::schema_for!(CapturePlan)).unwrap();
    let text = schema.to_string();
    assert!(
        schema["$defs"]["Phase"]["properties"]["tail_actuation"].is_object(),
        "{text}"
    );
    assert!(
        schema["$defs"]["CapturePlanDevice"]["properties"]
            .get("tail_actuation")
            .is_none()
    );
}
