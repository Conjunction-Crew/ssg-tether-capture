//! `recordings/<name>/manifest.json` (design D8, D6, D10; P6 S6): the reader and
//! validator with one test per rule, the compatibility rule, the write-and-read
//! round trips on generated manifests and the published JSON Schema file.

use serde_json::{Value, json};

use crate::capture::plan::CapturePlan;
use crate::project::manifest::BuiltWith;
use crate::project::plan_files::read_plan_value;
use crate::project::recording::{
    RECORDING_FRAMES_FILE, RECORDING_INPUTS_FILE, RECORDING_KEYFRAMES_PARQUET, RECORDING_LOG_FILE,
    RECORDING_MANIFEST_FILE, RECORDING_PARTIAL_SUFFIX, RECORDING_SCHEMA_VERSION,
    RECORDING_TRANSITIONS_FILE, RecordingBody, RecordingBodyRole, RecordingEndReason,
    RecordingLimits, RecordingManifest, RecordingSource, RecordingStartTrigger,
    read_recording_file, read_recording_value, recording_manifest_to_json, save_recording_manifest,
};
use crate::test_support::{Lcg, TempDir, repo_root};
use crate::tests::plan_schema_v1::check_schema;

const START: &str = "2026-04-03T16:03:47.002464Z";

/// The shipped example plan (v1), with its id set as the loader sets it.
fn example_plan() -> CapturePlan {
    let path = repo_root().join("assets/example_capture_plans/example_plan.json");
    let raw = std::fs::read_to_string(path).unwrap();
    let report = read_plan_value("example_plan", &serde_json::from_str(&raw).unwrap());
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    report.plan.unwrap()
}

fn body(index: u32, role: RecordingBodyRole, name: &str) -> RecordingBody {
    RecordingBody {
        index,
        role,
        name: name.to_string(),
    }
}

/// A valid manifest: the example plan, a target, a root, two nodes and a tail.
fn sample() -> RecordingManifest {
    RecordingManifest {
        schema_version: RECORDING_SCHEMA_VERSION,
        built_with: BuiltWith {
            app_version: "0.4.0-rc.1".to_string(),
            target: "aarch64-macos".to_string(),
            avian_features: vec!["simd".to_string(), "parallel".to_string()],
        },
        plan_id: "example_plan".to_string(),
        plan: example_plan(),
        start_step: 4318,
        start_epoch_utc: START.to_string(),
        keyframe_interval_steps: 64,
        bodies: vec![
            body(0, RecordingBodyRole::Target, "HST"),
            body(1, RecordingBodyRole::TetherRoot, "root"),
            body(2, RecordingBodyRole::TetherNode, "node 1"),
            body(3, RecordingBodyRole::TetherNode, "node 2"),
            body(4, RecordingBodyRole::TetherTail, "tail"),
        ],
        limits: RecordingLimits {
            max_duration_s: 1800.0,
            max_tether_bodies: 200,
            max_size_mb: None,
        },
        project: None,
        source: None,
        end_step: None,
        end_reason: None,
        start_trigger: None,
        start_phase: None,
    }
}

/// A valid `project.json` document for the embedded `project`.
fn project_value() -> Value {
    json!({
        "schema_version": 1,
        "name": "p",
        "start_epoch_utc": START,
        "target": { "label": "T", "orbit": { "elements": {
            "semi_major_axis_m": 6.9e6, "eccentricity": 0.001, "inclination_rad": 0.5,
            "raan_rad": 0.1, "arg_perigee_rad": 0.2, "mean_anomaly_rad": 0.3,
            "epoch_utc": START } } },
        "chaser": { "label": "C", "orbit": { "elements": {
            "semi_major_axis_m": 6.9e6, "eccentricity": 0.001, "inclination_rad": 0.5,
            "raan_rad": 0.1, "arg_perigee_rad": 0.2, "mean_anomaly_rad": 0.3,
            "epoch_utc": START } } }
    })
}

