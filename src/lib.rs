mod components;
mod constants;
mod plugins;
mod resources;
mod systems;
mod tests;
mod ui;

use avian3d::prelude::*;
use bevy::camera::visibility::RenderLayers;
use bevy::input_focus::{InputDispatchPlugin, tab_navigation::TabNavigationPlugin};
use bevy::post_process::auto_exposure::AutoExposurePlugin;
use bevy::prelude::*;
use bevy::ui_widgets::UiWidgetsPlugins;
use systems::setup::*;

use crate::constants::{MAP_LAYER, SCENE_LAYER};
use crate::plugins::orbit_camera::OrbitCameraPlugin;
use crate::plugins::orbital_mechanics::OrbitalMechanicsPlugin;
use crate::resources::capture_plans::CapturePlanLibrary;
use crate::systems::capture_algorithms::CaptureGizmoConfigGroup;
use crate::systems::gizmos::orbital_gizmos;
use crate::systems::physics::fixed_physics_step;
use crate::systems::propagation::floating_origin;
use crate::systems::user_input::{change_time_warp, toggle_map_view, toggle_origin};
use crate::systems::user_interface::{
    map_orbitals, update_capture_guidance, update_capture_telemetry, update_time_warp_readout,
};
use crate::ui::plugin::UiPlugin;
use crate::ui::state::UiScreen;

// Main entrypoint to run the desktop application.
pub fn run() {
    let mut app = create_app();
    app.add_plugins(DefaultPlugins.build())
        .add_plugins(InputDispatchPlugin)
        .add_plugins(TabNavigationPlugin)
        .add_plugins(UiWidgetsPlugins)
        .add_plugins(UiPlugin)
        .add_plugins(AutoExposurePlugin)
        .add_systems(
            OnEnter(UiScreen::Sim),
            ((
                setup_lighting,
                setup_celestial,
                setup_tether,
                setup_entities,
            )
                .chain(),),
        )
        .run();
}

// Create the bevy application.
// Shared plugins between desktop application and tests go here.
pub fn create_app() -> App {
    let mut app = App::new();
    app.add_plugins(OrbitalMechanicsPlugin)
        .add_plugins(OrbitCameraPlugin)
        .add_systems(
            Update,
            (
                toggle_map_view,
                toggle_origin,
                change_time_warp,
                update_time_warp_readout,
                update_capture_telemetry,
                update_capture_guidance,
                map_orbitals,
            )
                .run_if(in_state(UiScreen::Sim)),
        )
        .add_systems(Last, orbital_gizmos.run_if(in_state(UiScreen::Sim)))
        .insert_gizmo_config(
            DefaultGizmoConfigGroup,
            GizmoConfig {
                render_layers: RenderLayers::from_layers(&[SCENE_LAYER, MAP_LAYER]),
                ..default()
            },
        )
        .insert_gizmo_config(
            CaptureGizmoConfigGroup,
            GizmoConfig {
                render_layers: RenderLayers::layer(SCENE_LAYER),
                ..default()
            },
        )
        .insert_resource(ClearColor(Color::srgb(0.0, 0.0, 0.0)))
        .insert_resource(Gravity(Vec3::ZERO))
        .init_resource::<CapturePlanLibrary>();

    app
}
