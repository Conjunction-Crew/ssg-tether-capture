//! `summary.json`, `trajectory.csv` and the one-line stdout summary.

use std::fs;
use std::path::PathBuf;

use brahe::Epoch;
use serde_json::json;

use crate::args::Args;
use crate::scenario::Run;

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
        "steps": run.updates,
        "warp": args.warp,
        "transitions": transitions,
        "final": { "step": run.last.step, "state": run.last.state, "range_m": run.last.range_m },
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
        "plan {} ({} epoch): {} updates, {} transitions, final {} at step {}, range {:.6} m, {:.1} s wall -> {}",
        run.plan,
        run.epoch_mode,
        run.updates,
        run.transitions.len(),
        run.last.state,
        run.last.step,
        run.last.range_m,
        run.wall.as_secs_f64(),
        out.display()
    );
    Ok(())
}

/// `runs/<utc-timestamp>`, relative to the project directory; the timestamp has no
/// `:` so the name is valid on every platform.
fn default_out_dir() -> PathBuf {
    let stamp = Epoch::now().isostring().replace(['-', ':'], "");
    PathBuf::from("runs").join(stamp)
}
