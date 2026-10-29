//! Project files: the selected plan, the working directory and plan file I/O.

pub mod epoch;
pub mod manifest;
pub(crate) mod migrate_v0;
pub mod plan_files;

pub use epoch::{format_utc, parse_utc};

use std::fs;
use std::path::{Path, PathBuf};

use bevy::{platform::collections::HashMap, prelude::*};

use crate::capture::plan::{CapturePlan, CapturePlanLibrary};
use crate::orbit::catalog::SpaceObjectCatalog;
use crate::orbit::selection::OrbitalSelectionState;

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
    read_config_file(&config_path()?)
}

/// The `schema_version` of the config file this build writes and reads.
const CONFIG_SCHEMA_VERSION: u64 = 1;

/// The `working_directory` entry of the config file at `path`; `None` when the file
/// is missing, is not JSON, has no string entry or was written by a newer version
/// (a warning is logged then). A file without `schema_version` is read as before.
pub(crate) fn read_config_file(path: &Path) -> Option<String> {
    let raw = std::fs::read_to_string(path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&raw).ok()?;
    if let Some(version) = value.get("schema_version").and_then(|v| v.as_u64())
        && version > CONFIG_SCHEMA_VERSION
    {
        warn!(
            "{}: schema_version {version} was written by a newer version (this one reads \
             {CONFIG_SCHEMA_VERSION}); the file is ignored",
            path.display()
        );
        return None;
    }
    value
        .get("working_directory")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

pub fn save_to_config(working_dir: &str) {
    let Some(path) = config_path() else {
        return;
    };
    write_config_file(&path, working_dir);
}

/// Writes the config file at `path` (creating its directory) with `working_dir` as
/// the `working_directory` entry; failures are logged, not returned.
pub(crate) fn write_config_file(path: &Path, working_dir: &str) {
    if let Some(parent) = path.parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        warn!("Failed to create config directory: {e}");
        return;
    }
    let value = serde_json::json!({
        "schema_version": CONFIG_SCHEMA_VERSION,
        "working_directory": working_dir,
    });
    if let Err(e) = std::fs::write(path, value.to_string()) {
        warn!("Failed to save config: {e}");
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

/// Like [`load_plans_from_dir`] but also returns a map of file stems to error
/// messages for any files that could not be read, parsed, migrated or validated.
/// The first return value contains only the successfully loaded plans (v0 files
/// migrated to v1 in memory; nothing is written). `project.json` is not a plan.
pub fn load_plans_from_dir_with_errors(
    dir: &Path,
) -> (HashMap<String, CapturePlan>, HashMap<String, Vec<String>>) {
    let mut plans = HashMap::new();
    let mut errors: HashMap<String, Vec<String>> = HashMap::new();
    for (stem, report) in plan_files::plan_file_reports(dir) {
        match report.plan {
            Some(plan) => {
                plans.insert(stem, plan);
            }
            None => {
                errors.insert(stem, report.errors);
            }
        }
    }
    (plans, errors)
}

/// The `assets/` directory the sim reads datasets from: the app inserts the
/// directory it resolved at start-up, the test harness `<repo>/assets`.
#[derive(Resource, Debug, Clone)]
pub struct AssetsDir(pub PathBuf);

/// Sets the working directory, saves it to the config file and creates it.
pub fn set_working_directory(wd: &mut WorkingDirectory, path: &str) -> std::io::Result<()> {
    wd.path = path.to_string();
    save_to_config(path);
    fs::create_dir_all(path)
}

/// Writes `json`, pretty-printed, to `dir/filename`.
pub fn save_plan_file(dir: &Path, filename: &str, json: &serde_json::Value) -> std::io::Result<()> {
    let content = serde_json::to_string_pretty(json)?;
    fs::write(dir.join(filename), content)
}

impl CapturePlanLibrary {
    /// Loads the example and user plans, rebuilds the union and compiles every
    /// plan. Returns the load errors by file stem.
    pub fn load_all(
        &mut self,
        example_dir: &Path,
        user_dir: &Path,
    ) -> HashMap<String, Vec<String>> {
        let (example_plans, mut load_errors) = load_plans_from_dir_with_errors(example_dir);
        let (user_plans, user_load_errors) = load_plans_from_dir_with_errors(user_dir);
        load_errors.extend(user_load_errors);

        self.example_plans = example_plans;
        self.user_plans = user_plans;
        self.plans = self
            .example_plans
            .iter()
            .chain(self.user_plans.iter())
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        // F-53 (P6): rebuild the compiled plans from scratch, so a plan that is
        // gone or no longer valid has no compiled entry.
        self.compiled_plans.clear();
        for (name, plan) in self.plans.clone() {
            self.insert_plan(name.clone(), plan.clone());
        }

        load_errors
    }

    /// Reloads the user plans after a plan file was saved, rebuilds the union and
    /// recompiles every plan so `compiled_plans` holds exactly the union's plans.
    pub fn reload_user_plans(&mut self, user_dir: &Path) {
        let new_user_plans = load_plans_from_dir(user_dir);
        self.user_plans = new_user_plans;
        self.plans = self
            .example_plans
            .iter()
            .chain(self.user_plans.iter())
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let refreshed: Vec<(String, CapturePlan)> = self
            .plans
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        // F-53 (P6): as in `load_all`.
        self.compiled_plans.clear();
        for (id, plan) in refreshed {
            self.insert_plan(id, plan);
        }
    }
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
