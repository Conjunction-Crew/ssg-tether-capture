use bevy::prelude::*;
use std::path::PathBuf;

#[derive(Resource, Debug, Clone)]
pub struct WorkingDirectory {
    pub path: String,
    pub pending_path: String,
}

fn default_path() -> String {
    #[cfg(target_os = "windows")]
    let base = std::env::var("USERPROFILE")
        .unwrap_or_else(|_| "C:\\Users\\user".to_string());

    #[cfg(not(target_os = "windows"))]
    let base = std::env::var("HOME")
        .unwrap_or_else(|_| "/home/user".to_string());

    PathBuf::from(base)
        .join("ssg-tether-capture")
        .join("project1")
        .to_string_lossy()
        .to_string()
}

impl Default for WorkingDirectory {
    fn default() -> Self {
        let path = default_path();
        Self {
            path: path.clone(),
            pending_path: path,
        }
    }
}
