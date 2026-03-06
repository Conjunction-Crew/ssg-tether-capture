mod components;
mod constants;
mod plugins;
mod resources;
mod systems;
mod tests;
mod ui;

use avian3d::prelude::*;
use avian3d::schedule::PhysicsSystems;
use bevy::camera::visibility::RenderLayers;
use bevy::post_process::auto_exposure::AutoExposurePlugin;
use bevy::prelude::*;
use systems::propagation::ssg_propagate_keplerian;
use systems::setup::*;

use crate::constants::MAP_LAYER;
use crate::plugins::orbit_camera::OrbitCameraPlugin;
use crate::plugins::orbital_mechanics::OrbitalMechanicsPlugin;
use crate::resources::celestials::Celestials;
use crate::resources::orbital_entities::OrbitalEntities;
use crate::resources::time_warp::TimeWarp;
use crate::systems::gizmos::orbital_gizmos;
use crate::systems::propagation::{floating_origin, physics_bubble_add_remove, target_entity_reset_origin};
use crate::systems::user_input::{change_time_warp, toggle_map_view, toggle_origin};
use crate::systems::user_interface::{map_orbitals, track_objects};
use crate::ui::plugin::UiPlugin;

// Main entrypoint to run the desktop application.
pub fn run() {
    let mut app = create_app();
    app.add_plugins(DefaultPlugins.build())
        .add_plugins(UiPlugin)
        .add_plugins(AutoExposurePlugin)
        .add_systems(
            Startup,
            (
                setup_lighting,
                (setup_celestial, setup_tether, setup_entities).chain(),
            ),
        )
        .add_systems(Last, orbital_gizmos)
        .insert_gizmo_config(
            DefaultGizmoConfigGroup,
            GizmoConfig {
                render_layers: RenderLayers::layer(MAP_LAYER),
                ..default()
            },
        )
        .run();
}

// Create the bevy application.
// Shared plugins between desktop application and tests go here.
pub fn create_app() -> App {
    let mut app = App::new();
    app.add_plugins(PhysicsPlugins::default())
        .add_plugins(OrbitalMechanicsPlugin)
        .add_plugins(OrbitCameraPlugin)
        .add_systems(
            Update,
            (
                ssg_propagate_keplerian,
                toggle_map_view,
                toggle_origin,
                change_time_warp,
                track_objects,
                map_orbitals,
            ),
        )
        .add_systems(PostUpdate, floating_origin)
        .add_systems(
            FixedPostUpdate,
            (
                physics_bubble_add_remove.in_set(PhysicsSystems::First),
                target_entity_reset_origin.in_set(PhysicsSystems::First),
                ssg_propagate_keplerian.in_set(PhysicsSystems::Last),
            ),
        )
        .insert_resource(ClearColor(Color::srgb(0.0, 0.0, 0.0)))
        .insert_resource(Gravity(Vec3::ZERO))
        .init_resource::<Celestials>()
        .init_resource::<OrbitalEntities>()
        .init_resource::<TimeWarp>();

    app
}