fn doc() -> Value {
    serde_json::to_value(sample()).unwrap()
}

#[track_caller]
fn assert_error(value: &Value, expected: &[&str]) {
    let report = read_recording_value(value);
    assert_eq!(report.errors, expected, "errors for {value}");
    assert!(
        report.manifest.is_none(),
        "a manifest with errors does not load"
    );
}

// ---------------------------------------------------------------------------
// the layout and the example
// ---------------------------------------------------------------------------

#[test]
fn the_layout_names_are_the_designs() {
    assert_eq!(RECORDING_MANIFEST_FILE, "manifest.json");
    assert_eq!(RECORDING_INPUTS_FILE, "inputs.jsonl");
    assert_eq!(RECORDING_KEYFRAMES_PARQUET, "keyframes.parquet");
    assert_eq!(RECORDING_FRAMES_FILE, "frames.jsonl");
    assert_eq!(RECORDING_TRANSITIONS_FILE, "transitions.jsonl");
    assert_eq!(RECORDING_LOG_FILE, "log.jsonl");
    assert_eq!(RECORDING_PARTIAL_SUFFIX, ".partial");
    assert_eq!(RECORDING_SCHEMA_VERSION, 1);
}

#[test]
fn a_valid_manifest_loads_and_keeps_its_plan_id() {
    let report = read_recording_value(&doc());
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    let manifest = report.manifest.unwrap();
    assert_eq!(manifest, sample());
    assert_eq!(
        manifest.plan.id, "example_plan",
        "the plan's id is the plan_id"
    );
    assert_eq!(manifest.bodies.len(), 5);
    assert_eq!(manifest.bodies[1].role, RecordingBodyRole::TetherRoot);
}

#[test]
fn the_written_document_has_the_designs_keys_and_snake_case_roles() {
    let value = doc();
    let mut keys: Vec<&str> = value
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort();
    assert_eq!(
        keys,
        [
            "bodies",
            "built_with",
            "keyframe_interval_steps",
            "limits",
            "plan",
            "plan_id",
            "schema_version",
            "start_epoch_utc",
            "start_step"
        ]
    );
    assert_eq!(value["bodies"][1]["role"], json!("tether_root"));
    assert_eq!(value["bodies"][4]["role"], json!("tether_tail"));
    assert!(
        value["plan"].get("id").is_none(),
        "the plan's id is not a key of the embedded plan either"
    );
    assert_eq!(value["plan"]["schema_version"], json!(1));
}

// ---------------------------------------------------------------------------
// the compatibility rule (D6)
// ---------------------------------------------------------------------------

#[test]
fn a_newer_or_missing_or_unknown_schema_version_is_refused() {
    let mut value = doc();
    value.as_object_mut().unwrap().remove("schema_version");
    assert_error(&value, &["[manifest.json] /schema_version: missing"]);

    let mut value = doc();
    value["schema_version"] = json!(2);
    assert_error(
        &value,
        &[
            "[manifest.json] /schema_version: 2 is not supported; this version reads \
           schema_version 1 (the file was written by a newer version)",
        ],
    );

    let mut value = doc();
    value["schema_version"] = json!(0);
    assert_error(
        &value,
        &[
            "[manifest.json] /schema_version: 0 is not supported; this version reads schema_version 1",
        ],
    );

    let mut value = doc();
    value["schema_version"] = json!("1");
    assert_error(
        &value,
        &[
            "[manifest.json] /schema_version: \"1\" is not supported; this version reads schema_version 1",
        ],
    );
}

