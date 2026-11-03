//! `ssg-sim run <project-dir>` from `project.json` (design D2, D9; P6 S3): a project
//! whose manifest holds the Q13 scenario runs bit-identically to the flag run the
//! determinism tests use, and the flags override the manifest.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{Value, json};
use ssg_sim::test_support::q13;

/// The Q13 fixture's GP epoch, as in the harness (`fixed_epoch()`).
const EPOCH: &str = "2026-04-03T16:03:47.002464";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn temp_project(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "ssg-sim-cli-manifest-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The flag run's project: the Q13 fixture as `datasets/q13.json`, no `project.json`.
fn flag_project(name: &str) -> PathBuf {
    let dir = temp_project(name);
    std::fs::create_dir_all(dir.join("datasets")).unwrap();
    std::fs::copy(
        repo_root().join("crates/ssg-sim/src/test_support/fixtures/q13_hubble_pair.json"),
        dir.join("datasets/q13.json"),
    )
    .unwrap();
    dir
}

/// A project that holds the Q13 scenario in `project.json` only (GP snapshots, the
/// start epoch, the example plan) and no dataset file.
fn manifest_project(name: &str) -> PathBuf {
    let dir = temp_project(name);
    let record = |id| serde_json::to_value(q13::fixture_record(id)).unwrap();
    let manifest = json!({
        "schema_version": 1,
        "name": "Q13",
        "start_epoch_utc": format!("{EPOCH}Z"),
        "active_plan": "example_plan",
        "target": { "label": "HST", "orbit": { "gp": record(q13::HST_NORAD_ID) } },
        "chaser": { "label": "P0 HST CHASER", "orbit": { "gp": record(q13::CHASER_NORAD_ID) } },
    });
    std::fs::write(
        dir.join("project.json"),
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();
    dir
}

fn ssg_sim(project: &Path, out: &str, extra: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ssg-sim"));
    command
        .arg("run")
        .arg(project)
        .args(extra)
        .arg("--assets")
        .arg(repo_root().join("assets"))
        .arg("--out")
        .arg(project.join(out));
    command
}

#[track_caller]
fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "exit {:?}: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn outputs(project: &Path, out: &str) -> (Vec<u8>, Value) {
    let dir = project.join(out);
    let csv = std::fs::read(dir.join("trajectory.csv")).unwrap();
    let summary = serde_json::from_str(&std::fs::read_to_string(dir.join("summary.json")).unwrap())
        .expect("summary.json parses");
    (csv, summary)
}

fn without_wall(mut summary: Value) -> Value {
    summary
        .as_object_mut()
        .unwrap()
        .remove("wall_s")
        .expect("summary.json has wall_s");
    summary
}

/// The manifest run takes everything from `project.json`; the flag run gets the same
/// from flags and a dataset file. Their outputs are byte-identical.
#[test]
fn manifest_run_equals_flag_run() {
    let flags = flag_project("flags");
    let manifest = manifest_project("manifest");

    let flag_output = ssg_sim(
        &flags,
        "out",
        &[
            "--target", "20580", "--chaser", "99901", "--epoch", EPOCH, "--steps", "6400",
        ],
    )
    .output()
    .unwrap();
    assert_success(&flag_output);
    let manifest_output = ssg_sim(&manifest, "out", &["--steps", "6400"])
        .output()
        .unwrap();
    assert_success(&manifest_output);

    let (flag_csv, flag_summary) = outputs(&flags, "out");
    let (manifest_csv, manifest_summary) = outputs(&manifest, "out");
    assert_eq!(flag_csv.len(), manifest_csv.len(), "trajectory.csv length");
    assert!(flag_csv == manifest_csv, "trajectory.csv bytes differ");
    assert_eq!(
        without_wall(flag_summary.clone()),
        without_wall(manifest_summary)
    );
    assert_eq!(flag_summary["epoch_mode"], "fixed");
    assert_eq!(flag_summary["target"], 20580);
    assert!(
        String::from_utf8_lossy(&manifest_output.stderr).is_empty(),
        "no flag, nothing overridden: {}",
        String::from_utf8_lossy(&manifest_output.stderr)
    );
    let _ = std::fs::remove_dir_all(&flags);
    let _ = std::fs::remove_dir_all(&manifest);
}

#[test]
fn flags_override_the_manifest_and_say_so() {
    let project = manifest_project("override");
    // A second epoch, one second later; the run is short.
    let later = "2026-04-03T16:03:48.002464";

    let output = ssg_sim(
        &project,
        "out",
        &["--steps", "64", "--epoch", later, "--plan", "example_plan"],
    )
    .output()
    .unwrap();

    assert_success(&output);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("ssg-sim: --epoch overrides project.json"),
        "{stderr}"
    );
    assert!(
        stderr.contains("ssg-sim: --plan overrides project.json"),
        "{stderr}"
    );
    assert!(!stderr.contains("--target"), "{stderr}");
    let (_, summary) = outputs(&project, "out");
    assert_eq!(
        summary["epoch"], "2026-04-03T16:03:48.002464Z",
        "the flag's epoch is used"
    );
    assert_eq!(summary["epoch_mode"], "fixed");
    let _ = std::fs::remove_dir_all(&project);
}

