//! Project files: the selected plan, the working directory and plan file I/O.

use std::fs;
use std::path::{Path, PathBuf};

use bevy::{platform::collections::HashMap, prelude::*};

use crate::sim::capture::plan::{CapturePlan, CapturePlanLibrary, validate_capture_plan};
use crate::sim::orbit::catalog::SpaceObjectCatalog;
use crate::sim::orbit::selection::OrbitalSelectionState;

#[derive(Resource, Debug, Clone, Default)]
pub struct SelectedProject {
    pub project_id: Option<String>,
}

#[derive(Resource, Debug, Clone)]
pub struct WorkingDirectory {
    pub path: String,
    pub pending_path: String,
}

fn default_path() -> String {
    #[cfg(target_os = "windows")]
    let base = std::env::var("USERPROFILE").unwrap_or_else(|_| "C:\\Users\\user".to_string());

    #[cfg(not(target_os = "windows"))]
    let base = std::env::var("HOME").unwrap_or_else(|_| "/home/user".to_string());

    PathBuf::from(base)
        .join("ssg-tether-capture")
        .join("project1")
        .to_string_lossy()
        .to_string()
}

fn config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("ssg-tether-capture").join("config.json"))
}

pub fn load_from_config() -> Option<String> {
    let path = config_path()?;
    let raw = std::fs::read_to_string(path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&raw).ok()?;
    value
        .get("working_directory")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

pub fn save_to_config(working_dir: &str) {
    let Some(path) = config_path() else {
        return;
    };
    if let Some(parent) = path.parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        eprintln!("Failed to create config directory: {e}");
        return;
    }
    let value = serde_json::json!({ "working_directory": working_dir });
    if let Err(e) = std::fs::write(&path, value.to_string()) {
        eprintln!("Failed to save config: {e}");
    }
}

impl Default for WorkingDirectory {
    fn default() -> Self {
        let path = load_from_config().unwrap_or_else(default_path);
        Self {
            path: path.clone(),
            pending_path: path,
        }
    }
}

/// Loads all valid `.json` capture plan files from `dir` and returns them as a map
/// keyed by file stem. Returns an empty map if the directory does not exist or cannot
/// be read.
pub fn load_plans_from_dir(dir: &Path) -> HashMap<String, CapturePlan> {
    let (plans, _) = load_plans_from_dir_with_errors(dir);
    plans
}

/// Like [`load_plans_from_dir`] but also returns a map of file stems to validation
/// error messages for any files that could not be parsed or failed validation. The
/// first return value contains only the successfully loaded plans.
pub fn load_plans_from_dir_with_errors(
    dir: &Path,
) -> (HashMap<String, CapturePlan>, HashMap<String, Vec<String>>) {
    let mut plans = HashMap::new();
    let mut errors: HashMap<String, Vec<String>> = HashMap::new();

    if !dir.exists() {
        return (plans, errors);
    }
    let read_dir = match fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return (plans, errors),
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        if !path.extension().is_some_and(|ext| ext == "json") {
            continue;
        }
        let stem = match path.file_stem().and_then(|s| s.to_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };
        let raw_json = match fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                errors.insert(stem, vec![format!("Could not read file: {e}")]);
                continue;
            }
        };
        let mut plan = match serde_json::from_str::<CapturePlan>(&raw_json) {
            Ok(p) => p,
            Err(e) => {
                errors.insert(stem, vec![format!("Invalid JSON: {e}")]);
                continue;
            }
        };
        let validation_errors = validate_capture_plan(&stem, &plan);
        if validation_errors.is_empty() {
            plan.id = stem.clone();
            plans.insert(stem, plan);
        } else {
            errors.insert(stem, validation_errors);
        }
    }
    (plans, errors)
}

/// Project-level resources. `WorkingDirectory` is initialised by the app: its
/// `Default` reads the user's config file.
pub struct ProjectPlugin;

impl Plugin for ProjectPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SelectedProject>()
            .init_resource::<CapturePlanLibrary>()
            .init_resource::<SpaceObjectCatalog>()
            .init_resource::<OrbitalSelectionState>();
    }
}
