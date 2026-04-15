use bevy::prelude::*;
use std::path::PathBuf;

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
    if let Some(parent) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            eprintln!("Failed to create config directory: {e}");
            return;
        }
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
