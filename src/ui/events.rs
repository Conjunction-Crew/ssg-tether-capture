use bevy::prelude::*;

use crate::resources::capture_plan_form::UnitSystem;

#[derive(Message, Debug, Clone)]
pub enum UiEvent {
    OpenProject(String),
    BackToHome,
    ShowExitConfirm,
    CancelExitConfirm,
    WorkingDirectorySelected(String),
    BrowseForWorkingDirectory,
    ChangeWorkingDirectory,
    OpenNewCapturePlanForm,
    CloseNewCapturePlanForm,
    AddApproachTransition,
    RemoveApproachTransition(usize),
    AddTerminalTransition,
    RemoveTerminalTransition(usize),
    SaveCapturePlan,
    ConfirmOverwriteCapturePlan,
    CancelOverwriteCapturePlan,
    EditCapturePlan(String),
    CaptureDebris {
        entity: Option<Entity>,
        plan_id: String,
    },
    ToggleMapView,
    ToggleCaptureGizmos,
    ChangeTimeWarp {
        increase: bool,
    },
    CycleCameraTarget,
    SetUnitSystem(UnitSystem),
}
