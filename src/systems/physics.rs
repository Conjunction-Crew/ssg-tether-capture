use std::time::Duration;

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::{
    plugins::orbital_mechanics::{ManualPhysics, SimSchedule},
    resources::world_time::WorldTime,
};

// FixedUpdate frequency.
// Higher values will provide higher fidelity simulation, at the cost of performance.
pub const FIXED_HZ: f64 = 64.0;
pub const PHYS_DT: f64 = 1.0 / FIXED_HZ;

pub fn fixed_physics_step(world: &mut World) {
    let mult = world.resource::<WorldTime>().multiplier;
    for _ in 0..mult {
        if mult <= 32 {
            world
                .resource_mut::<Time<Physics>>()
                .advance_by(Duration::from_secs_f64(PHYS_DT));
            world.run_schedule(ManualPhysics);
        }
        world.resource_mut::<WorldTime>().epoch += PHYS_DT;

        world.run_schedule(SimSchedule);
    }
}