#[test]
fn a_target_flag_replaces_only_the_target() {
    let project = manifest_project("override-target");
    std::fs::create_dir_all(project.join("datasets")).unwrap();
    std::fs::copy(
        repo_root().join("crates/ssg-sim/src/test_support/fixtures/q13_hubble_pair.json"),
        project.join("datasets/q13.json"),
    )
    .unwrap();

    let output = ssg_sim(&project, "out", &["--steps", "64", "--target", "20580"])
        .output()
        .unwrap();

    assert_success(&output);
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("ssg-sim: --target overrides project.json")
    );
    let (_, summary) = outputs(&project, "out");
    assert_eq!(summary["target"], 20580);
    assert_eq!(
        summary["chaser"], 99901,
        "the chaser still comes from project.json"
    );
    let _ = std::fs::remove_dir_all(&project);
}

#[test]
fn without_a_manifest_the_ids_are_required() {
    let project = flag_project("no-manifest");
    for (flags, message) in [
        (vec![], "missing --target"),
        (vec!["--target", "20580"], "missing --chaser"),
    ] {
        let output = ssg_sim(&project, "out", &flags).output().unwrap();
        assert_eq!(output.status.code(), Some(2), "{flags:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(message),
            "{flags:?}"
        );
    }
    let _ = std::fs::remove_dir_all(&project);
}

