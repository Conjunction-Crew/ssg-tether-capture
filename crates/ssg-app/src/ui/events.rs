use bevy::prelude::*;

use crate::ui::catalog_state::{OrbitalElementField, OrbitalSelectionRole};
use crate::ui::form::UnitSystem;
use ssg_sim::project::manifest::RelativePreset;

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
    /// + Add Transition in the form's phase at this index.
    AddTransition(usize),
    /// × on a transition of the form's phase.
    RemoveTransition {
        phase: usize,
        index: usize,
    },
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
    /// Select (F-45): opens the catalog in the map view (one catalog for both
    /// roles; Use Selected names the role).
    SelectFromCatalog,
    /// Use Selected: the selected catalog object becomes this role's orbit and
    /// the catalog row selection is cleared (F-45).
    UseCatalogSelection(OrbitalSelectionRole),
    /// Clear (F-45): this role has no orbit again.
    ClearOrbitalSelection(OrbitalSelectionRole),
    /// Create Custom: this role's orbit becomes the ISS defaults.
    CreateCustomOrbit(OrbitalSelectionRole),
    /// Relative to Target (P9 S5): the chaser `offset_m` metres from the target on
    /// `preset`'s line, written to `project.json` as `relative_to_target`.
    PlaceChaserRelative {
        preset: RelativePreset,
        offset_m: f64,
    },
    /// A slider moved: writes the element (in the slider's units) to the role's orbit.
    SetOrbitalElement {
        role: OrbitalSelectionRole,
        field: OrbitalElementField,
        value: f32,
    },
}
