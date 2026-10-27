//! Reading the inputs: the capture plans and the GP records of the two bodies.

use std::fs;
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ssg_sim::capture::plan::CapturePlanLibrary;
use ssg_sim::orbit::catalog::{JsonOrbital, JsonOrbitalData};

use crate::args::{Args, DEFAULT_PLAN};

/// Load the example plans and the project's plans, pick the plan id.
pub fn load_plans(app: &mut App, args: &Args) -> Result<String, String> {
    let mut library = app.world_mut().resource_mut::<CapturePlanLibrary>();
    let errors = library.load_all(
        &args.assets.join("example_capture_plans"),
        &args.project_dir,
    );
    for (plan, messages) in &errors {
        eprintln!("ssg-sim: plan `{plan}` not loaded: {}", messages.join("; "));
    }
    let plan = match &args.plan {
        Some(plan) => plan.clone(),
        None if library.user_plans.len() == 1 => library.user_plans.keys().next().cloned().unwrap(),
        None => DEFAULT_PLAN.to_string(),
    };
    if library.plans.contains_key(&plan) {
        Ok(plan)
    } else {
        Err(format!("capture plan `{plan}` not found"))
    }
}

/// The first record for each NORAD id, searching `<project>/datasets/*.json`,
/// `<assets>/datasets/*.json`, then the `--dataset` files.
pub fn find_records(args: &Args) -> Result<(JsonOrbital, JsonOrbital), String> {
    let mut files = json_files(&args.project_dir.join("datasets"));
    files.extend(json_files(&args.assets.join("datasets")));
    let explicit = args.datasets.len();
    files.extend(args.datasets.iter().cloned());
    let (mut target, mut chaser) = (None, None);
    for (index, path) in files.iter().enumerate() {
        if target.is_some() && chaser.is_some() {
            break;
        }
        let from_flag = index >= files.len() - explicit;
        let data = match fs::read_to_string(path)
            .map_err(|e| e.to_string())
            .and_then(|raw| {
                serde_json::from_str::<JsonOrbitalData>(&raw).map_err(|e| e.to_string())
            }) {
            Ok(data) => data,
            Err(e) if from_flag => return Err(format!("dataset {}: {e}", path.display())),
            Err(e) => {
                eprintln!("ssg-sim: skipping dataset {}: {e}", path.display());
                continue;
            }
        };
        for record in data.data {
            let id = record.norad_cat_id.as_ref().and_then(|id| id.as_u64());
            if target.is_none() && id == Some(args.target) {
                target = Some(record);
            } else if chaser.is_none() && id == Some(args.chaser) {
                chaser = Some(record);
            }
        }
    }
    Ok((
        target.ok_or_else(|| format!("NORAD {} not found in any dataset", args.target))?,
        chaser.ok_or_else(|| format!("NORAD {} not found in any dataset", args.chaser))?,
    ))
}

fn json_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    files.sort();
    files
}
