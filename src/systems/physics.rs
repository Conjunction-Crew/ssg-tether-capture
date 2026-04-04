use std::time::Duration;

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::{plugins::orbital_mechanics::ManualPhysics, resources::world_time::WorldTime};

// 64 Hz. DO NOT CHANGE
// This aligns with Bevy's FixedUpdate schedule of 64 Hz.
pub const PHYS_DT: f64 = 1.0 / 64.0;

pub fn fixed_physics_step(world: &mut World) {
    for _ in 0..world.resource::<WorldTime>().multiplier {
        world
            .resource_mut::<Time<Physics>>()
            .advance_by(Duration::from_secs_f64(PHYS_DT));
        world.resource_mut::<WorldTime>().epoch += PHYS_DT;

        world.run_schedule(ManualPhysics);
    }
}
