use bevy::prelude::*;

use crate::ui::form::UnitSystem;

#[derive(Message, Debug, Clone)]
pub enum UiEvent {
    OpenProject(String),
    StartSim,
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
    /// Reset Sim / Restart Sim: restarts through Home with the current plan.
    RestartSim,
    /// Apply Live: the changed plan takes effect without a restart.
    ApplyPlanLive,
    /// Continue on the restart prompt: the sim stays out of sync.
    DismissRestartPrompt,
    ToggleSatelliteIndicator,
}
