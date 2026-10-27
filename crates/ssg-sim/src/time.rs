//! Simulation time: the fixed step, the physics schedule and the sim clock.

use std::time::Duration;

use avian3d::prelude::*;
use bevy::{ecs::schedule::ScheduleLabel, math::DVec3, prelude::*};
use brahe::Epoch;

use crate::SimStep;
use crate::session::{SimState, UiScreen};
use crate::telemetry::{LogEvent, LogLevel};

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

/// The simulation's explicit inputs (F-06). The sim never reads the wall clock:
/// the app, the CLI or the test harness inserts this before entering the sim.
#[derive(Resource, Debug, Clone)]
pub struct SimConfig {
    /// The epoch the simulation starts at; orbital elements are taken at it.
    pub start_epoch: Epoch,
}

/// Resets the sim clock to [`SimConfig::start_epoch`] on entering the sim.
pub fn setup_time(
    config: Res<SimConfig>,
    mut world_time: ResMut<WorldTime>,
    mut log: MessageWriter<LogEvent>,
) {
    world_time.start_epoch = config.start_epoch;
    world_time.epoch = config.start_epoch;
    world_time.multiplier = 1;
    log.write(LogEvent {
        level: LogLevel::Info,
        source: "time",
        message: format!("Sim start epoch: {}", config.start_epoch),
    });
}

#[derive(Resource, Debug)]
pub struct WorldTime {
    pub multiplier: u32,
    pub epoch: Epoch,
    pub start_epoch: Epoch,
}

impl WorldTime {
    /// A sim clock at `epoch`, at 1x.
    pub fn starting_at(epoch: Epoch) -> Self {
        Self {
            multiplier: 1,
            epoch,
            start_epoch: epoch,
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
