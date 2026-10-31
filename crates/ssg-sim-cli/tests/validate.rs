//! `ssg-sim validate <project-dir>` (design D9; P6 S3): exact output lines and exit
//! codes 0 / 1 / 2 on test projects.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{Value, json};
use ssg_sim::test_support::q13;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn temp_project(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "ssg-sim-cli-validate-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A v1 plan with one phase.
fn plan_v1() -> String {
    json!({
        "schema_version": 1, "name": "Mine", "tether": "Tether1",
        "phases": [ { "id": "only", "max_velocity_mps": 1.0, "max_force_n": 1.0 } ]
    })
    .to_string()
}

/// A valid manifest: GP snapshot target, elements chaser, written by this build.
fn manifest() -> Value {
    json!({
        "schema_version": 1,
        "name": "Test project",
        "start_epoch_utc": "2026-04-03T16:03:47.002464Z",
        "active_plan": "mine",
        "target": { "label": "HST", "orbit": { "gp": serde_json::to_value(q13::fixture_record(q13::HST_NORAD_ID)).unwrap() } },
        "chaser": { "label": "Chaser", "orbit": { "elements": {
            "semi_major_axis_m": 6914000.0, "eccentricity": 0.0002, "inclination_rad": 0.4969,
            "raan_rad": 1.747, "arg_perigee_rad": 1.4, "mean_anomaly_rad": 4.88,
            "epoch_utc": "2026-04-03T16:03:47.002464Z" } } },
        "built_with": serde_json::to_value(ssg_sim::project::manifest::BuiltWith::current()).unwrap(),
    })
}

fn write_manifest(dir: &Path, manifest: &Value) {
    std::fs::write(
        dir.join("project.json"),
        serde_json::to_string_pretty(manifest).unwrap(),
    )
    .unwrap();
}

/// A v1 project: `project.json` and one plan; nothing to report.
fn valid_project(name: &str) -> PathBuf {
    let dir = temp_project(name);
    write_manifest(&dir, &manifest());
    std::fs::write(dir.join("mine.json"), plan_v1()).unwrap();
    dir
}

fn validate(project: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ssg-sim"))
        .arg("validate")
        .arg(project)
        .arg("--assets")
        .arg(repo_root().join("assets"))
        .output()
        .unwrap()
}

fn stdout_lines(output: &Output) -> Vec<String> {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::to_string)
        .collect()
}

#[test]
fn a_valid_v1_project_has_nothing_to_report() {
    let project = valid_project("valid");

    let output = validate(&project);

    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(stdout_lines(&output), ["0 errors, 0 warnings"]);
    let _ = std::fs::remove_dir_all(&project);
}

#[test]
fn a_v0_folder_loads_with_its_migration_warnings() {
    let project = temp_project("v0");
    std::fs::copy(
        repo_root().join("crates/ssg-sim/tests/fixtures/v0/q10_ignored_fields.json"),
        project.join("q10_ignored_fields.json"),
    )
    .unwrap();

    let output = validate(&project);

    assert_eq!(output.status.code(), Some(0));
    let lines = stdout_lines(&output);
    assert_eq!(
        lines[0],
        "info: no project.json: the folder is read as a v0 project (plans only)"
    );
    let warnings: Vec<_> = lines
        .iter()
        .filter(|l| l.starts_with("warning: [q10_ignored_fields] "))
        .collect();
    assert!(
        !warnings.is_empty(),
        "migration warnings name the plan: {lines:?}"
    );
    assert_eq!(
        lines.last().unwrap(),
        &format!("0 errors, {} warnings", warnings.len())
    );
    let _ = std::fs::remove_dir_all(&project);
}

#[test]
fn a_broken_manifest_and_a_broken_plan_are_errors() {
    let project = temp_project("broken");
    std::fs::write(project.join("project.json"), "{ not json").unwrap();
    std::fs::write(
        project.join("bad_plan.json"),
        r#"{"schema_version": 1, "name": "", "tether": "T", "phases": []}"#,
    )
    .unwrap();

    let output = validate(&project);

    assert_eq!(output.status.code(), Some(1));
    let lines = stdout_lines(&output);
    assert!(
        lines[0].starts_with("error: [project.json] Invalid JSON: "),
        "{lines:?}"
    );
    assert!(
        lines.contains(&"error: [bad_plan] /name: must not be blank".to_string()),
        "{lines:?}"
    );
    assert!(
        lines.contains(&"error: [bad_plan] /phases: at least one phase is required".to_string()),
        "{lines:?}"
    );
    assert_eq!(lines.last().unwrap(), "3 errors, 0 warnings");
    let _ = std::fs::remove_dir_all(&project);
}

