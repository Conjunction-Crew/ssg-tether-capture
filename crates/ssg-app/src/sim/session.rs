//! Session lifecycle: the screen and sim states and the per-session resources.

use avian3d::{PhysicsPlugins, prelude::PhysicsSystems};
use bevy::prelude::*;

use crate::render::celestial::Celestials;
use crate::render::floating_origin_visuals::floating_origin_update_visuals;
use crate::render::gizmos::{capture_gizmos, dev_gizmos};
use crate::sim::capture::{CaptureSphereRadius, capture_state_machine_update};
use crate::sim::floating_origin::{calculate_com_rv, target_entity_reset_origin};
use crate::sim::orbit::cache::OrbitalCache;
use crate::sim::orbit::catalog::load_dataset_entities;
use crate::sim::orbit::{cache_eci_states, init_orbitals};
use crate::sim::physics_bubble::physics_bubble_add_remove;
use crate::sim::time::{ManualPhysics, WorldTime, fixed_physics_step, setup_time};

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

pub struct OrbitalMechanicsPlugin;

impl Plugin for OrbitalMechanicsPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<SimState>()
            .add_plugins(PhysicsPlugins::new(ManualPhysics))
            .add_systems(
                OnEnter(UiScreen::Sim),
                (init_sim_resources, load_dataset_entities, setup_time).chain(),
            )
            .add_systems(OnExit(UiScreen::Sim), remove_sim_resources)
            .add_systems(First, init_orbitals.run_if(in_state(UiScreen::Sim)))
            .add_systems(
                FixedUpdate,
                fixed_physics_step
                    .run_if(in_state(UiScreen::Sim))
                    .run_if(in_state(SimState::Running)),
            )
            .add_systems(
                Update,
                (dev_gizmos, capture_gizmos, floating_origin_update_visuals)
                    .run_if(in_state(UiScreen::Sim))
                    .run_if(in_state(SimState::Running)),
            )
            .add_systems(
                ManualPhysics,
                ((
                    calculate_com_rv,
                    target_entity_reset_origin,
                    cache_eci_states,
                    physics_bubble_add_remove,
                    capture_state_machine_update,
                )
                    .chain()
                    .in_set(PhysicsSystems::Last),)
                    .run_if(in_state(UiScreen::Sim)),
            );
    }
}

fn init_sim_resources(mut commands: Commands) {
    commands.init_resource::<Celestials>();
    commands.init_resource::<OrbitalCache>();
    commands.init_resource::<WorldTime>();
    commands.insert_resource(CaptureSphereRadius { radius: 25.0 });
}

fn remove_sim_resources(mut commands: Commands) {
    commands.remove_resource::<Celestials>();
    commands.remove_resource::<OrbitalCache>();
    commands.remove_resource::<WorldTime>();
    commands.remove_resource::<CaptureSphereRadius>();
}
