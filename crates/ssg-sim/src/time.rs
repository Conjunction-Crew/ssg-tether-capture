//! Simulation time: the fixed step, the physics schedule and the sim clock.

use std::time::Duration;

use avian3d::prelude::*;
use bevy::{ecs::schedule::ScheduleLabel, math::DVec3, prelude::*};
use brahe::Epoch;

use crate::SimStep;
use crate::session::{PauseState, SimState, UiScreen};
use crate::telemetry::{LogEvent, LogLevel};

// FixedUpdate frequency.
// Higher values will provide higher fidelity simulation, at the cost of performance.
pub const FIXED_HZ: f64 = 64.0;
pub const PHYS_DT: f64 = 1.0 / FIXED_HZ;

/// Fixed steps one `FixedUpdate` run may execute, whatever the requested warp
/// (F-12). Chosen so one run stays under about 50 ms: the Q13 scenario costs
/// 0.576 ms per step in release on an Apple M1 (three 6400-step CLI runs, P4 S1),
/// so 64 steps take about 37 ms and 128 about 74 ms.
pub const MAX_STEPS_PER_UPDATE: u32 = 64;

/// Runs `min(multiplier, MAX_STEPS_PER_UPDATE)` fixed steps and records the count
/// as the effective warp. No step debt is carried over.
pub fn fixed_physics_step(world: &mut World) {
    let steps = world
        .resource::<WorldTime>()
        .multiplier
        .min(MAX_STEPS_PER_UPDATE);
    for _ in 0..steps {
        // `Time<Physics>` is advanced once, by avian's `run_physics_schedule`
        // (F-38: fixed in P4).
        world.resource_mut::<WorldTime>().epoch += PHYS_DT;

        world.run_schedule(ManualPhysics);
    }
    world.resource_mut::<WorldTime>().effective_multiplier = steps;
}

/// Lowers Bevy's catch-up window (250 ms, 16 fixed steps) to 3 fixed steps, so
/// one slow frame cannot multiply the work of a `FixedUpdate` run by 16 (F-12).
pub fn configure_catch_up_window(mut virtual_time: ResMut<Time<Virtual>>) {
    virtual_time.set_max_delta(Duration::from_secs_f64(3.0 * PHYS_DT));
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
    world_time.effective_multiplier = 0;
    log.write(LogEvent {
        level: LogLevel::Info,
        source: "time",
        message: format!("Sim start epoch: {}", config.start_epoch),
    });
}

#[derive(Resource, Debug)]
pub struct WorldTime {
    /// The requested warp: fixed steps per `FixedUpdate` run.
    pub multiplier: u32,
    /// The fixed steps the last `FixedUpdate` run executed (at most
    /// [`MAX_STEPS_PER_UPDATE`]); 0 before the first run.
    pub effective_multiplier: u32,
    pub epoch: Epoch,
    pub start_epoch: Epoch,
}

impl WorldTime {
    /// A sim clock at `epoch`, at 1x.
    pub fn starting_at(epoch: Epoch) -> Self {
        Self {
            multiplier: 1,
            effective_multiplier: 0,
            epoch,
            start_epoch: epoch,
        }
    }
}

/// The top of the time-warp ladder (1, 2, 4, …, 8192), itself a rung (F-12).
pub const MAX_TIME_WARP: u32 = 8192;

/// The outcome of one step on the time-warp ladder; the callers apply the pause
/// state and log the change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WarpChange {
    Unchanged,
    Multiplier(u32),
    Paused,
    Resumed,
}

impl WorldTime {
    /// One step up (`increase`) or down the time-warp ladder. Up while `paused`
    /// resumes at the current multiplier; up doubles while the result stays
    /// within [`MAX_TIME_WARP`]; down halves while above 1; down at 1 pauses.
    /// The multiplier is never 0.
    pub fn change_warp(&mut self, increase: bool, paused: bool) -> WarpChange {
        match (increase, paused) {
            (true, true) => WarpChange::Resumed,
            (true, false) if self.multiplier * 2 <= MAX_TIME_WARP => {
                self.multiplier *= 2;
                WarpChange::Multiplier(self.multiplier)
            }
            (false, false) if self.multiplier > 1 => {
                self.multiplier /= 2;
                WarpChange::Multiplier(self.multiplier)
            }
            (false, false) => WarpChange::Paused,
            _ => WarpChange::Unchanged,
        }
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
                    .run_if(in_state(SimState::Running))
                    .run_if(in_state(PauseState::Running)),
            )
            .add_systems(Startup, configure_catch_up_window)
            .configure_sets(
                ManualPhysics,
                (
                    SimStep::CenterOfMass,
                    SimStep::OriginReset,
                    SimStep::CacheEci,
                    SimStep::Bubble,
                    SimStep::RelativeDynamics,
                    SimStep::Capture,
                    SimStep::Record,
                )
                    .chain()
                    .in_set(PhysicsSystems::Last)
                    .run_if(in_state(UiScreen::Sim)),
            );
    }
}
