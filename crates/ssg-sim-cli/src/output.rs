//! `summary.json`, `trajectory.csv` and the one-line stdout summary.

use std::fs;
use std::path::PathBuf;

use brahe::Epoch;
use serde_json::json;

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
        "target": args.target,
        "chaser": args.chaser,
        "epoch_mode": run.epoch_mode,
        "epoch": run.start_epoch,
        "steps": run.steps,
        "warp": args.warp,
        "transitions": transitions,
        "final": { "step": run.last.step, "state": run.last.state, "range_m": run.last.range_m },
        "state_hash": format!("{:016x}", state_hash(&run.samples)),
        "wall_s": run.wall.as_secs_f64(),
    });
    let path = out.join("summary.json");
    let text = serde_json::to_string_pretty(&summary).map_err(|e| e.to_string())?;
    fs::write(&path, text).map_err(|e| format!("cannot write {}: {e}", path.display()))?;

    let path = out.join("trajectory.csv");
    let mut csv = csv::Writer::from_path(&path)
        .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    csv.write_record(["step", "epoch", "state", "range_m", "rel_speed_mps"])
        .map_err(|e| e.to_string())?;
    for s in &run.samples {
        csv.write_record([
            s.step.to_string(),
            s.epoch.clone(),
            s.state.clone(),
            s.range_m.to_string(),
            s.rel_speed_mps.to_string(),
        ])
        .map_err(|e| e.to_string())?;
    }
    csv.flush().map_err(|e| e.to_string())?;

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
mod tests {
    use bevy::math::DVec3;

    use super::state_hash;
    use crate::scenario::Sample;

    /// The documented byte layout, checked against an independent FNV-1a 64 of the
    /// same 104 bytes (Python `struct.pack('<Q', 643)` + 12 × `'<d'`).
    #[test]
    fn state_hash_follows_documented_layout() {
        assert_eq!(
            state_hash(&[]),
            0xcbf2_9ce4_8422_2325,
            "empty: the offset basis"
        );
        let sample = Sample {
            step: 643,
            epoch: String::new(),
            state: String::new(),
            range_m: 0.0,
            rel_speed_mps: 0.0,
            root_position: DVec3::new(1.0, -2.5, 3.25),
            root_velocity: DVec3::new(0.5, 0.0, -0.0),
            debris_position: DVec3::new(1e6, 2e-3, -7.0),
            debris_velocity: DVec3::new(0.125, 6.5, -1e-9),
        };
        assert_eq!(state_hash(&[sample]), 0xc1d8_7d67_f58d_7639);
    }
}
