//! App-side test harness (P3 S2): the sim harness plus the app plugins and
//! resources the app-side tests need. Sim-side tests use `test_support`.

use bevy::prelude::*;

use crate::render::camera::OrbitCameraPlugin;
use crate::ui::catalog_state::SpaceCatalogUiState;

pub(crate) use super::test_support::{LogTap, enter_sim, start_running, tick};

/// [`super::test_support::test_app`] plus the camera plugin and the UI state the
/// keyboard and camera systems read.
pub(crate) fn test_app() -> App {
    let mut app = super::test_support::test_app();
    app.add_plugins(OrbitCameraPlugin)
        .init_asset::<StandardMaterial>()
        .init_resource::<SpaceCatalogUiState>();
    app
}