#[test]
fn an_unknown_key_is_refused_with_the_key_named_and_where() {
    for (pointer, setup) in [
        ("/", None),
        ("/built_with", Some("built_with")),
        ("/limits", Some("limits")),
    ] {
        let mut value = doc();
        let object = match setup {
            None => value.as_object_mut().unwrap(),
            Some(key) => value[key].as_object_mut().unwrap(),
        };
        object.insert("future_key".to_string(), json!(1));
        let report = read_recording_value(&value);
        assert_eq!(report.errors.len(), 1, "{pointer}: {:?}", report.errors);
        assert!(report.manifest.is_none());
        assert!(
            report.errors[0].starts_with(&format!(
                "[manifest.json] {pointer}: unknown field `future_key`"
            )),
            "{pointer}: {}",
            report.errors[0]
        );
    }

    let mut value = doc();
    value["bodies"][2]["colour"] = json!("red");
    let report = read_recording_value(&value);
    assert!(
        report.errors[0].starts_with("[manifest.json] /bodies/2: unknown field `colour`"),
        "{}",
        report.errors[0]
    );

    let mut value = doc();
    value["plan"]["phases"][1]["future_key"] = json!(true);
    let report = read_recording_value(&value);
    assert_eq!(report.errors.len(), 1, "{:?}", report.errors);
    assert!(
        report.errors[0].starts_with("[manifest.json] /plan/phases/1: unknown field `future_key`"),
        "{}",
        report.errors[0]
    );
}

#[test]
fn a_wrong_type_is_reported_where_it_is() {
    let mut value = doc();
    value["limits"]["max_duration_s"] = json!("long");
    let report = read_recording_value(&value);
    assert!(
        report.errors[0].starts_with("[manifest.json] /limits: invalid type: string \"long\""),
        "{}",
        report.errors[0]
    );

    let mut value = doc();
    value["bodies"][3]["role"] = json!("rope");
    let report = read_recording_value(&value);
    assert!(
        report.errors[0].starts_with("[manifest.json] /bodies/3: unknown variant `rope`"),
        "{}",
        report.errors[0]
    );

    let mut value = doc();
    value["start_step"] = json!(-1);
    let report = read_recording_value(&value);
    assert!(
        report.errors[0].starts_with("[manifest.json] /: invalid value: integer `-1`"),
        "{}",
        report.errors[0]
    );
}

#[test]
fn the_embedded_plan_is_always_schema_v1() {
    // A v0 plan (no schema_version) is not migrated inside a recording.
    let mut value = doc();
    value["plan"]
        .as_object_mut()
        .unwrap()
        .remove("schema_version");
    assert_error(
        &value,
        &["[manifest.json] /plan/schema_version: must be 1 (an embedded plan is always schema v1)"],
    );
    let mut value = doc();
    value["plan"]["schema_version"] = json!(2);
    assert_error(
        &value,
        &["[manifest.json] /plan/schema_version: must be 1 (an embedded plan is always schema v1)"],
    );
}

// ---------------------------------------------------------------------------
// one test row per rule
// ---------------------------------------------------------------------------

