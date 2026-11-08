//! The plan form's model on the v1 [`CapturePlan`] (P9 S1, F-15): a plan is
//! loaded into the form's text, and Save writes the text back into a clone of
//! the loaded plan, validates it with `validate_capture_plan` and writes it with
//! the v1 writer. For every valid plan P, an unchanged form gives P back.

use std::path::Path;

use ssg_sim::capture::gains::PhaseController;
use ssg_sim::capture::plan::{
    CapturePlan, CapturePlanDevice, CaptureSphere, ConditionBounds, Transition,
    validate_capture_plan,
};
use ssg_sim::project::plan_files::save_plan;
use ssg_sim::telemetry::LogLevel;

use super::*;
use crate::ui::form::{
    ConditionForm, ConditionKind, ControllerKey, FormError, NumberError, NumberField, PhaseForm,
    Quantity,
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
    pub const SPAWN_ORIENTATION: &str = "/device/spawn_orientation";

    /// The `device` numbers S2 shows, after the tether length.
    pub const DEVICE_NUMBERS: [&str; 5] = [
        "/device/segment_spacing_m",
        "/device/segment_mass_kg",
        "/device/end_mass_kg",
        "/device/joint_damping_linear",
        "/device/joint_damping_angular",
    ];
    pub const INITIAL_RADIUS: &str = "/capture_sphere/initial_radius_m";
    pub const MIN_RADIUS: &str = "/capture_sphere/min_radius_m";

    /// The phase keys S2 shows besides the limits and the transitions.
    pub const PHASE_KEYS: [&str; 6] = [
        "end",
        "actuation",
        "tail_actuation",
        "tail_spacing_m",
        "hold_plane",
        "velocity_scales_with_sphere",
    ];

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
    form.segment_spacing = NumberField::shown(device.segment_spacing_m, Quantity::Length, unit);
    form.segment_mass = NumberField::shown(device.segment_mass_kg, Quantity::Mass, unit);
    form.end_mass = NumberField::shown(device.end_mass_kg, Quantity::Mass, unit);
    form.damping_linear = NumberField::shown(device.joint_damping_linear, Quantity::Unitless, unit);
    form.damping_angular =
        NumberField::shown(device.joint_damping_angular, Quantity::Unitless, unit);
    form.spawn_orientation = device.spawn_orientation;
    let sphere = plan.capture_sphere.clone().unwrap_or_default();
    form.initial_radius = NumberField::shown(sphere.initial_radius_m, Quantity::Length, unit);
    form.min_radius = NumberField::shown(sphere.min_radius_m, Quantity::Length, unit);
    form.phases = plan
        .phases
        .iter()
        .map(|phase| PhaseForm {
            id: phase.id.clone(),
            max_velocity: NumberField::shown(Some(phase.max_velocity_mps), Quantity::Speed, unit),
            max_force: NumberField::shown(Some(phase.max_force_n), Quantity::Force, unit),
            shrink_rate: NumberField::shown(phase.shrink_rate_mps, Quantity::Speed, unit),
            end: phase.end,
            actuation: phase.actuation,
            tail_actuation: phase.tail_actuation,
            tail_spacing: NumberField::shown(phase.tail_spacing_m, Quantity::Length, unit),
            hold_plane: phase.hold_plane,
            velocity_scales_with_sphere: phase.velocity_scales_with_sphere,
            controller: ControllerKey::ALL.map(|key| {
                let mut controller = phase.controller.unwrap_or_default();
                NumberField::shown(*key.value_mut(&mut controller), key.quantity(), unit)
            }),
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
    let [spacing, segment_mass, end_mass, linear, angular] = [
        (&form.segment_spacing, Quantity::Length),
        (&form.segment_mass, Quantity::Mass),
        (&form.end_mass, Quantity::Mass),
        (&form.damping_linear, Quantity::Unitless),
        (&form.damping_angular, Quantity::Unitless),
    ]
    .into_iter()
    .zip(pointer::DEVICE_NUMBERS)
    .map(|((field, quantity), at)| number(field, quantity, unit, at.to_string(), &mut errors))
    .collect::<Vec<_>>()
    .try_into()
    .expect("five device numbers");
    let device = CapturePlanDevice {
        device_type,
        tether_length_m: length,
        segment_spacing_m: spacing,
        segment_mass_kg: segment_mass,
        end_mass_kg: end_mass,
        joint_damping_linear: linear,
        joint_damping_angular: angular,
        spawn_orientation: form.spawn_orientation,
    };
    // An absent `device` stays absent unless a value was entered (S2).
    if plan.device.is_some() || device != CapturePlanDevice::default() {
        plan.device = Some(device);
    }
    let sphere = CaptureSphere {
        initial_radius_m: number(
            &form.initial_radius,
            Quantity::Length,
            unit,
            pointer::INITIAL_RADIUS.to_string(),
            &mut errors,
        ),
        min_radius_m: number(
            &form.min_radius,
            Quantity::Length,
            unit,
            pointer::MIN_RADIUS.to_string(),
            &mut errors,
        ),
    };
    if plan.capture_sphere.is_some() || sphere != CaptureSphere::default() {
        plan.capture_sphere = Some(sphere);
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
        phase.end = shown.end;
        phase.actuation = shown.actuation;
        phase.tail_actuation = shown.tail_actuation;
        phase.hold_plane = shown.hold_plane;
        phase.velocity_scales_with_sphere = shown.velocity_scales_with_sphere;
        phase.tail_spacing_m = number(
            &shown.tail_spacing,
            Quantity::Length,
            unit,
            pointer::phase(i, "tail_spacing_m"),
            &mut errors,
        );
        let mut controller = PhaseController::default();
        for (field, key) in shown.controller.iter().zip(ControllerKey::ALL) {
            *key.value_mut(&mut controller) = number(
                field,
                key.quantity(),
                unit,
                pointer::phase(i, &format!("controller/{}", key.key())),
                &mut errors,
            );
        }
        // An absent `controller` stays absent unless a gain was entered; a
        // loaded `"controller": {}` is kept.
        phase.controller = if controller == PhaseController::default() {
            phase.controller.filter(|c| *c == controller)
        } else {
            Some(controller)
        };
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
    let mut pointers: Vec<String> = [
        pointer::NAME,
        pointer::TETHER,
        pointer::TETHER_LENGTH,
        pointer::SPAWN_ORIENTATION,
        pointer::INITIAL_RADIUS,
        pointer::MIN_RADIUS,
    ]
    .into_iter()
    .chain(pointer::DEVICE_NUMBERS)
    .map(str::to_string)
    .collect();
    for (i, phase) in form.phases.iter().enumerate() {
        for key in pointer::PHASE_KEYS {
            pointers.push(pointer::phase(i, key));
        }
        for key in ControllerKey::ALL {
            pointers.push(pointer::phase(i, &format!("controller/{}", key.key())));
        }
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
/// to `dir`, the user plans are reloaded and the form closes. An edit saved on
/// the sim screen asks for a restart unless only the name changed (O3 W1.03 (b)).
/// Returns whether a file was written.
pub(crate) fn save_plan_from_form(
    form: &mut NewCapturePlanForm,
    dir: &Path,
    overwrite: bool,
    on_sim_screen: bool,
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
    let restart = editing && on_sim_screen && !only_name_changed;
    log(
        LogLevel::Info,
        format!("Capture plan '{}' saved", form.plan_name),
    );
    form.open = false;
    form.reset();
    form.show_restart_prompt = restart;
    true
}
