mod components;
mod systems;
mod tests;
mod plugins;

use avian3d::prelude::*;
use plugins::post_processing_test::PostProcessPlugin;
use bevy::prelude::*;
use systems::orbit_camera::*;
use systems::setup::*;
use systems::propagation::ssg_propagate_keplerian;

// Main entrypoint to run the desktop application.
pub fn run() {
    let mut app = create_app();
    app.add_plugins(DefaultPlugins.build().disable::<TransformPlugin>())
        .add_plugins(PostProcessPlugin)
        .add_systems(Startup, (setup_celestial, setup_lighting, setup_physics).chain())
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
                orbit_camera_sync_rotation.after(orbit_camera_input),
                orbit_camera_track,
                orbit_camera_switch_target,
                orbit_camera_control_target,
                ssg_propagate_keplerian,
            ),
        )
        .insert_resource(ClearColor(Color::srgb(0.0, 0.0, 0.0)))
        .insert_resource(Gravity(Vec3::ZERO));

    app
}
