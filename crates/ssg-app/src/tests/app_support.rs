//! App-side test harness (P3 S2): the sim harness plus the app plugins and
//! resources the app-side tests need. Sim-side tests use `test_support`.

use bevy::prelude::*;

use crate::render::attach::AttachPlugin;
use crate::render::camera::OrbitCameraPlugin;
use crate::ui::catalog_state::SpaceCatalogUiState;
use crate::ui::screen::{ScreenPlugin, UiScreen};

pub(crate) use ssg_sim::test_support::{LogTap, start_running, tick};

/// [`ssg_sim::test_support::test_app`] plus the app's screen state (which drives
/// the sim session), the camera plugin and the UI state the keyboard and camera
/// systems read.
pub(crate) fn test_app() -> App {
    let mut app = ssg_sim::test_support::test_app();
    app.add_plugins((ScreenPlugin, OrbitCameraPlugin))
        .init_asset::<StandardMaterial>()
        .init_resource::<SpaceCatalogUiState>();
    app
}

/// [`test_app`] plus the app's attach systems (P7.5 S3, C5): the meshes, the
/// debris scene and the camera target the app puts on the sim's bodies. The glTF
/// load has no loader here, fails asynchronously and inserts nothing.
pub(crate) fn test_app_with_attach() -> App {
    let mut app = test_app();
    app.add_plugins(AttachPlugin);
    app
}

/// Switch to the app's sim screen, as the app does: the sim session becomes
/// active in the same update ([`ScreenPlugin`]); `setup_time` starts the clock
/// at the [`ssg_sim::time::SimConfig`] epoch.
pub(crate) fn enter_sim(app: &mut App) {
    app.world_mut()
        .resource_mut::<NextState<UiScreen>>()
        .set(UiScreen::Sim);
    app.update();
}
