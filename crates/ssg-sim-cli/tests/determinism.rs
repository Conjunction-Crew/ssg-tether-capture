//! The determinism contract, checked on the built `ssg-sim` binary (F-17, P4 S6):
//! the same build, platform, project and inputs give bit-identical output, and the
//! time warp does not change the result. Platform-independent: each test compares
//! runs with each other, never with the macOS goldens.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// The Q13 fixture's GP epoch, as in the harness (`fixed_epoch()`).
const EPOCH: &str = "2026-04-03T16:03:47.002464";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A temporary project holding the Q13 fixture as `datasets/q13.json`.
fn q13_project(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "ssg-sim-cli-determinism-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("datasets")).unwrap();
    std::fs::copy(
        repo_root().join("crates/ssg-sim/src/test_support/fixtures/q13_hubble_pair.json"),
        dir.join("datasets/q13.json"),
    )
    .unwrap();
    dir
}

/// `ssg-sim run <project> --target 20580 --chaser 99901 --epoch EPOCH <extra>
/// --out <project>/<out>`, not yet started.
fn command(project: &Path, out: &str, extra: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ssg-sim"));
    command
        .arg("run")
        .arg(project)
        .args(["--target", "20580", "--chaser", "99901", "--epoch", EPOCH])
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

/// `trajectory.csv` bytes and `summary.json` of one run's output folder.
fn outputs(project: &Path, out: &str) -> (Vec<u8>, serde_json::Value) {
    let dir = project.join(out);
    let csv = std::fs::read(dir.join("trajectory.csv")).unwrap();
    let summary = serde_json::from_str(&std::fs::read_to_string(dir.join("summary.json")).unwrap())
        .expect("summary.json parses");
    (csv, summary)
}

/// `summary.json` without the wall-clock time, the one field that may differ.
fn without_wall(mut summary: serde_json::Value) -> serde_json::Value {
    summary
        .as_object_mut()
        .expect("summary.json is an object")
        .remove("wall_s")
        .expect("summary.json has wall_s");
    summary
}

#[test]
fn cli_two_runs_bit_identical() {
    let project = q13_project("two-runs");
    for out in ["a", "b"] {
        assert_success(
            &command(&project, out, &["--steps", "6400"])
                .output()
                .unwrap(),
        );
    }
    let (csv_a, summary_a) = outputs(&project, "a");
    let (csv_b, summary_b) = outputs(&project, "b");

    assert_eq!(csv_a.len(), csv_b.len(), "trajectory.csv length");
    assert!(csv_a == csv_b, "trajectory.csv bytes differ");
    assert_eq!(without_wall(summary_a), without_wall(summary_b));
    let _ = std::fs::remove_dir_all(&project);
}

#[test]
fn cli_warp_16_equals_warp_1() {
    let project = q13_project("warp");
    for (out, warp) in [("x1", "1"), ("x16", "16")] {
        let args = ["--steps", "6400", "--sample-every", "64", "--warp", warp];
        assert_success(&command(&project, out, &args).output().unwrap());
    }
    let (csv_1, summary_1) = outputs(&project, "x1");
    let (csv_16, summary_16) = outputs(&project, "x16");

    // Precondition: the horizon reaches the first transition.
    assert_eq!(summary_1["transitions"][0]["to"], "approach");
    assert!(csv_1 == csv_16, "trajectory.csv bytes differ at warp 16");
    assert_eq!(summary_1["transitions"], summary_16["transitions"]);
    assert_eq!(summary_1["state_hash"], summary_16["state_hash"]);
    assert_eq!(summary_1["steps"], summary_16["steps"]);
    let _ = std::fs::remove_dir_all(&project);
}

#[test]
fn cli_steps_must_be_multiple_of_warp() {
    let output = Command::new(env!("CARGO_BIN_EXE_ssg-sim"))
        .args(["run", "somewhere", "--target", "1", "--chaser", "2"])
        .args(["--steps", "100", "--warp", "16"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("--steps must be a multiple of --warp")
    );
}

/// The P0 capture scenario to `capture`: three runs at warp 1 and one at warp 16
/// end in bit-identical output. `--steps 163840` without `--stop-at-capture`, so
/// all four runs cover the same steps whatever the warp. The four processes run
/// at the same time.
#[test]
#[ignore = "P4 determinism measurement (~10 min): cargo test -p ssg-sim-cli --locked cli_three_runs_to_capture_identical -- --ignored --nocapture"]
fn cli_three_runs_to_capture_identical() {
    let project = q13_project("to-capture");
    let runs = [("w1-a", "1"), ("w1-b", "1"), ("w1-c", "1"), ("w16", "16")];
    let children: Vec<_> = runs
        .iter()
        .map(|(out, warp)| {
            command(&project, out, &["--steps", "163840", "--warp", warp])
                .spawn()
                .expect("the binary starts")
        })
        .collect();
    for child in children {
        assert_success(&child.wait_with_output().unwrap());
    }

    let results: Vec<_> = runs.iter().map(|(out, _)| outputs(&project, out)).collect();
    for ((out, _), (_, summary)) in runs.iter().zip(&results) {
        println!("{out}: {summary}");
    }
    let (csv_0, summary_0) = &results[0];
    assert_eq!(
        summary_0["final"]["state"], "capture",
        "the scenario reaches capture"
    );
    for ((out, _), (csv, summary)) in runs.iter().zip(&results).skip(1) {
        assert!(csv == csv_0, "{out}: trajectory.csv differs from w1-a");
        assert_eq!(
            summary["state_hash"], summary_0["state_hash"],
            "{out}: state_hash"
        );
        assert_eq!(
            summary["transitions"], summary_0["transitions"],
            "{out}: transitions"
        );
        assert_eq!(summary["final"], summary_0["final"], "{out}: final sample");
    }
    let _ = std::fs::remove_dir_all(&project);
}