#[test]
fn an_unknown_key_is_an_error_naming_the_key() {
    let project = valid_project("unknown-key");
    let mut manifest = manifest();
    manifest["colour"] = json!("red");
    write_manifest(&project, &manifest);

    let output = validate(&project);

    assert_eq!(output.status.code(), Some(1));
    let lines = stdout_lines(&output);
    assert!(
        lines[0].starts_with("error: [project.json] /: unknown field `colour`"),
        "{lines:?}"
    );
    assert_eq!(lines.last().unwrap(), "1 errors, 0 warnings");
    let _ = std::fs::remove_dir_all(&project);
}

#[test]
fn a_missing_active_plan_is_an_error() {
    let project = valid_project("missing-plan");
    let mut manifest = manifest();
    manifest["active_plan"] = json!("nope");
    write_manifest(&project, &manifest);

    let output = validate(&project);

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        stdout_lines(&output),
        [
            "error: [project.json] /active_plan: no plan `nope` in the project or the example plans",
            "1 errors, 0 warnings"
        ]
    );
    let _ = std::fs::remove_dir_all(&project);
}

#[test]
fn the_active_plan_may_be_an_example_plan() {
    let project = valid_project("example-plan");
    let mut manifest = manifest();
    manifest["active_plan"] = json!("example_plan");
    write_manifest(&project, &manifest);

    let output = validate(&project);

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(stdout_lines(&output), ["0 errors, 0 warnings"]);
    let _ = std::fs::remove_dir_all(&project);
}

#[test]
fn a_field_that_is_not_supported_yet_is_an_error() {
    let project = valid_project("unsupported");
    let mut manifest = manifest();
    manifest["chaser"]["orbit"] =
        json!({ "relative_to_target": { "preset": "v_bar", "offset_m": 100.0 } });
    // Applied since P7.5 (Q10 row 11): an inertia tensor and the chaser's mass
    // are no longer errors.
    manifest["target"]["inertia_kg_m2"] =
        json!([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);
    manifest["chaser"]["mass_kg"] = json!(500.0);
    write_manifest(&project, &manifest);

    let output = validate(&project);

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        stdout_lines(&output),
        [
            "error: [project.json] /chaser/orbit/relative_to_target: an orbit relative to the target is not supported yet; it is applied from v0.4.0-rc.5",
            "1 errors, 0 warnings"
        ]
    );
    let _ = std::fs::remove_dir_all(&project);
}

#[test]
fn a_missing_dataset_is_a_warning_and_another_build_is_info() {
    let project = valid_project("warnings");
    let mut manifest = manifest();
    manifest["target"]["source"] = json!({ "dataset": "datasets/gone.json", "norad_id": 20580 });
    manifest["built_with"] = json!({ "app_version": "0.0.1", "target": "nowhere-os", "avian_features": ["simd", "parallel"] });
    write_manifest(&project, &manifest);

    let output = validate(&project);

    assert_eq!(
        output.status.code(),
        Some(0),
        "warnings and notes do not fail the run"
    );
    let lines = stdout_lines(&output);
    assert_eq!(
        lines[0],
        "warning: [project.json] /target/source/dataset: `datasets/gone.json` does not exist (the stored orbit is used)"
    );
    assert!(
        lines[1].starts_with("info: [project.json] /built_with: written by app version 0.0.1"),
        "{lines:?}"
    );
    assert_eq!(lines.last().unwrap(), "0 errors, 1 warnings");
    let _ = std::fs::remove_dir_all(&project);
}

#[test]
fn a_folder_without_plans_says_so() {
    let project = temp_project("no-plans");
    let mut manifest = manifest();
    manifest["active_plan"] = json!("example_plan");
    write_manifest(&project, &manifest);

    let output = validate(&project);

    let lines = stdout_lines(&output);
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("info: no plan files in the project folder")),
        "{lines:?}"
    );
    let _ = std::fs::remove_dir_all(&project);
}