#[test]
fn an_invalid_manifest_stops_the_run_with_its_errors() {
    let project = manifest_project("invalid");
    let mut manifest: Value =
        serde_json::from_str(&std::fs::read_to_string(project.join("project.json")).unwrap())
            .unwrap();
    manifest["chaser"]["mass_kg"] = json!(-1.0);
    std::fs::write(project.join("project.json"), manifest.to_string()).unwrap();

    let output = ssg_sim(&project, "out", &["--steps", "64"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("ssg-sim: [project.json] /chaser/mass_kg: must be greater than 0"),
        "{stderr}"
    );
    let _ = std::fs::remove_dir_all(&project);
}

#[test]
fn a_manifest_with_an_unknown_active_plan_names_it() {
    let project = manifest_project("no-plan");
    let mut manifest: Value =
        serde_json::from_str(&std::fs::read_to_string(project.join("project.json")).unwrap())
            .unwrap();
    manifest["active_plan"] = json!("nope");
    std::fs::write(project.join("project.json"), manifest.to_string()).unwrap();

    let output = ssg_sim(&project, "out", &["--steps", "64"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("capture plan `nope` not found"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let _ = std::fs::remove_dir_all(&project);
}

/// The `warning:` lines `ssg-sim validate` prints for `project`.
fn validate_warnings(project: &Path) -> Vec<String> {
    let output = Command::new(env!("CARGO_BIN_EXE_ssg-sim"))
        .arg("validate")
        .arg(project)
        .arg("--assets")
        .arg(repo_root().join("assets"))
        .output()
        .unwrap();
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.strip_prefix("warning: "))
        .map(str::to_string)
        .collect()
}

/// The `ssg-sim: warning:` lines of a run's stderr, without the prefix.
fn run_warnings(output: &Output) -> Vec<String> {
    String::from_utf8_lossy(&output.stderr)
        .lines()
        .filter_map(|line| line.strip_prefix("ssg-sim: warning: "))
        .map(str::to_string)
        .collect()
}

/// F-56: a run prints the warnings of the plan it runs (here the migration warnings
/// of a v0 plan with unmapped values) as `validate` words them, and the outputs are
/// byte-identical to the run of the same plan without those values.
#[test]
fn a_run_prints_the_plan_warnings_validate_prints() {
    let fixture = repo_root().join("crates/ssg-sim/tests/fixtures/v0/q10_ignored_fields.json");
    let warned = flag_project("plan-warned");
    std::fs::copy(&fixture, warned.join("q10_ignored_fields.json")).unwrap();
    // The same plan without every key the migration reports as unmapped.
    let clean = flag_project("plan-clean");
    let mut plan: Value =
        serde_json::from_str(&std::fs::read_to_string(&fixture).unwrap()).unwrap();
    for key in ["chaser", "target", "start_epoch"] {
        plan.as_object_mut().unwrap().remove(key);
    }
    for key in ["damping", "segment_mass", "segments"] {
        plan["device"].as_object_mut().unwrap().remove(key);
    }
    plan["states"][0]["parameters"]
        .as_object_mut()
        .unwrap()
        .remove("kp");
    plan["states"][0]["transitions"] = json!([]);
    plan["states"][1]["transitions"][0]["distance"]
        .as_object_mut()
        .unwrap()
        .remove("units");
    std::fs::write(clean.join("q10_ignored_fields.json"), plan.to_string()).unwrap();
    let flags = [
        "--target", "20580", "--chaser", "99901", "--epoch", EPOCH, "--steps", "640",
    ];

    let expected = validate_warnings(&warned);
    assert!(!expected.is_empty(), "the fixture has migration warnings");
    assert!(
        validate_warnings(&clean).is_empty(),
        "the stripped plan has none"
    );
    let warned_output = ssg_sim(&warned, "out", &flags).output().unwrap();
    let clean_output = ssg_sim(&clean, "out", &flags).output().unwrap();

    assert_success(&warned_output);
    assert_success(&clean_output);
    assert_eq!(
        run_warnings(&warned_output),
        expected,
        "same lines, same order"
    );
    assert!(run_warnings(&clean_output).is_empty());
    assert_eq!(
        String::from_utf8_lossy(&warned_output.stdout)
            .lines()
            .count(),
        1,
        "stdout keeps its one summary line"
    );
    let (warned_csv, warned_summary) = outputs(&warned, "out");
    let (clean_csv, clean_summary) = outputs(&clean, "out");
    assert!(warned_csv == clean_csv, "trajectory.csv bytes differ");
    assert_eq!(without_wall(warned_summary), without_wall(clean_summary));
    let _ = std::fs::remove_dir_all(&warned);
    let _ = std::fs::remove_dir_all(&clean);
}

/// F-56: a run prints the warnings of `project.json` (here a dataset that does not
/// exist) as `validate` words them, and the outputs equal those of the run without.
#[test]
fn a_run_prints_the_project_json_warnings_validate_prints() {
    let warned = manifest_project("manifest-warned");
    let mut manifest: Value =
        serde_json::from_str(&std::fs::read_to_string(warned.join("project.json")).unwrap())
            .unwrap();
    manifest["target"]["source"] = json!({ "dataset": "datasets/gone.json", "norad_id": 20580 });
    std::fs::write(warned.join("project.json"), manifest.to_string()).unwrap();
    let clean = manifest_project("manifest-clean");

    let expected = validate_warnings(&warned);
    assert_eq!(
        expected,
        [
            "[project.json] /target/source/dataset: `datasets/gone.json` does not exist (the stored orbit is used)"
        ]
    );
    let warned_output = ssg_sim(&warned, "out", &["--steps", "640"])
        .output()
        .unwrap();
    let clean_output = ssg_sim(&clean, "out", &["--steps", "640"])
        .output()
        .unwrap();

    assert_success(&warned_output);
    assert_success(&clean_output);
    assert_eq!(run_warnings(&warned_output), expected);
    assert!(run_warnings(&clean_output).is_empty());
    let (warned_csv, warned_summary) = outputs(&warned, "out");
    let (clean_csv, clean_summary) = outputs(&clean, "out");
    assert!(warned_csv == clean_csv, "trajectory.csv bytes differ");
    assert_eq!(without_wall(warned_summary), without_wall(clean_summary));
    let _ = std::fs::remove_dir_all(&warned);
    let _ = std::fs::remove_dir_all(&clean);
}
