//! Simulation time: the fixed step, the physics schedule and the sim clock.

use std::time::Duration;

use avian3d::prelude::*;
use bevy::{ecs::schedule::ScheduleLabel, prelude::*};
use brahe::Epoch;

// FixedUpdate frequency.
// Higher values will provide higher fidelity simulation, at the cost of performance.
pub const FIXED_HZ: f64 = 64.0;
pub const PHYS_DT: f64 = 1.0 / FIXED_HZ;

pub fn fixed_physics_step(world: &mut World) {
    for _ in 0..world.resource::<WorldTime>().multiplier {
        world
            .resource_mut::<Time<Physics>>()
            .advance_by(Duration::from_secs_f64(PHYS_DT));
        world.resource_mut::<WorldTime>().epoch += PHYS_DT;

        world.run_schedule(ManualPhysics);
    }
}

#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
pub struct ManualPhysics;

pub fn setup_time(mut world_time: ResMut<WorldTime>) {
    world_time.start_epoch = Epoch::now();
    world_time.epoch = Epoch::now();
}

#[derive(Resource, Debug)]
pub struct WorldTime {
    pub multiplier: u32,
    pub epoch: Epoch,
    pub start_epoch: Epoch,
}

impl Default for WorldTime {
    fn default() -> Self {
        Self {
            multiplier: 1,
            epoch: Epoch::now(),
            start_epoch: Epoch::now(),
        }
    }
}
