mod form;
mod interactions;
mod modal;

use bevy::camera::visibility::RenderLayers;
use bevy::ecs::hierarchy::ChildSpawnerCommands;
use bevy::ecs::observer::On;
use bevy::input::mouse::MouseScrollUnit;
use bevy::picking::Pickable;
use bevy::picking::events::{Pointer, Scroll};
use bevy::prelude::*;
use bevy::ui_widgets::{ControlOrientation, CoreScrollbarThumb, Scrollbar};

use crate::ui::events::UiEvent;
use crate::ui::form::{NewCapturePlanForm, TransitionForm, UnitSystem};
use crate::ui::theme::UiTheme;
use crate::ui::widgets::{InputField, InputFieldText};
use ssg_sim::session::UiScreen;

// ── Marker components ──────────────────────────────────────────────────────

#[derive(Component)]
pub struct CapturePlanModal;

#[derive(Component)]
pub struct NewPlanCancelButton;

#[derive(Component)]
pub struct NewPlanSaveButton;

#[derive(Component)]
pub struct AddApproachTransitionButton;

#[derive(Component)]
pub struct RemoveApproachTransitionButton(pub usize);

#[derive(Component)]
pub struct AddTerminalTransitionButton;

#[derive(Component)]
pub struct RemoveTerminalTransitionButton(pub usize);

#[derive(Component)]
pub struct ConfirmOverwriteButton;

#[derive(Component)]
pub struct CancelOverwriteButton;

#[derive(Component)]
pub struct CapturePlanScrollBody;

#[derive(Component)]
pub struct UnitMetricButton;

#[derive(Component)]
pub struct UnitImperialButton;

#[derive(Component, PartialEq, Eq, Clone, Copy, Debug)]
pub enum DropdownField {
    To,
    Condition,
}

#[derive(Component, Clone)]
pub struct DropdownTrigger {
    pub is_approach: bool,
    pub index: usize,
    pub field: DropdownField,
}

#[derive(Component, Clone)]
pub struct DropdownOptions {
    pub is_approach: bool,
    pub index: usize,
    pub field: DropdownField,
}

#[derive(Component, Clone)]
pub struct DropdownOption {
    pub is_approach: bool,
    pub index: usize,
    pub field: DropdownField,
    pub value: String,
}

#[derive(Component)]
pub struct DropdownTriggerLabel;

#[derive(Component)]
pub struct ScrollContentWrapper;

// ── Field ID tags so the keyboard system knows which field to update ──────

#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub enum FormFieldId {
    PlanName,
    TetherName,
    TetherLength,
    ApproachMaxVelocity,
    ApproachMaxForce,
    ApproachTransitionDistanceValue(usize),
    TerminalMaxVelocity,
    TerminalMaxForce,
    TerminalShrinkRate,
    TerminalTransitionDistanceValue(usize),
    CaptureMaxVelocity,
    CaptureMaxForce,
    CaptureShrinkRate,
}

#[derive(Component, Debug, Clone)]
pub struct TetherTypeRadioButton(pub String);

pub use form::{build_capture_plan_json, generate_filename, validate_form};
pub use interactions::{
    capture_plan_interactions, dropdown_interactions, sync_dropdown_labels, sync_form_fields,
    tether_type_radio_interactions,
};
pub use modal::spawn_capture_plan_modal;