#[test]
fn each_validation_rule_reports_its_message() {
    let cases: Vec<(&str, Box<dyn Fn(&mut Value)>, &str)> = vec![
        (
            "blank plan id",
            Box::new(|v| v["plan_id"] = json!("  ")),
            "[manifest.json] /plan_id: must not be blank",
        ),
        (
            "epoch not UTC",
            Box::new(|v| v["start_epoch_utc"] = json!("yesterday")),
            "[manifest.json] /start_epoch_utc: `yesterday` is not a UTC epoch (YYYY-MM-DDTHH:MM:SS.ffffffZ)",
        ),
        (
            "keyframe interval zero",
            Box::new(|v| v["keyframe_interval_steps"] = json!(0)),
            "[manifest.json] /keyframe_interval_steps: must be at least 1",
        ),
        (
            "no bodies",
            Box::new(|v| v["bodies"] = json!([])),
            "[manifest.json] /bodies: must list at least one body",
        ),
        (
            "body index off",
            Box::new(|v| v["bodies"][2]["index"] = json!(7)),
            "[manifest.json] /bodies/2/index: must be 2 (the position in the list), not 7",
        ),
        (
            "blank body name",
            Box::new(|v| v["bodies"][4]["name"] = json!("")),
            "[manifest.json] /bodies/4/name: must not be blank",
        ),
        (
            "duration zero",
            Box::new(|v| v["limits"]["max_duration_s"] = json!(0.0)),
            "[manifest.json] /limits/max_duration_s: must be greater than 0",
        ),
        (
            "duration negative",
            Box::new(|v| v["limits"]["max_duration_s"] = json!(-5.0)),
            "[manifest.json] /limits/max_duration_s: must be greater than 0",
        ),
        (
            "tether bodies zero",
            Box::new(|v| v["limits"]["max_tether_bodies"] = json!(0)),
            "[manifest.json] /limits/max_tether_bodies: must be at least 1",
        ),
        (
            "more tether bodies than allowed",
            Box::new(|v| v["limits"]["max_tether_bodies"] = json!(3)),
            "[manifest.json] /bodies: lists 4 tether bodies; limits.max_tether_bodies allows 3",
        ),
        (
            "size cap zero",
            Box::new(|v| v["limits"]["max_size_mb"] = json!(0.0)),
            "[manifest.json] /limits/max_size_mb: must be greater than 0",
        ),
        (
            "end before start",
            Box::new(|v| v["end_step"] = json!(4000)),
            "[manifest.json] /end_step: 4000 is before start_step 4318",
        ),
        (
            "phase trigger without a phase",
            Box::new(|v| v["start_trigger"] = json!("phase")),
            "[manifest.json] /start_phase: missing (start_trigger is `phase`)",
        ),
        (
            "blank start phase",
            Box::new(|v| {
                v["start_trigger"] = json!("phase");
                v["start_phase"] = json!(" ");
            }),
            "[manifest.json] /start_phase: must not be blank",
        ),
        (
            "phase with a manual trigger",
            Box::new(|v| {
                v["start_trigger"] = json!("manual");
                v["start_phase"] = json!("approach");
            }),
            "[manifest.json] /start_phase: only allowed when start_trigger is `phase`",
        ),
        (
            "phase without a trigger",
            Box::new(|v| v["start_phase"] = json!("approach")),
            "[manifest.json] /start_phase: only allowed when start_trigger is `phase`",
        ),
        (
            "embedded project rule",
            Box::new(|v| {
                v["project"] = project_value();
                v["project"]["name"] = json!(" ");
            }),
            "[manifest.json] /project/name: must not be blank",
        ),
        (
            "plan rule",
            Box::new(|v| v["plan"]["phases"][0]["max_force_n"] = json!(-1.0)),
            "[manifest.json] /plan/phases/0/max_force_n: must not be negative",
        ),
        (
            "blank plan tether",
            Box::new(|v| v["plan"]["tether"] = json!(" ")),
            "[manifest.json] /plan/tether: must not be blank",
        ),
    ];
    for (what, mutate, expected) in cases {
        let mut value = doc();
        mutate(&mut value);
        let report = read_recording_value(&value);
        assert_eq!(report.errors, [expected], "{what}");
        assert!(report.manifest.is_none(), "{what}");
    }
}

#[test]
fn independent_errors_are_reported_in_field_order() {
    let mut value = doc();
    value["limits"]["max_duration_s"] = json!(0.0);
    value["bodies"][0]["name"] = json!("");
    value["start_epoch_utc"] = json!("nope");
    value["plan_id"] = json!("");
    value["plan"]["name"] = json!("");
    assert_error(
        &value,
        &[
            "[manifest.json] /plan_id: must not be blank",
            "[manifest.json] /plan/name: must not be blank",
            "[manifest.json] /start_epoch_utc: `nope` is not a UTC epoch (YYYY-MM-DDTHH:MM:SS.ffffffZ)",
            "[manifest.json] /bodies/0/name: must not be blank",
            "[manifest.json] /limits/max_duration_s: must be greater than 0",
        ],
    );
}

