mod components;
mod systems;
mod tests;

use avian3d::prelude::*;
use avian3d::schedule::PhysicsSystems;
use bevy::prelude::*;
use systems::orbit_camera::*;
use systems::propagation::ssg_propagate_keplerian;
use systems::setup::*;

use crate::systems::propagation::{floating_origin, target_entity_reset_origin};
use crate::systems::user_interface::track_objects;

// Main entrypoint to run the desktop application.
pub fn run() {
    let mut app = create_app();
    app.add_plugins(DefaultPlugins.build().disable::<TransformPlugin>())
        .add_systems(Startup, (setup_lighting, setup_scene, setup_tether))
        .run();
}

// Create the bevy application.
// Shared plugins between desktop application and tests go here.
pub fn create_app() -> App {
    let mut app = App::new();
    app.add_plugins(PhysicsPlugins::default())
        .add_systems(
            Update,
            (
                orbit_camera_input,
                orbit_camera_track,
                orbit_camera_switch_target,
                orbit_camera_control_target,
                ssg_propagate_keplerian,
                floating_origin,
                track_objects,
            ),
        )
        .add_systems(
            FixedPostUpdate,
            target_entity_reset_origin.in_set(PhysicsSystems::First),
        )
        .insert_resource(ClearColor(Color::srgb(0.0, 0.0, 0.0)))
        .insert_resource(Gravity(Vec3::ZERO));

    app
}
