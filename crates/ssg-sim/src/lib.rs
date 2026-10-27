//! Simulation core: everything that runs headless (time, orbits, the physics
//! bubble, the floating origin, the tether, capture, logging, project files).
//!
//! This crate is headless: nothing in it may use the app's render or UI modules,
//! the app's asset-path helper, or a render, window or UI type (the P3
//! sim-boundary check).

pub mod anchor;
pub mod capture;
pub mod floating_origin;
pub mod orbit;
pub mod physics_bubble;
pub mod project;
pub mod session;
pub mod telemetry;
#[cfg(any(test, feature = "test-support"))]
pub mod test_support;
pub mod tether;
pub mod time;

#[cfg(test)]
mod tests;

use bevy::app::PluginGroupBuilder;
use bevy::prelude::*;

/// The steps of one `ManualPhysics` run, chained in this order and all after
/// avian's step (`PhysicsSystems::Last`). This is the order the systems had
/// before P3, now with names (F-13); P3 moves nothing relative to avian.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum SimStep {
    SyncAnchor,
    CenterOfMass,
    OriginReset,
    CacheEci,
    Bubble,
    Capture,
}

/// Sets in other schedules that the app orders its systems against.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum SimSet {
    /// `OnEnter(UiScreen::Sim)`: session resources, dataset load, sim clock (F-40).
    SessionInit,
    /// `OnEnter(SimState::Running)`: the tether and the debris are spawned (F-34).
    SpawnBodies,
    /// `Last`: queued [`capture::SimCommand`]s are applied.
    ApplyCommands,
}

/// The simulation plugins, one per feature module. Add them after `StatesPlugin`:
/// `init_state` needs the `StateTransition` schedule.
pub struct SimPlugins;

impl PluginGroup for SimPlugins {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add(session::SessionPlugin)
            .add(time::TimePlugin)
            .add(project::ProjectPlugin)
            .add(telemetry::TelemetryPlugin)
            .add(orbit::OrbitPlugin)
            .add(anchor::AnchorPlugin)
            .add(floating_origin::FloatingOriginPlugin)
            .add(physics_bubble::PhysicsBubblePlugin)
            .add(tether::TetherPlugin)
            .add(capture::CapturePlugin)
    }
}