#[test]
fn a_manifest_that_says_another_schema_version_does_not_validate() {
    let mut manifest = sample();
    manifest.schema_version = 2;
    assert_eq!(
        manifest.validate(),
        ["[manifest.json] /schema_version: must be 1"]
    );
}

// ---------------------------------------------------------------------------
// files
// ---------------------------------------------------------------------------

#[test]
fn a_folder_without_a_manifest_has_no_report() {
    let dir = TempDir::new("recording_none");
    assert!(read_recording_file(dir.path()).is_none());
}

#[test]
fn unreadable_manifest_files_are_reported() {
    let dir = TempDir::new("recording_bad");
    dir.write("manifest.json", "{ not json");
    let report = read_recording_file(dir.path()).unwrap();
    assert_eq!(report.errors.len(), 1);
    assert!(
        report.errors[0].starts_with("[manifest.json] Invalid JSON: "),
        "{}",
        report.errors[0]
    );
    assert!(report.manifest.is_none());

    std::fs::write(dir.path().join("manifest.json"), [0xff, 0xfe, 0x00]).unwrap();
    let report = read_recording_file(dir.path()).unwrap();
    assert_eq!(report.errors.len(), 1);
    assert!(
        report.errors[0].starts_with("[manifest.json] Could not read file: "),
        "{}",
        report.errors[0]
    );
}

#[test]
fn a_saved_manifest_reads_back_equal_and_pretty_with_the_version_first() {
    let dir = TempDir::new("recording_save");
    let manifest = sample();
    save_recording_manifest(dir.path(), &manifest).unwrap();
    let text = std::fs::read_to_string(dir.path().join("manifest.json")).unwrap();
    assert_eq!(text, recording_manifest_to_json(&manifest));
    assert!(
        text.starts_with("{\n  \"schema_version\": 1,\n"),
        "pretty, the version first: {}",
        &text[..40]
    );
    let report = read_recording_file(dir.path()).unwrap();
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert_eq!(report.manifest.unwrap(), manifest);
}

#[test]
fn saving_into_a_missing_folder_is_an_io_error() {
    let dir = TempDir::new("recording_missing");
    assert!(save_recording_manifest(&dir.path().join("no/such/folder"), &sample()).is_err());
}

/// A generated valid manifest.
fn generated(rng: &mut Lcg) -> RecordingManifest {
    let mut manifest = sample();
    manifest.start_step = rng.next_u64() >> rng.below(40);
    manifest.start_epoch_utc = format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:06}Z",
        2000 + rng.below(60),
        1 + rng.below(12),
        1 + rng.below(28),
        rng.below(24),
        rng.below(60),
        rng.below(60),
        rng.below(1_000_000)
    );
    manifest.keyframe_interval_steps = 1 + rng.below(100_000) as u32;
    let tether = 1 + rng.below(200);
    manifest.bodies = vec![body(0, RecordingBodyRole::Target, "target")];
    manifest
        .bodies
        .push(body(1, RecordingBodyRole::TetherRoot, "root"));
    for i in 0..tether {
        manifest.bodies.push(body(
            2 + i as u32,
            RecordingBodyRole::TetherNode,
            &format!("node {i}"),
        ));
    }
    let tail = manifest.bodies.len() as u32;
    manifest
        .bodies
        .push(body(tail, RecordingBodyRole::TetherTail, "tail"));
    manifest.limits = RecordingLimits {
        // 17 significant digits: the text must give back the same f64.
        max_duration_s: rng.range(1e-3, 1e5),
        max_tether_bodies: (tether + 2) as u32 + rng.below(50) as u32,
        max_size_mb: rng.chance(0.5).then(|| rng.range(1e-3, 1e4)),
    };
    manifest.built_with.app_version = format!("0.{}.{}", rng.below(10), rng.below(10));
    manifest
}

