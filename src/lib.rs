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
use bevy::math::DVec3;
use bevy::post_process::auto_exposure::AutoExposurePlugin;
use bevy::prelude::*;
use bevy::transform::TransformSystems;
use bevy::ui_widgets::UiWidgetsPlugins;
use systems::setup::*;

use crate::constants::{MAP_LAYER, SCENE_LAYER};
use crate::plugins::gpu_compute::GpuComputePlugin;
use crate::plugins::orbit_camera::OrbitCameraPlugin;
use crate::plugins::orbital_mechanics::OrbitalMechanicsPlugin;
use crate::resources::capture_plans::CapturePlanLibrary;
use crate::resources::settings::Settings;
use crate::resources::space_catalog::{
    FilteredSpaceCatalogResults, SpaceCatalogUiState, SpaceObjectCatalog,
};
use crate::systems::gizmos::{CaptureGizmoConfigGroup, orbital_gizmos};
use crate::systems::physics::FIXED_HZ;
use crate::systems::user_input::{
    change_time_warp, toggle_capture_gizmos, toggle_map_view, toggle_origin,
};
use crate::systems::user_interface::{
    map_orbitals, update_capture_guidance, update_capture_telemetry, update_time_warp_readout,
};
use crate::ui::plugin::UiPlugin;
use crate::ui::state::UiScreen;

/// Returns the absolute path to the `assets/` directory.
///
/// On macOS, when running inside a `.app` bundle, assets live at
/// `Contents/Resources/assets/` and are not reachable via a relative path
/// regardless of CWD. Detect this case and return an absolute path so Bevy's
/// `AssetPlugin` finds them however the app was launched (Finder, Spotlight,
/// CLI `open`, etc.).
fn resolve_asset_path() -> String {
    #[cfg(target_os = "macos")]
    if let Some(path) = std::env::current_exe().ok().and_then(|exe| {
        let assets = exe.parent()?.parent()?.join("Resources").join("assets");
        assets
            .is_dir()
            .then(|| assets.to_string_lossy().into_owned())
    }) {
        return path;
    }
    "assets".to_string()
}

// Main entrypoint to run the desktop application.
pub fn run() {
    let mut app = create_app();
    app.add_plugins(DefaultPlugins.set(AssetPlugin {
        file_path: resolve_asset_path(),
        ..default()
    }))
    .add_plugins(InputDispatchPlugin)
    .add_plugins(TabNavigationPlugin)
    .add_plugins(UiWidgetsPlugins)
    .add_plugins(UiPlugin)
    .add_plugins(AutoExposurePlugin)
    .add_plugins(GpuComputePlugin)
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
                toggle_capture_gizmos,
                change_time_warp,
                update_time_warp_readout,
                update_capture_telemetry,
                update_capture_guidance,
            )
                .run_if(in_state(UiScreen::Sim)),
        )
        .add_systems(
            PostUpdate,
            map_orbitals
                .after(TransformSystems::Propagate)
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
        .insert_resource(Gravity(DVec3::ZERO))
        .insert_resource(SubstepCount(4)) // Avian3d Substep Count
        .insert_resource(Time::<Fixed>::from_hz(FIXED_HZ)) // FixedUpdate rate (physics.rs)
        .init_resource::<CapturePlanLibrary>()
        .init_resource::<SpaceObjectCatalog>()
        .init_resource::<SpaceCatalogUiState>()
        .init_resource::<FilteredSpaceCatalogResults>()
        .init_resource::<Settings>();

    app
}
