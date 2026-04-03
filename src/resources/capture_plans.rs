use std::fs;
use std::path::Path;

use bevy::{platform::collections::HashMap, prelude::*};

use crate::components::capture_components::CapturePlan;

#[derive(Resource, Debug, Default)]
pub struct CapturePlanLibrary {
    /// Union of user_plans and example_plans — used by simulation systems.
    pub plans: HashMap<String, CapturePlan>,
    /// Plans loaded from the user's working directory.
    pub user_plans: HashMap<String, CapturePlan>,
    /// Plans loaded from assets/capture_plans.
    pub example_plans: HashMap<String, CapturePlan>,
}

#[derive(Resource, Debug, Default)]
pub struct CaptureSphereRadius {
    pub radius: f64,
}

/// Loads all valid `.json` capture plan files from `dir` and returns them as a map
/// keyed by file stem. Returns an empty map if the directory does not exist or cannot
/// be read.
pub fn load_plans_from_dir(dir: &Path) -> HashMap<String, CapturePlan> {
    let mut plans = HashMap::new();
    if !dir.exists() {
        return plans;
    }
    let read_dir = match fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return plans,
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "json") {
            if let Ok(raw_json) = fs::read_to_string(&path) {
                if let Ok(plan) = serde_json::from_str::<CapturePlan>(&raw_json) {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        plans.insert(stem.to_string(), plan);
                    }
                }
            }
        }
    }
    plans
}
