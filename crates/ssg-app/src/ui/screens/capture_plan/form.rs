//! The plan form's model on the v1 [`CapturePlan`] (P9 S1, F-15): a plan is
//! loaded into the form's text, and Save writes the text back into a clone of
//! the loaded plan, validates it with `validate_capture_plan` and writes it with
//! the v1 writer. For every valid plan P, an unchanged form gives P back.

use std::path::Path;

use ssg_sim::capture::plan::{
    CapturePlan, CapturePlanDevice, ConditionBounds, Transition, validate_capture_plan,
};
use ssg_sim::project::plan_files::save_plan;
use ssg_sim::telemetry::LogLevel;

use super::*;
use crate::ui::form::{
    ConditionForm, ConditionKind, FormError, NumberError, NumberField, PhaseForm, Quantity,
};

/// The example plan + New Plan starts from (A-2).
pub const NEW_PLAN_TEMPLATE: &str = "example_plan";

pub fn generate_filename(plan_name: &str) -> String {
    // Characters illegal on Windows (superset of what POSIX disallows)
    const ILLEGAL: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*'];
    let sanitized: String = plan_name
        .to_lowercase()
        .chars()
        .map(|c| {
            if c == ' ' || ILLEGAL.contains(&c) || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    format!("{}.json", sanitized)
}

/// The JSON Pointers of the plan values the form shows a field for: Save's
/// messages about them are shown under the field, the others in the list.
pub mod pointer {
    pub const NAME: &str = "/name";
    pub const TETHER: &str = "/tether";
    pub const TETHER_LENGTH: &str = "/device/tether_length_m";

    pub fn phase(i: usize, key: &str) -> String {
        format!("/phases/{i}/{key}")
    }

    pub fn transition(i: usize, j: usize, key: &str) -> String {
        format!("/phases/{i}/transitions/{j}/{key}")
    }
}

/// `UiEvent::EditCapturePlan`: fills the form from `plan` (read-only for an
/// example plan), shown in the toggle's units. An unchanged Save writes nothing.
pub fn load_plan_into_form(
    form: &mut NewCapturePlanForm,
    plan_id: &str,
    plan: &CapturePlan,
    is_example: bool,
) {
    fill(form, plan.clone());
    form.read_only = is_example;
    form.editing_plan_id = Some(plan_id.to_string());
}

/// `UiEvent::OpenNewCapturePlanForm`: a new plan with the phases and values of
/// `template` (the example plan, A-2) and a blank name. Without a template the
/// form has no phases and Save says that one is required.
pub fn new_plan_into_form(form: &mut NewCapturePlanForm, template: Option<&CapturePlan>) {
    let mut plan = template
        .cloned()
        .unwrap_or_else(|| NewCapturePlanForm::default().draft);
    plan.name = String::new();
    plan.id = String::new();
    fill(form, plan);
}

fn fill(form: &mut NewCapturePlanForm, plan: CapturePlan) {
    form.reset();
    let unit = form.unit_system;
    let device = plan.device.clone().unwrap_or_default();
    form.plan_name = plan.name.clone();
    form.tether_name = plan.tether.clone();
    form.tether_type = device.device_type.unwrap_or_default();
    form.tether_length = NumberField::shown(device.tether_length_m, Quantity::Length, unit);
    form.phases = plan
        .phases
        .iter()
        .map(|phase| PhaseForm {
            id: phase.id.clone(),
            max_velocity: NumberField::shown(Some(phase.max_velocity_mps), Quantity::Speed, unit),
            max_force: NumberField::shown(Some(phase.max_force_n), Quantity::Force, unit),
            shrink_rate: NumberField::shown(phase.shrink_rate_mps, Quantity::Speed, unit),
            transitions: phase
                .transitions
                .iter()
                .flatten()
                .map(|transition| TransitionForm {
                    to: transition.to.clone(),
                    conditions: ConditionKind::ALL
                        .into_iter()
                        .filter_map(|kind| {
                            let bounds = kind.bounds(&transition.all_of)?;
                            let quantity = kind.quantity();
                            Some(ConditionForm {
                                kind,
                                less_than: NumberField::shown(bounds.less_than, quantity, unit),
                                greater_than: NumberField::shown(
                                    bounds.greater_than,
                                    quantity,
                                    unit,
                                ),
                            })
                        })
                        .collect(),
                    base: transition.all_of.clone(),
                })
                .collect(),
        })
        .collect();
    form.draft = plan;
}

/// Parses `field` into metric, recording why it cannot be read at `pointer`.
fn number(
    field: &NumberField,
    quantity: Quantity,
    unit: UnitSystem,
    pointer: String,
    errors: &mut Vec<FormError>,
) -> Option<f64> {
    match field.metric(quantity, unit) {
        Ok(value) => value,
        Err(NumberError::NotANumber) => {
            errors.push(FormError {
                pointer,
                message: "must be a number".to_string(),
            });
            None
        }
    }
}

/// The plan the form describes: what it shows, written into a clone of the
/// loaded plan, then validated. `Err` lists every field that cannot be read and
/// every rule the plan breaks.
pub fn plan_from_form(form: &NewCapturePlanForm) -> Result<CapturePlan, Vec<FormError>> {
    let unit = form.unit_system;
    let mut errors = Vec::new();
    let mut plan = form.draft.clone();
    plan.name = form.plan_name.clone();
    plan.tether = form.tether_name.clone();

    let length = number(
        &form.tether_length,
        Quantity::Length,
        unit,
        pointer::TETHER_LENGTH.to_string(),
        &mut errors,
    );
    let device_type = Some(form.tether_type.clone()).filter(|t| !t.is_empty());
    if plan.device.is_some() || length.is_some() || device_type.is_some() {
        let device = plan.device.get_or_insert_with(CapturePlanDevice::default);
        device.device_type = device_type;
        device.tether_length_m = length;
    }

    for (i, (phase, shown)) in plan.phases.iter_mut().zip(&form.phases).enumerate() {
        let mut required = |field: &NumberField, quantity, key: &str, current: f64| {
            if field.text.trim().is_empty() {
                errors.push(FormError {
                    pointer: pointer::phase(i, key),
                    message: "is required".to_string(),
                });
                return current;
            }
            number(field, quantity, unit, pointer::phase(i, key), &mut errors).unwrap_or(current)
        };
        phase.max_velocity_mps = required(
            &shown.max_velocity,
            Quantity::Speed,
            "max_velocity_mps",
            phase.max_velocity_mps,
        );
        phase.max_force_n = required(
            &shown.max_force,
            Quantity::Force,
            "max_force_n",
            phase.max_force_n,
        );
        phase.shrink_rate_mps = number(
            &shown.shrink_rate,
            Quantity::Speed,
            unit,
            pointer::phase(i, "shrink_rate_mps"),
            &mut errors,
        );
        let transitions: Vec<Transition> = shown
            .transitions
            .iter()
            .enumerate()
            .map(|(j, transition)| {
                let mut all_of = transition.base.clone();
                for condition in &transition.conditions {
                    let quantity = condition.kind.quantity();
                    let mut bound = |field: &NumberField, name: &str| {
                        let at = format!("all_of/{}/{name}", condition.kind.key());
                        number(
                            field,
                            quantity,
                            unit,
                            pointer::transition(i, j, &at),
                            &mut errors,
                        )
                    };
                    let less_than = bound(&condition.less_than, "less_than");
                    let greater_than = bound(&condition.greater_than, "greater_than");
                    *condition.kind.bounds_mut(&mut all_of) = (less_than.is_some()
                        || greater_than.is_some())
                    .then_some(ConditionBounds {
                        less_than,
                        greater_than,
                    });
                }
                Transition {
                    to: transition.to.clone(),
                    all_of,
                }
            })
            .collect();
        // A phase that had `"transitions": []` keeps it; one without keeps none.
        phase.transitions = if transitions.is_empty() {
            phase.transitions.take().filter(Vec::is_empty)
        } else {
            Some(transitions)
        };
    }

    let plan_id = form.editing_plan_id.as_deref().unwrap_or_default();
    let prefix = format!("[{plan_id}] ");
    for message in validate_capture_plan(plan_id, &plan) {
        let message = message.strip_prefix(&prefix).unwrap_or(&message);
        let (pointer, message) = message.split_once(": ").unwrap_or(("/", message));
        // A field that could not be read is reported once.
        if !errors.iter().any(|error| error.pointer == pointer) {
            errors.push(FormError {
                pointer: pointer.to_string(),
                message: message.to_string(),
            });
        }
    }
    if errors.is_empty() {
        Ok(plan)
    } else {
        Err(errors)
    }
}

/// Every pointer the form shows a field (or a transition box) for.
pub fn field_pointers(form: &NewCapturePlanForm) -> Vec<String> {
    let mut pointers = vec![
        pointer::NAME.to_string(),
        pointer::TETHER.to_string(),
        pointer::TETHER_LENGTH.to_string(),
    ];
    for (i, phase) in form.phases.iter().enumerate() {
        for key in [
            "max_velocity_mps",
            "max_force_n",
            "shrink_rate_mps",
            "transitions",
        ] {
            pointers.push(pointer::phase(i, key));
        }
        for (j, transition) in phase.transitions.iter().enumerate() {
            pointers.push(pointer::transition(i, j, "to"));
            pointers.push(pointer::transition(i, j, "all_of"));
            for condition in &transition.conditions {
                let key = condition.kind.key();
                for at in [
                    format!("all_of/{key}"),
                    format!("all_of/{key}/less_than"),
                    format!("all_of/{key}/greater_than"),
                ] {
                    pointers.push(pointer::transition(i, j, &at));
                }
            }
        }
    }
    pointers
}

/// Save (`overwrite: false`) or Overwrite on the conflict prompt (`true`): an
/// unchanged edit closes the form; an invalid plan shows its errors; a new plan
/// whose file exists raises the overwrite prompt; otherwise the plan is written
/// to `dir`, the user plans are reloaded and the form closes. An edit saved while
/// a project is open asks for a restart unless only the name changed. Returns
/// whether a file was written.
pub(crate) fn save_plan_from_form(
    form: &mut NewCapturePlanForm,
    dir: &Path,
    overwrite: bool,
    project_open: bool,
    library: &mut ssg_sim::capture::plan::CapturePlanLibrary,
    log: &mut impl FnMut(LogLevel, String),
) -> bool {
    let plan = match plan_from_form(form) {
        Ok(plan) => plan,
        Err(errors) => {
            form.validation_errors = errors;
            return false;
        }
    };
    form.validation_errors.clear();
    let editing = form.editing_plan_id.is_some() && !overwrite;
    if editing && plan == form.draft {
        form.open = false;
        form.reset();
        return false;
    }
    // An edit always writes the file it was loaded from, so renaming a plan
    // changes only its `name`.
    let filename = match form.editing_plan_id.as_deref() {
        Some(plan_id) if !overwrite => generate_filename(plan_id),
        _ => generate_filename(&form.plan_name),
    };
    if !overwrite
        && !editing
        && form.overwrite_conflict_path.is_none()
        && dir.join(&filename).exists()
    {
        form.overwrite_conflict_path = Some(dir.join(&filename).to_string_lossy().to_string());
        return false;
    }
    if let Err(e) = save_plan(dir, &filename, &plan) {
        log(
            LogLevel::Error,
            format!("Failed to write capture plan '{}': {e}", form.plan_name),
        );
        return false;
    }
    library.reload_user_plans(dir);
    // `name` is not a sim parameter: a rename applies without a restart.
    let only_name_changed = CapturePlan {
        name: form.draft.name.clone(),
        ..plan
    } == form.draft;
    let restart = editing && project_open && !only_name_changed;
    log(
        LogLevel::Info,
        format!("Capture plan '{}' saved", form.plan_name),
    );
    form.open = false;
    form.reset();
    form.show_restart_prompt = restart;
    true
}
