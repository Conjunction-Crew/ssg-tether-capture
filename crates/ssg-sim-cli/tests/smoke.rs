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

fn tempdir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ssg-sim-cli-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}
