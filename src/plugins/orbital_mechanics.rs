use avian3d::{
    PhysicsPlugins,
    prelude::{PhysicsSchedule, PhysicsSystems},
};
use bevy::{ecs::schedule::ScheduleLabel, prelude::*};

use crate::{
    resources::{
        capture_plans::CaptureSphereRadius, celestials::Celestials, orbital_entities::OrbitalEntities, settings::Settings, world_time::WorldTime
    },
    systems::{
        capture_algorithms::capture_state_machine_update, gizmos::dev_gizmos, physics::fixed_physics_step, propagation::{
            floating_origin, init_orbitals, physics_bubble_add_remove, ssg_propagate_keplerian,
            target_entity_reset_origin,
        }
    },
    ui::state::UiScreen,
};

#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
pub struct ManualPhysics;

pub struct OrbitalMechanicsPlugin;

impl Plugin for OrbitalMechanicsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(PhysicsPlugins::new(ManualPhysics))
            .add_systems(OnEnter(UiScreen::Sim), init_sim_resources)
            .add_systems(OnExit(UiScreen::Sim), remove_sim_resources)
            .add_systems(
                Update,
                (init_orbitals, fixed_physics_step, dev_gizmos)
                    .chain()
                    .run_if(in_state(UiScreen::Sim)),
            )
            .add_systems(
                ManualPhysics,
                (
                    (physics_bubble_add_remove, target_entity_reset_origin)
                        .chain()
                        .in_set(PhysicsSystems::First),
                    (
                        ssg_propagate_keplerian,
                        floating_origin,
                        capture_state_machine_update,
                    )
                        .chain()
                        .in_set(PhysicsSystems::Last),
                )
                    .run_if(in_state(UiScreen::Sim)),
            );
    }
}

fn init_sim_resources(mut commands: Commands) {
    commands.init_resource::<Celestials>();
    commands.init_resource::<OrbitalEntities>();
    commands.init_resource::<WorldTime>();
    commands.init_resource::<Settings>();
    commands.insert_resource(CaptureSphereRadius { radius: 25.0 });
}

fn remove_sim_resources(mut commands: Commands) {
    commands.remove_resource::<Celestials>();
    commands.remove_resource::<OrbitalEntities>();
    commands.remove_resource::<WorldTime>();
    commands.remove_resource::<Settings>();
    commands.remove_resource::<CaptureSphereRadius>();
}
