//! Session lifecycle: the session and sim states and the per-session resources.

use bevy::prelude::*;

use crate::SimSet;
use crate::capture::{CaptureSphereRadius, INITIAL_CAPTURE_SPHERE_RADIUS_M};
use crate::orbit::cache::OrbitalCache;
use crate::orbit::catalog::load_dataset_entities;
use crate::time::{SimConfig, WorldTime, setup_time};

/// Whether a sim session exists (Q35, P8): `Active` holds the session
/// resources, the bodies and the clock. The app drives it from its own screen
/// state; the CLI and the tests set it directly.
#[derive(States, Debug, Clone, Eq, PartialEq, Hash, Default)]
pub enum SimSession {
    #[default]
    Idle,
    Active,
}

#[derive(States, Debug, Clone, Eq, PartialEq, Hash, Default)]
pub enum SimState {
    #[default]
    Setup,
    Running,
    /// Playing a recording (P7): visual-only bodies, no physics step, no sim
    /// commands and no recorder. Entered from `Setup` by the app's Recordings window.
    Replay,
}

/// Whether the sim clock advances (F-12): pause is a state, not a 0x warp.
#[derive(States, Debug, Clone, Copy, Eq, PartialEq, Hash, Default)]
pub enum PauseState {
    #[default]
    Running,
    Paused,
}

pub struct SessionPlugin;

impl Plugin for SessionPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<SimSession>()
            .init_state::<SimState>()
            .init_state::<PauseState>()
            .configure_sets(OnEnter(SimSession::Active), SimSet::SessionInit)
            .add_systems(
                OnEnter(SimSession::Active),
                (init_sim_resources, load_dataset_entities, setup_time)
                    .chain()
                    .in_set(SimSet::SessionInit),
            )
            .add_systems(OnExit(SimSession::Active), remove_sim_resources);
    }
}

fn init_sim_resources(
    mut commands: Commands,
    config: Res<SimConfig>,
    mut pause: ResMut<NextState<PauseState>>,
) {
    pause.set(PauseState::Running);
    commands.init_resource::<OrbitalCache>();
    commands.insert_resource(WorldTime::starting_at(config.start_epoch));
    commands.insert_resource(CaptureSphereRadius {
        radius: INITIAL_CAPTURE_SPHERE_RADIUS_M,
    });
}

fn remove_sim_resources(mut commands: Commands) {
    commands.remove_resource::<OrbitalCache>();
    commands.remove_resource::<WorldTime>();
    commands.remove_resource::<CaptureSphereRadius>();
}
