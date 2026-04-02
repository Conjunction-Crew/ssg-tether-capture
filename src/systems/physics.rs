use std::time::Duration;

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::{plugins::orbital_mechanics::ManualPhysics, resources::world_time::WorldTime};

// 240 Hz
pub const PHYS_DT: f64 = 1.0 / 240.0;

pub fn fixed_physics_step(world: &mut World) {
    for _ in 0..world.resource::<WorldTime>().multiplier {
        world
            .resource_mut::<Time<Physics>>()
            .advance_by(Duration::from_secs_f64(PHYS_DT));

        world.run_schedule(ManualPhysics);
    }
}
