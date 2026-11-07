pub mod catalog;
mod interactions;
pub mod modals;
pub mod orbital_selection;
pub mod sidebar;
mod spawn;

use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;

use crate::render::layers::{MAP_LAYER, UI_LAYER};
use crate::ui::catalog_state::{FilteredSpaceCatalogResults, SpaceCatalogUiState};
use crate::ui::events::UiEvent;
use crate::ui::form::{NewCapturePlanForm, SimPlanSyncState};
use crate::ui::screen::UiScreen;
use crate::ui::theme::UiTheme;
use ssg_sim::capture::CaptureComponent;
use ssg_sim::capture::plan::CapturePlanLibrary;
use ssg_sim::orbit::catalog::SpaceObjectCatalog;
use ssg_sim::orbit::selection::OrbitalSelectionState;
use ssg_sim::project::{SelectedProject, WorkingDirectory};
use ssg_sim::session::SimState;
use ssg_sim::telemetry::{LogEvent, LogLevel};

/// The egui header's height.
pub(crate) const SIM_HEADER_HEIGHT: f32 = 72.0;

/// The exit-confirm modal is open.
#[derive(Resource, Default)]
pub struct ExitConfirmOpen(pub bool);

#[derive(Component)]
pub struct SimScreen;

pub use catalog::{
    refresh_space_catalog_results, reset_space_catalog_ui_state, space_catalog_panel,
};
pub use interactions::sim_screen_events;
pub use orbital_selection::restore_catalog_sources;
pub use sidebar::{sim_screen_modals, sim_screen_panels};
pub use spawn::{cleanup_project_detail_screen, spawn_project_detail_screen};
