mod catalog;
mod interactions;
mod modals;
mod orbital_selection;
mod overlays;
mod spawn;

use avian3d::prelude::{RigidBodyDisabled, RigidBodyQueryReadOnly};
use bevy::camera::visibility::RenderLayers;
use bevy::ecs::hierarchy::ChildSpawnerCommands;
use bevy::ecs::observer::On;
use bevy::input::ButtonState;
use bevy::input::keyboard::KeyboardInput;
use bevy::input::mouse::MouseScrollUnit;
use bevy::picking::Pickable;
use bevy::picking::events::{Pointer, Scroll};
use bevy::prelude::*;
use bevy::ui_widgets::{
    ControlOrientation, CoreScrollbarThumb, Scrollbar, Slider, SliderPrecision, SliderRange,
    SliderStep, SliderThumb, SliderValue, TrackClick, ValueChange,
};

use crate::render::layers::{MAP_LAYER, SCENE_LAYER, UI_LAYER, eci_position_to_map};
use crate::ui::catalog_state::{
    FilteredSpaceCatalogResults, OrbitalSelectionRole, SpaceCatalogUiState,
};
use crate::ui::events::UiEvent;
use crate::ui::form::{NewCapturePlanForm, SimPlanSyncState};
use crate::ui::hud::{
    CaptureGuidanceReadout, CaptureTelemetryReadout, OrbitLabel, TimeWarpReadout,
};
use crate::ui::screens::capture_plan::ScrollContentWrapper;
use crate::ui::theme::UiTheme;
use crate::ui::widgets::ScreenRoot;
use ssg_sim::capture::CaptureComponent;
use ssg_sim::capture::plan::CapturePlanLibrary;
use ssg_sim::orbit::ISS_ORBIT;
use ssg_sim::orbit::cache::OrbitalCache;
use ssg_sim::orbit::catalog::{
    GpuComputeEpochOrigin, GpuElements, SpaceObjectCatalog, propagate_catalog_eci_state,
};
use ssg_sim::orbit::selection::{
    EditableOrbitalElements, OrbitalSelectionSource, OrbitalSelectionState, SelectedOrbitalObject,
};
use ssg_sim::project::{SelectedProject, WorkingDirectory};
use ssg_sim::session::SimState;
use ssg_sim::telemetry::{LogEvent, LogLevel};

#[derive(Component)]
pub struct SimScreen;

#[derive(Component)]
pub struct BackButton;

#[derive(Component)]
pub struct CaptureButton {
    pub entity: Option<Entity>,
    pub plan_id: String,
}

#[derive(Component, PartialEq, Eq, Clone, Copy)]
pub enum CollapsibleSection {
    ProjectInformation,
    OrbitalSelection,
    TimeWarp,
    SimulationControls,
    SimulationHud,
    Reference,
}

#[derive(Component)]
pub struct CollapsibleToggle {
    pub section: CollapsibleSection,
}

#[derive(Component)]
pub struct CollapsibleContent {
    pub section: CollapsibleSection,
}

#[derive(Component)]
pub struct StartSimButton;

#[derive(Component)]
pub struct UseCatalogSelectionButton {
    pub role: OrbitalSelectionRole,
}

#[derive(Component)]
pub struct CreateCustomOrbitButton {
    pub role: OrbitalSelectionRole,
}

#[derive(Component)]
pub struct CurrentCatalogSelectionText;

#[derive(Component)]
pub struct OrbitalSelectionSummaryText {
    pub role: OrbitalSelectionRole,
}

#[derive(Component)]
pub struct OrbitalSelectionSliderGroup {
    pub role: OrbitalSelectionRole,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrbitalElementField {
    SemiMajorAxis,
    Eccentricity,
    Inclination,
    Raan,
    ArgPerigee,
    MeanAnomaly,
}

#[derive(Component)]
pub struct OrbitalElementSlider {
    pub role: OrbitalSelectionRole,
    pub field: OrbitalElementField,
}

#[derive(Component)]
pub struct OrbitalElementSliderFill {
    pub role: OrbitalSelectionRole,
    pub field: OrbitalElementField,
}

#[derive(Component)]
pub struct OrbitalElementSliderThumb {
    pub role: OrbitalSelectionRole,
    pub field: OrbitalElementField,
}

#[derive(Component)]
pub struct OrbitalElementSliderValueText {
    pub role: OrbitalSelectionRole,
    pub field: OrbitalElementField,
}

#[derive(Component)]
pub struct MapViewButton;

#[derive(Component)]
pub struct TimeWarpDecreaseButton;

#[derive(Component)]
pub struct TimeWarpIncreaseButton;

#[derive(Component)]
pub struct CycleCameraButton;

#[derive(Component)]
pub struct ToggleCaptureGizmosButton;

#[derive(Component)]
pub struct SidebarPanel;

#[derive(Component)]
pub struct ExitSimConfirmModal;

#[derive(Component)]
pub struct ExitSimCancelButton;

#[derive(Component)]
pub struct ExitSimConfirmButton;

#[derive(Component)]
pub struct ViewEditPlanButton;

#[derive(Component)]
pub struct RestartPromptModal;

#[derive(Component)]
pub struct RestartSimButton;

#[derive(Component)]
pub struct DismissRestartButton;

#[derive(Component)]
pub struct RecompileLiveButton;

#[derive(Component)]
pub struct SyncIndicator;

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
pub struct SatelliteIndicatorToggleButton;

#[derive(Component)]
pub struct CatalogResultButton {
    pub slot: usize,
    pub entry_index: Option<usize>,
}

#[derive(Component)]
pub struct CatalogResultText {
    pub slot: usize,
}

#[derive(Component)]
pub struct SelectedCatalogOverlay;

#[derive(Component)]
pub struct SelectedCatalogOverlayLabel;

#[derive(Component)]
pub struct SatelliteIndicatorOverlay {
    pub entity: Option<Entity>,
    pub label: String,
}

#[derive(Component)]
pub struct SatelliteIndicatorOverlayLabel;

const CATALOG_PAGE_SIZE: usize = 48;
const SATELLITE_SCENE_INDICATOR_HALF_EXTENT: f32 = 12.0;
const MAP_OVERLAY_BOX_SIZE_PX: f32 = 30.0;

fn position_overlay_at_viewport_center(node: &mut Node, center: Vec2, size_px: f32) {
    let half_size = size_px * 0.5;

    node.display = Display::Flex;
    node.left = px(center.x - half_size);
    node.top = px(center.y - half_size);
    node.width = px(size_px);
    node.height = px(size_px);
}
const ORBITAL_ELEMENT_FIELDS: [OrbitalElementField; 6] = [
    OrbitalElementField::SemiMajorAxis,
    OrbitalElementField::Eccentricity,
    OrbitalElementField::Inclination,
    OrbitalElementField::Raan,
    OrbitalElementField::ArgPerigee,
    OrbitalElementField::MeanAnomaly,
];

pub use catalog::{
    catalog_interactions, catalog_keyboard_input, refresh_space_catalog_results,
    reset_space_catalog_ui_state, sync_space_catalog_ui,
};
pub use interactions::{
    collapsible_toggle_interaction, project_detail_interactions, restart_prompt_interactions,
    update_sync_indicator, view_edit_plan_interactions,
};
pub use modals::{spawn_exit_confirm_modal, spawn_restart_prompt_modal};
pub use orbital_selection::{orbital_selection_interactions, sync_orbital_selection_ui};
pub use overlays::{update_satellite_indicator_overlay, update_selected_catalog_overlay};
pub use spawn::{cleanup_project_detail_screen, spawn_project_detail_screen};