#[test]
fn usage_errors_exit_2() {
    let missing = std::env::temp_dir().join("ssg-sim-cli-validate-does-not-exist");
    for (args, message) in [
        (vec!["validate"], "missing <project-dir>"),
        (vec!["validate", "--bogus"], "unknown option `--bogus`"),
        (vec!["validate", "a", "b"], "unexpected argument `b`"),
        (vec!["validate", "a", "--assets"], "--assets needs a value"),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_ssg-sim"))
            .args(&args)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(message),
            "{args:?}"
        );
    }
    let output = Command::new(env!("CARGO_BIN_EXE_ssg-sim"))
        .arg("validate")
        .arg(&missing)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("is not a directory"));
}

/// A project with the Q13 fixture as a dataset and one 200-step recording `r`
/// (frames at steps 2, 34, … 194 and 202).
fn recorded_project(name: &str) -> PathBuf {
    let project = temp_project(name);
    std::fs::create_dir_all(project.join("datasets")).unwrap();
    std::fs::copy(
        repo_root().join("crates/ssg-sim/src/test_support/fixtures/q13_hubble_pair.json"),
        project.join("datasets/q13.json"),
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_ssg-sim"))
        .arg("run")
        .arg(&project)
        .args(["--target", "20580", "--chaser", "99901"])
        .args(["--epoch", "2026-04-03T16:03:47.002464"])
        .args(["--steps", "200", "--record", "r"])
        .arg("--assets")
        .arg(repo_root().join("assets"))
        .arg("--out")
        .arg(project.join("out"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    project
}

#[test]
fn a_recording_is_checked_and_summarised() {
    let project = recorded_project("recording_ok");

    let output = validate(&project);

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        stdout_lines(&output),
        [
            "info: no project.json: the folder is read as a v0 project (plans only)",
            "info: no plan files in the project folder (the example plans are used)",
            "info: recordings/r: 8 frames, 193 bodies, steps 2..202",
            "0 errors, 0 warnings",
        ]
    );
    let _ = std::fs::remove_dir_all(&project);
}

#[test]
fn a_broken_recording_is_an_error_and_an_unfinished_one_a_warning() {
    let project = recorded_project("recording_broken");
    std::fs::remove_file(project.join("recordings/r/keyframes.parquet")).unwrap();
    std::fs::create_dir_all(project.join("recordings/crashed.partial")).unwrap();

    let output = validate(&project);

    assert_eq!(output.status.code(), Some(1));
    let lines = stdout_lines(&output);
    assert_eq!(
        lines[2..],
        [
            "warning: recordings/crashed.partial: unfinished (an interrupted run or a crash); \
             its Parquet file has no footer and cannot be recovered: make the recording again",
            "error: [recordings/r/keyframes.parquet] missing",
            "1 errors, 1 warnings",
        ]
    );
    let _ = std::fs::remove_dir_all(&project);
}

#[test]
fn the_recording_block_is_checked_against_the_active_plan() {
    let project = valid_project("recording_block");
    let mut value = manifest();
    value["recording"] = json!({ "auto_start_phase": "approach", "keyframe_interval_steps": 16 });
    write_manifest(&project, &value);

    let output = validate(&project);

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        stdout_lines(&output),
        [
            "warning: [project.json] /recording/auto_start_phase: no phase `approach` in plan \
             `mine` (its phases: only); the recording would never start",
            "0 errors, 1 warnings",
        ]
    );
    // A phase of the plan is fine; a wrong number is an error.
    value["recording"] = json!({ "auto_start_phase": "only", "max_size_mb": 0.0 });
    write_manifest(&project, &value);
    let output = validate(&project);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        stdout_lines(&output),
        [
            "error: [project.json] /recording/max_size_mb: must be greater than 0",
            "1 errors, 0 warnings",
        ]
    );
    value["recording"] = json!({ "auto_start_phase": "only" });
    write_manifest(&project, &value);
    assert_eq!(stdout_lines(&validate(&project)), ["0 errors, 0 warnings"]);
    let _ = std::fs::remove_dir_all(&project);
}
