use bevy::prelude::*;

use crate::resources::new_capture_plan_form::UnitSystem;

#[derive(Message, Debug, Clone)]
pub enum UiEvent {
    OpenProject(String),
    BackToHome,
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
    CaptureDebris {
        entity: Option<Entity>,
        plan_id: String,
    },
    ToggleMapView,
    ToggleOrigin,
    ChangeTimeWarp { increase: bool },
    CycleCameraTarget,
    SetUnitSystem(UnitSystem),
}