#[test]
fn generated_manifests_survive_a_write_and_a_read_bit_for_bit() {
    const SEED: u64 = 0x5EC0_4D1A_6006;
    const CASES: usize = 300;
    let mut rng = Lcg::new(SEED);
    for case in 0..CASES {
        let manifest = generated(&mut rng);
        let ctx = format!("seed {SEED:#x}, case {case}");
        let text = recording_manifest_to_json(&manifest);
        let value: Value = serde_json::from_str(&text).unwrap();
        let report = read_recording_value(&value);
        assert!(report.errors.is_empty(), "{ctx}: {:?}", report.errors);
        let read = report.manifest.unwrap();
        assert_eq!(read, manifest, "{ctx}");
        assert_eq!(
            read.limits.max_duration_s.to_bits(),
            manifest.limits.max_duration_s.to_bits(),
            "{ctx}: the duration is bit for bit"
        );
        assert_eq!(
            recording_manifest_to_json(&read),
            text,
            "{ctx}: written twice, same text"
        );
    }
    println!("seed {SEED:#x}: {CASES} generated manifests written and read back equal");
}

// ---------------------------------------------------------------------------
// the published JSON Schema
// ---------------------------------------------------------------------------

#[test]
fn recording_schema_file_is_current() {
    check_schema::<RecordingManifest>("recording-manifest.v1.schema.json");
}

#[test]
fn the_recording_schema_requires_the_v1_keys_and_refuses_unknown_ones() {
    let schema = serde_json::to_value(schemars::schema_for!(RecordingManifest)).unwrap();
    let mut required: Vec<&str> = schema["required"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    required.sort();
    assert_eq!(
        required,
        [
            "bodies",
            "built_with",
            "keyframe_interval_steps",
            "limits",
            "plan",
            "plan_id",
            "schema_version",
            "start_epoch_utc",
            "start_step"
        ]
    );
    assert_eq!(schema["additionalProperties"], json!(false));
    assert_eq!(schema["properties"]["schema_version"]["minimum"], json!(1));
    assert_eq!(schema["properties"]["schema_version"]["maximum"], json!(1));
    assert_eq!(
        schema["properties"]["keyframe_interval_steps"]["minimum"],
        json!(1)
    );
}

#[test]
fn the_optional_keys_round_trip_and_an_unfinished_manifest_stays_valid() {
    let mut manifest = sample();
    manifest.project = Some(serde_json::from_value(project_value()).unwrap());
    manifest.source = Some(RecordingSource::Cli);
    manifest.end_step = Some(4400);
    manifest.end_reason = Some(RecordingEndReason::EndSuccess);
    manifest.start_trigger = Some(RecordingStartTrigger::Phase);
    manifest.start_phase = Some("approach".to_string());
    manifest.limits.max_size_mb = Some(100.0);
    let value: Value = serde_json::from_str(&recording_manifest_to_json(&manifest)).unwrap();
    assert_eq!(value["source"], json!("cli"));
    assert_eq!(value["end_reason"], json!("end_success"));
    assert_eq!(value["start_trigger"], json!("phase"));
    assert_eq!(value["limits"]["max_size_mb"], json!(100.0));
    let report = read_recording_value(&value);
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert_eq!(report.manifest.unwrap(), manifest);
    // Without them the document has none of the keys (the rc.1 shape).
    let plain = doc();
    for key in [
        "project",
        "source",
        "end_step",
        "end_reason",
        "start_trigger",
        "start_phase",
    ] {
        assert!(plain.get(key).is_none(), "{key}");
    }
    assert!(plain["limits"].get("max_size_mb").is_none());
}

#[test]
fn a_wrong_typed_project_is_located() {
    let mut value = doc();
    value["project"] = json!({ "schema_version": 1, "name": 5 });
    let report = read_recording_value(&value);
    assert_eq!(report.errors.len(), 1, "{:?}", report.errors);
    assert!(
        report.errors[0].starts_with("[manifest.json] /project: "),
        "{:?}",
        report.errors
    );
}
