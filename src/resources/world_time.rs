use bevy::prelude::*;
use brahe::Epoch;

#[derive(Resource, Debug)]
pub struct WorldTime {
    pub multiplier: u32,
    pub epoch: Epoch,
}

impl Default for WorldTime {
    fn default() -> Self {
        Self {
            multiplier: 1,
            epoch: Epoch::now(),
        }
    }
}
