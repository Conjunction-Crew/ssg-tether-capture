mod render;
mod tests;
mod ui;

use bevy::input_focus::{InputDispatchPlugin, tab_navigation::TabNavigationPlugin};
use bevy::post_process::auto_exposure::AutoExposurePlugin;
use bevy::prelude::*;
use bevy::ui_widgets::UiWidgetsPlugins;
use bevy_egui::EguiPlugin;

use crate::render::RenderPlugins;
use crate::ui::plugin::UiPlugin;
use ssg_sim::SimPlugins;
use ssg_sim::project::AssetsDir;

/// Returns the absolute path to the `assets/` directory as a [`PathBuf`].
///
/// On macOS, when running inside a `.app` bundle, assets live at
/// `Contents/Resources/assets/` and are not reachable via a relative path
/// regardless of CWD. Detect this case and return an absolute path so any
/// runtime file-system access finds them however the app was launched
/// (Finder, Spotlight, CLI `open`, etc.).
pub(crate) fn resolve_assets_dir() -> std::path::PathBuf {
    #[cfg(target_os = "macos")]
    if let Some(path) = std::env::current_exe().ok().and_then(|exe| {
        let assets = exe.parent()?.parent()?.join("Resources").join("assets");
        assets.is_dir().then_some(assets)
    }) {
        return path;
    }
    std::path::PathBuf::from("assets")
}

fn resolve_asset_path() -> String {
    resolve_assets_dir().to_string_lossy().into_owned()
}

// Main entrypoint to run the desktop application.
pub fn run() {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(AssetPlugin {
        file_path: resolve_asset_path(),
        ..default()
    }))
    .add_plugins(InputDispatchPlugin)
    .add_plugins(TabNavigationPlugin)
    .add_plugins(UiWidgetsPlugins)
    .add_plugins(EguiPlugin::default())
    .add_plugins(AutoExposurePlugin)
    .insert_resource(AssetsDir(resolve_assets_dir()))
    .add_plugins(SimPlugins)
    .add_plugins(RenderPlugins)
    .add_plugins(UiPlugin)
    .run();
}
