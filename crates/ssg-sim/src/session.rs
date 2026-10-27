//! Session lifecycle: the screen and sim states and the per-session resources.

use bevy::prelude::*;

use crate::SimSet;
use crate::capture::CaptureSphereRadius;
use crate::orbit::cache::OrbitalCache;
use crate::orbit::catalog::load_dataset_entities;
use crate::time::{SimConfig, WorldTime, setup_time};

#[derive(States, Debug, Clone, Eq, PartialEq, Hash, Default)]
pub enum UiScreen {
    #[default]
    WorkingDirectorySetup,
    Home,
    Sim,
}

#[derive(States, Debug, Clone, Eq, PartialEq, Hash, Default)]
pub enum SimState {
    #[default]
    Setup,
    Running,
}

pub struct SessionPlugin;

impl Plugin for SessionPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<UiScreen>()
            .init_state::<SimState>()
            .configure_sets(OnEnter(UiScreen::Sim), SimSet::SessionInit)
            .add_systems(
                OnEnter(UiScreen::Sim),
                (init_sim_resources, load_dataset_entities, setup_time)
                    .chain()
                    .in_set(SimSet::SessionInit),
            )
            .add_systems(OnExit(UiScreen::Sim), remove_sim_resources);
    }
}

fn init_sim_resources(mut commands: Commands, config: Res<SimConfig>) {
    commands.init_resource::<OrbitalCache>();
    commands.insert_resource(WorldTime::starting_at(config.start_epoch));
    commands.insert_resource(CaptureSphereRadius { radius: 25.0 });
}

fn remove_sim_resources(mut commands: Commands) {
    commands.remove_resource::<OrbitalCache>();
    commands.remove_resource::<WorldTime>();
    commands.remove_resource::<CaptureSphereRadius>();
}
