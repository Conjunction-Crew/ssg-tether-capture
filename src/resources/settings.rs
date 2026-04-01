use bevy::prelude::*;
use brahe::Epoch;

#[derive(Resource, Debug)]
pub struct Settings {
    pub dev_gizmos: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            dev_gizmos: false,
        }
    }
}
