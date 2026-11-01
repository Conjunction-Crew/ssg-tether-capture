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
/// (F-12). Chosen so one run stays under about 50 ms: the Q13 scenario cost
/// 0.576 ms per step in release on an Apple M1 (three 6400-step CLI runs, P4 S1),
/// so 64 steps took about 37 ms and 128 about 74 ms. The rope (P7.5 S2) costs
/// 0.354 ms per step (the same measure; 0.661 on the rod chain at P7.5 S1), so 64
/// steps take about 23 ms and 128 about 45 ms.
pub const MAX_STEPS_PER_UPDATE: u32 = 64;

/// The fixed steps one `FixedUpdate` run may execute (F-46, R9); default
/// [`MAX_STEPS_PER_UPDATE`]. It changes how many steps run per update, never which
/// steps. The CLI keeps the default; the replay driver raises it for the update
/// that spawns the bodies when the recording's k is larger.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct StepCap(pub u32);

impl Default for StepCap {
    fn default() -> Self {
        Self(MAX_STEPS_PER_UPDATE)
    }
}

/// Runs `min(multiplier, StepCap)` fixed steps and records the count as the
/// effective warp. No step debt is carried over. The cap is read once, before the
/// first step (C8).
pub fn fixed_physics_step(world: &mut World) {
    let cap = world.get_resource::<StepCap>().copied().unwrap_or_default();
    let steps = world.resource::<WorldTime>().multiplier.min(cap.0);
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
    /// The fixed steps the last `FixedUpdate` run executed (at most the
    /// [`StepCap`]); 0 before the first run.
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
            // Avian3d Substep Count. F-66 (P7.5 design D3): 8 holds the rope to
            // 0.4 mm of stretch under the capture force (4: 1.5 mm, 16: 0.1 mm).
            .insert_resource(SubstepCount(8))
            .insert_resource(Time::<Fixed>::from_hz(FIXED_HZ)) // FixedUpdate rate
            .init_resource::<StepCap>()
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
