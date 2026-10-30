//! `summary.json`, `trajectory.csv`, `ephemeris.oem` and the one-line stdout
//! summary.

use std::fs;
use std::path::PathBuf;

use brahe::Epoch;
use serde_json::json;
use ssg_sim::project::export::{OemObject, TrajectoryRow, write_oem, write_trajectory_csv};

use crate::args::Args;
use crate::scenario::{Run, Sample};

pub fn write(args: &Args, run: &Run) -> Result<(), String> {
    let out = args
        .out
        .clone()
        .unwrap_or_else(|| args.project_dir.join(default_out_dir()));
    fs::create_dir_all(&out).map_err(|e| format!("cannot create {}: {e}", out.display()))?;

    let transitions: Vec<_> = run
        .transitions
        .iter()
        .map(|t| json!({ "step": t.step, "from": t.from, "to": t.to }))
        .collect();
    let summary = json!({
        "plan": run.plan,
        "target": run.target,
        "chaser": run.chaser,
        "epoch_mode": run.epoch_mode,
        "epoch": run.start_epoch,
        "steps": run.steps,
        "warp": args.warp,
        "transitions": transitions,
        "final": { "step": run.last.step, "phase": run.last.state, "range_m": run.last.range_m },
        "state_hash": format!("{:016x}", state_hash(&run.samples)),
        "wall_s": run.wall.as_secs_f64(),
    });
    let path = out.join("summary.json");
    let text = serde_json::to_string_pretty(&summary).map_err(|e| e.to_string())?;
    fs::write(&path, text).map_err(|e| format!("cannot write {}: {e}", path.display()))?;

    write_trajectory_csv(
        &out.join("trajectory.csv"),
        run.samples.iter().map(|s| TrajectoryRow {
            step: s.step,
            epoch_utc: &s.epoch,
            phase: &s.state,
            range_m: s.range_m,
            rel_speed_mps: s.rel_speed_mps,
        }),
    )?;

    // Without samples (`--stop-at-capture` before the first) there is no ephemeris.
    if !run.ephemeris.is_empty() {
        let segment = |name, id, chaser: bool| OemObject {
            name,
            id,
            states: run
                .ephemeris
                .iter()
                .map(|e| (e.epoch, if chaser { e.chaser } else { e.target }))
                .collect(),
        };
        write_oem(
            &out.join("ephemeris.oem"),
            run.start,
            &[
                segment(&run.target_label, run.target_object_id.as_deref(), false),
                segment(&run.chaser_label, run.chaser_object_id.as_deref(), true),
            ],
        )?;
    }

    println!(
        "plan {} ({} epoch): {} steps at {}x, {} transitions, final {} at step {}, range {:.6} m, {:.1} s wall -> {}",
        run.plan,
        run.epoch_mode,
        run.steps,
        args.warp,
        run.transitions.len(),
        run.last.state,
        run.last.step,
        run.last.range_m,
        run.wall.as_secs_f64(),
        out.display()
    );
    Ok(())
}

/// 64-bit FNV-1a over the sampled state, written to `summary.json` as 16 hex
/// digits. Byte layout, per sample in order: `step` as a `u64`, then the root's
/// `Position` and `LinearVelocity` and the debris's `Position` and `LinearVelocity`
/// (x, y, z each) as 12 `f64` bit patterns (`f64::to_bits`), every value
/// little-endian: 104 bytes per sample. Equal hashes mean bit-identical sampled
/// states (up to an FNV collision).
pub fn state_hash(samples: &[Sample]) -> u64 {
    const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0100_0000_01b3;
    let mut hash = OFFSET_BASIS;
    let mut feed = |value: u64| {
        for byte in value.to_le_bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(PRIME);
        }
    };
    for sample in samples {
        feed(sample.step);
        for vector in [
            sample.root_position,
            sample.root_velocity,
            sample.debris_position,
            sample.debris_velocity,
        ] {
            for component in vector.to_array() {
                feed(component.to_bits());
            }
        }
    }
    hash
}

/// `runs/<utc-timestamp>`, relative to the project directory; the timestamp has no
/// `:` so the name is valid on every platform.
fn default_out_dir() -> PathBuf {
    let stamp = Epoch::now().isostring().replace(['-', ':'], "");
    PathBuf::from("runs").join(stamp)
}

#[cfg(test)]
#[path = "tests/output.rs"]
mod tests;
