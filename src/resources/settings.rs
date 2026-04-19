use bevy::prelude::*;

#[derive(Resource, Debug)]
pub struct Settings {
    pub dev_gizmos: bool,
    pub capture_gizmos: bool,
    pub start_sim: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            dev_gizmos: false,
            capture_gizmos: false,
            start_sim: false,
        }
    }
}
