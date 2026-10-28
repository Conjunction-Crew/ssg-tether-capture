//! F-51: the CLI and the test harness run the same Q13 scenario, offset by one
//! step. The CLI measures from step 2 (three updates, the capture command applied
//! in the third), `q13::build` from step 3 (it ticks once more before the capture
//! starts), so a harness checkpoint at tick `t` and the CLI sample with the same
//! absolute `step` are the same trajectory point. This test checks that they are
//! bit-identical, on whatever platform it runs: it compares two code paths with
//! each other and never with the macOS goldens.

use std::path::{Path, PathBuf};
use std::process::Command;

use ssg_sim::test_support::q13;

/// The Q13 fixture's GP epoch, as in the harness (`fixed_epoch()`).
const EPOCH: &str = "2026-04-03T16:03:47.002464";
/// Harness ticks. `q13::run` samples every 640 ticks, the last at tick 6,400
/// (step 6,403): 10 checkpoints.
const HARNESS_TICKS: u64 = 6_400;
/// CLI steps for the same horizon: its first sample is step 3 and `--steps 6401`
/// ends at step 6,403.
const CLI_STEPS: &str = "6401";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// One `trajectory.csv` row: `step,epoch,state,range_m,rel_speed_mps`.
struct Row {
    step: u64,
    state: String,
    range_m: f64,
}

fn parse_rows(csv: &str) -> Vec<Row> {
    let mut lines = csv.lines();
    assert_eq!(
        lines.next(),
        Some("step,epoch,state,range_m,rel_speed_mps"),
        "trajectory.csv header"
    );
    lines
        .map(|line| {
            let fields: Vec<&str> = line.split(',').collect();
            assert_eq!(fields.len(), 5, "row `{line}`");
            Row {
                step: fields[0].parse().expect("step"),
                state: fields[2].to_string(),
                range_m: fields[3].parse().expect("range_m"),
            }
        })
        .collect()
}

#[test]
fn cli_trajectory_equals_the_harness_checkpoints_bit_for_bit() {
    let project = std::env::temp_dir().join(format!("ssg-sim-cli-parity-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&project);
    std::fs::create_dir_all(project.join("datasets")).unwrap();
    std::fs::copy(
        repo_root().join("crates/ssg-sim/src/test_support/fixtures/q13_hubble_pair.json"),
        project.join("datasets/q13.json"),
    )
    .unwrap();
    let out = project.join("out");

    let output = Command::new(env!("CARGO_BIN_EXE_ssg-sim"))
        .arg("run")
        .arg(&project)
        .args(["--target", "20580", "--chaser", "99901", "--epoch", EPOCH])
        .args(["--steps", CLI_STEPS, "--sample-every", "1"])
        .arg("--assets")
        .arg(repo_root().join("assets"))
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
    let rows = parse_rows(&std::fs::read_to_string(out.join("trajectory.csv")).unwrap());
    let summary: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(out.join("summary.json")).unwrap()).unwrap();
    let _ = std::fs::remove_dir_all(&project);

    let harness = q13::run(HARNESS_TICKS, false);

    // Preconditions: the offset is the documented one, and the horizons match.
    assert_eq!(
        rows.first().map(|r| r.step),
        Some(3),
        "the CLI samples from step 3"
    );
    assert_eq!(rows.last().map(|r| r.step), Some(6_403));
    assert_eq!(harness.checkpoints.len(), 10);
    assert_eq!(harness.checkpoints[0].step, 643);
    assert_eq!(harness.checkpoints[9].step, 6_403);

    // The state and the `range_m` bit pattern at each of the 10 checkpoints.
    for checkpoint in &harness.checkpoints {
        let row = rows
            .iter()
            .find(|row| row.step == checkpoint.step)
            .unwrap_or_else(|| panic!("the CLI has no sample at step {}", checkpoint.step));
        assert_eq!(
            row.state, checkpoint.state,
            "state at step {}",
            checkpoint.step
        );
        assert_eq!(
            row.range_m.to_bits(),
            checkpoint.range_m.to_bits(),
            "range_m at step {}: CLI {} vs harness {}",
            checkpoint.step,
            row.range_m,
            checkpoint.range_m
        );
    }

    // The transitions: the same step, from and to.
    let cli_transitions: Vec<(u64, String, String)> = summary["transitions"]
        .as_array()
        .expect("summary.json has transitions")
        .iter()
        .map(|t| {
            (
                t["step"].as_u64().expect("step"),
                t["from"].as_str().expect("from").to_string(),
                t["to"].as_str().expect("to").to_string(),
            )
        })
        .collect();
    assert_eq!(cli_transitions, harness.transitions);
    assert!(
        !cli_transitions.is_empty(),
        "precondition: the horizon reaches the first transition"
    );
}
