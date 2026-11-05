mod catalog;
mod interactions;
pub mod modals;
mod orbital_selection;
pub mod sidebar;
mod spawn;

use bevy::camera::visibility::RenderLayers;
use bevy::ecs::observer::On;
use bevy::input::ButtonState;
use bevy::input::keyboard::KeyboardInput;
use bevy::input::mouse::MouseScrollUnit;
use bevy::picking::events::{Pointer, Scroll};
use bevy::prelude::*;

use crate::render::layers::{MAP_LAYER, UI_LAYER};
use crate::ui::catalog_state::{FilteredSpaceCatalogResults, SpaceCatalogUiState};
use crate::ui::events::UiEvent;
use crate::ui::form::{NewCapturePlanForm, SimPlanSyncState};
use crate::ui::screen::UiScreen;
use crate::ui::screens::capture_plan::ScrollContentWrapper;
use crate::ui::theme::UiTheme;
use crate::ui::widgets::ScreenRoot;
use ssg_sim::capture::CaptureComponent;
use ssg_sim::capture::plan::CapturePlanLibrary;
use ssg_sim::orbit::catalog::SpaceObjectCatalog;
use ssg_sim::orbit::selection::OrbitalSelectionState;
use ssg_sim::project::{SelectedProject, WorkingDirectory};
use ssg_sim::session::SimState;
use ssg_sim::telemetry::{LogEvent, LogLevel};

/// The egui header's height; the bevy_ui catalog panel starts below it.
pub(crate) const SIM_HEADER_HEIGHT: f32 = 72.0;

/// The exit-confirm modal is open.
#[derive(Resource, Default)]
pub struct ExitConfirmOpen(pub bool);

#[derive(Component)]
pub struct SimScreen;

#[derive(Component)]
pub struct CatalogToggleButton;

#[derive(Component)]
pub struct CatalogPanel;

#[derive(Component)]
pub struct CatalogSearchField;

#[derive(Component)]
pub struct CatalogSearchText;

#[derive(Component)]
pub struct CatalogResultsSummary;

#[derive(Component)]
pub struct CatalogResultsList;

#[derive(Component)]
pub struct CatalogPageLabel;

#[derive(Component)]
pub struct CatalogPrevPageButton;

#[derive(Component)]
pub struct CatalogNextPageButton;

#[derive(Component)]
pub struct CatalogPointsToggleButton;

#[derive(Component)]
pub struct CatalogResultButton {
    pub slot: usize,
    pub entry_index: Option<usize>,
}

#[derive(Component)]
pub struct CatalogResultText {
    pub slot: usize,
}

const CATALOG_PAGE_SIZE: usize = 48;

pub use catalog::{
    catalog_interactions, catalog_keyboard_input, refresh_space_catalog_results,
    reset_space_catalog_ui_state, sync_space_catalog_ui,
};
pub use interactions::sim_screen_events;
pub use sidebar::{sim_screen_modals, sim_screen_panels};
pub use spawn::{cleanup_project_detail_screen, spawn_project_detail_screen};
