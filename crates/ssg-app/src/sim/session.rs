//! Session lifecycle: the screen and sim states and the per-session resources.

use bevy::prelude::*;

use crate::sim::SimSet;
use crate::sim::capture::CaptureSphereRadius;
use crate::sim::orbit::cache::OrbitalCache;
use crate::sim::orbit::catalog::load_dataset_entities;
use crate::sim::time::{WorldTime, setup_time};

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

fn init_sim_resources(mut commands: Commands) {
    commands.init_resource::<OrbitalCache>();
    commands.init_resource::<WorldTime>();
    commands.insert_resource(CaptureSphereRadius { radius: 25.0 });
}

fn remove_sim_resources(mut commands: Commands) {
    commands.remove_resource::<OrbitalCache>();
    commands.remove_resource::<WorldTime>();
    commands.remove_resource::<CaptureSphereRadius>();
}
