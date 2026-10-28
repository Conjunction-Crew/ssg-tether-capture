//! Runs the built `ssg-sim` binary. Platform-independent: it checks structure and
//! the first transition, not the fixed-epoch goldens (those are macOS/aarch64).

use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn run_writes_summary_and_trajectory() {
    let root = repo_root();
    let project = tempdir("project");
    std::fs::create_dir_all(project.join("datasets")).unwrap();
    std::fs::copy(
        root.join("crates/ssg-sim/src/test_support/fixtures/q13_hubble_pair.json"),
        project.join("datasets/q13.json"),
    )
    .unwrap();
    let out = project.join("out");

    let output = Command::new(env!("CARGO_BIN_EXE_ssg-sim"))
        .arg("run")
        .arg(&project)
        .args(["--target", "20580", "--chaser", "99901"])
        .args(["--epoch", "2026-04-03T16:03:47.002464"])
        .args(["--steps", "6400"])
        .arg("--assets")
        .arg(root.join("assets"))
        .arg("--out")
        .arg(&out)
        .output()
        .expect("the binary runs");
    assert!(
        output.status.success(),
        "exit {:?}: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );

    let summary: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(out.join("summary.json")).unwrap())
            .expect("summary.json parses");
    assert_eq!(summary["transitions"][0]["from"], "rendezvous");
    assert_eq!(summary["transitions"][0]["to"], "approach");
    assert_eq!(summary["final"]["state"], "approach");
    // `--steps` counts fixed steps (A-12); the sampled state is hashed.
    assert_eq!(summary["steps"], 6400);
    let hash = summary["state_hash"]
        .as_str()
        .expect("state_hash is a string");
    assert!(
        hash.len() == 16 && hash.chars().all(|c| c.is_ascii_hexdigit()),
        "state_hash `{hash}` is 16 hex digits"
    );
    let csv = std::fs::read_to_string(out.join("trajectory.csv")).unwrap();
    assert_eq!(csv.lines().count(), 101, "header + 6400 / 64 samples");
    assert_eq!(
        csv.lines().next(),
        Some("step,epoch,state,range_m,rel_speed_mps")
    );
    let _ = std::fs::remove_dir_all(&project);
}

#[test]
fn no_arguments_prints_usage_and_exits_2() {
    let output = Command::new(env!("CARGO_BIN_EXE_ssg-sim"))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("usage: ssg-sim run <project-dir>"));
}

#[test]
fn unknown_option_exits_2() {
    let output = Command::new(env!("CARGO_BIN_EXE_ssg-sim"))
        .args(["run", "somewhere", "--bogus"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
}

/// `ssg-sim run <project> --target T --chaser C <extra>`, run to the end.
fn run_cli(project: &Path, ids: (&str, &str), extra: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ssg-sim"))
        .arg("run")
        .arg(project)
        .args(["--target", ids.0, "--chaser", ids.1])
        .args(extra)
        .output()
        .expect("the binary runs")
}

#[test]
fn malformed_epoch_exits_2_and_names_the_expected_format() {
    let output = run_cli(
        Path::new("somewhere"),
        ("1", "2"),
        &["--epoch", "yesterday"],
    );

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("ssg-sim: --epoch: `yesterday` is not `now` or YYYY-MM-DDTHH:MM:SS[.fff]"),
        "stderr: {stderr}"
    );
}

#[test]
fn unknown_norad_id_exits_1_and_names_the_id() {
    let project = tempdir("unknown-id");
    let output = run_cli(
        &project,
        ("424242", "99901"),
        &[
            "--assets",
            repo_root().join("assets").to_str().unwrap(),
            "--steps",
            "64",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("ssg-sim: NORAD 424242 not found in any dataset"),
        "stderr: {stderr}"
    );
    let _ = std::fs::remove_dir_all(&project);
}

#[test]
fn project_without_datasets_exits_1_when_the_assets_have_none_either() {
    let project = tempdir("empty-project");
    let assets = tempdir("empty-assets");
    let output = run_cli(
        &project,
        ("20580", "99901"),
        &["--assets", assets.to_str().unwrap(), "--steps", "64"],
    );

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("ssg-sim: NORAD 20580 not found in any dataset"),
        "stderr: {stderr}"
    );
    assert!(
        !project.join("runs").exists(),
        "no output folder is written"
    );
    let _ = std::fs::remove_dir_all(&project);
    let _ = std::fs::remove_dir_all(&assets);
}

#[test]
fn stop_at_capture_without_a_capture_exits_1_with_the_final_state() {
    let root = repo_root();
    let project = tempdir("no-capture");
    std::fs::create_dir_all(project.join("datasets")).unwrap();
    std::fs::copy(
        root.join("crates/ssg-sim/src/test_support/fixtures/q13_hubble_pair.json"),
        project.join("datasets/q13.json"),
    )
    .unwrap();
    let out = project.join("out");

    let output = run_cli(
        &project,
        ("20580", "99901"),
        &[
            "--epoch",
            "2026-04-03T16:03:47.002464",
            "--steps",
            "64",
            "--stop-at-capture",
            "--assets",
            root.join("assets").to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("ssg-sim: no capture within 64 steps (final state `rendezvous`)"),
        "stderr: {stderr}"
    );
    assert!(!out.exists(), "a failed run writes no output");
    let _ = std::fs::remove_dir_all(&project);
}

fn tempdir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ssg-sim-cli-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}
