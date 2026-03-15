use bevy::prelude::*;
use brahe::Epoch;

#[derive(Resource, Debug)]
pub struct WorldTime {
    pub multiplier: f64,
    pub epoch: Epoch,
}

impl Default for WorldTime {
    fn default() -> Self {
        Self {
            multiplier: 1.0,
            epoch: Epoch::now(),
        }
    }
}
