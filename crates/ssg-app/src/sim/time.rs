//! Simulation time: the fixed step, the physics schedule and the sim clock.

use std::time::Duration;

use avian3d::prelude::*;
use bevy::{ecs::schedule::ScheduleLabel, math::DVec3, prelude::*};
use brahe::Epoch;

use crate::sim::SimStep;
use crate::sim::session::{SimState, UiScreen};

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

/// The time-warp ladder stops below this multiplier (F-12).
pub const MAX_TIME_WARP: u32 = 10000;

impl WorldTime {
    /// One step up (`increase`) or down the time-warp ladder: up doubles while the
    /// result stays within [`MAX_TIME_WARP`] (0 goes to 1), down halves (1 goes to
    /// 0). Returns whether the multiplier changed; the callers log the change.
    pub fn change_warp(&mut self, increase: bool) -> bool {
        let prev = self.multiplier;
        if increase && self.multiplier * 2 <= MAX_TIME_WARP {
            if self.multiplier == 0 {
                self.multiplier = 1;
            } else {
                self.multiplier *= 2;
            }
        } else if !increase {
            if self.multiplier == 1 {
                self.multiplier = 0;
            } else {
                self.multiplier /= 2;
            }
        }
        self.multiplier != prev
    }
}

pub struct TimePlugin;

impl Plugin for TimePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(PhysicsPlugins::new(ManualPhysics))
            .insert_resource(Gravity(DVec3::ZERO))
            .insert_resource(SubstepCount(4)) // Avian3d Substep Count
            .insert_resource(Time::<Fixed>::from_hz(FIXED_HZ)) // FixedUpdate rate
            .add_systems(
                FixedUpdate,
                fixed_physics_step
                    .run_if(in_state(UiScreen::Sim))
                    .run_if(in_state(SimState::Running)),
            )
            .configure_sets(
                ManualPhysics,
                (
                    SimStep::SyncAnchor,
                    SimStep::CenterOfMass,
                    SimStep::OriginReset,
                    SimStep::CacheEci,
                    SimStep::Bubble,
                    SimStep::Capture,
                )
                    .chain()
                    .in_set(PhysicsSystems::Last)
                    .run_if(in_state(UiScreen::Sim)),
            );
    }
}
