//! P9 S4: per-phase controller gains at their defaults change nothing. The Q13
//! 6,400-step run of the example plan with every phase's `controller` set to the
//! eight defaults explicitly gives the same state hash and transitions as the
//! example plan without the key, and the reference hash, on every platform.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Value, json};
use ssg_sim::capture::{
    BRANCH_DEAD_BAND, FORCE_RAMP_S, INNER_EDGE_FRACTION, PLANE_HOLD_BAND_M, RADIAL_RATE_BAND_MPS,
    RADIUS_HOLD_BAND_M, SPACING_BAND_M, SPACING_RATE_BAND_MPS,
};
use ssg_sim::test_support::q13;

/// The Q13 fixture's GP epoch, as in the harness (`fixed_epoch()`).
const EPOCH: &str = "2026-04-03T16:03:47.002464";

/// `ssg-sim run` of the Q13 6,400-step command (the same on every platform; the
/// value `record.rs` asserts).
const Q13_6400_HASH: &str = "5496fcd1fb51cb11";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A project with the Q13 scenario in `project.json` running `active_plan`, and
/// the example plan with explicit default gains as `example_plan_defaults.json`.
fn project(name: &str, active_plan: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "ssg-sim-cli-controller-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let record = |id| serde_json::to_value(q13::fixture_record(id)).unwrap();
    let manifest = json!({
        "schema_version": 1,
        "name": "Q13 study",
        "start_epoch_utc": format!("{EPOCH}Z"),
        "active_plan": active_plan,
        "target": { "label": "HST", "orbit": { "gp": record(q13::HST_NORAD_ID) } },
        "chaser": { "label": "P0 HST CHASER", "orbit": { "gp": record(q13::CHASER_NORAD_ID) } },
    });
    std::fs::write(
        dir.join("project.json"),
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();

    let example = repo_root().join("assets/example_capture_plans/example_plan.json");
    let mut plan: Value = serde_json::from_str(&std::fs::read_to_string(example).unwrap()).unwrap();
    for phase in plan["phases"].as_array_mut().unwrap() {
        phase["controller"] = json!({
            "radius_hold_band_m": RADIUS_HOLD_BAND_M,
            "radial_rate_band_mps": RADIAL_RATE_BAND_MPS,
            "spacing_band_m": SPACING_BAND_M,
            "spacing_rate_band_mps": SPACING_RATE_BAND_MPS,
            "plane_hold_band_m": PLANE_HOLD_BAND_M,
            "inner_edge_fraction": INNER_EDGE_FRACTION,
            "dead_band_fraction": BRANCH_DEAD_BAND,
            "force_ramp_s": FORCE_RAMP_S,
        });
    }
    std::fs::write(
        dir.join("example_plan_defaults.json"),
        serde_json::to_string_pretty(&plan).unwrap(),
    )
    .unwrap();
    dir
}

/// `ssg-sim run <project> --steps 6400 --out <project>/out`; the summary.
fn run_6400(project: &Path) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_ssg-sim"))
        .arg("run")
        .arg(project)
        .args(["--steps", "6400"])
        .arg("--assets")
        .arg(repo_root().join("assets"))
        .arg("--out")
        .arg(project.join("out"))
        .output()
        .expect("ssg-sim runs");
    assert!(
        output.status.success(),
        "exit {:?}: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_str(&std::fs::read_to_string(project.join("out/summary.json")).unwrap())
        .unwrap()
}

#[test]
fn explicit_default_gains_keep_the_q13_6400_hash() {
    let without = run_6400(&project("without", "example_plan"));
    let with = run_6400(&project("with", "example_plan_defaults"));
    println!(
        "example_plan: {} · every phase's controller at the defaults: {}",
        without["state_hash"], with["state_hash"]
    );
    assert_eq!(with["state_hash"], without["state_hash"]);
    assert_eq!(with["transitions"], without["transitions"]);
    assert_eq!(with["final"], without["final"]);
    assert_eq!(with["state_hash"], Q13_6400_HASH);
}
