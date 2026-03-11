use bevy::prelude::*;

#[derive(Resource, Debug)]
pub struct TimeWarp {
    pub multiplier: f64,
}

impl Default for TimeWarp {
    fn default() -> Self {
        Self { multiplier: 1.0 }
    }
}
