//! One form section per phase of the plan, in file order (P9 S1, F-15): its
//! limits and its transitions, each with every condition it has; all of a
//! transition's conditions must hold (Q10 row 4). Phase ids are not editable and
//! phases are not added, removed or reordered here (A-1, E1). S2 adds every
//! other phase key, the controller gains and the conditions of S3.

use bevy_egui::egui;

use super::pointer;
use super::window::{
    FieldCtx, choice_row, default_number, dropdown, errors_at, number_field, section,
};
use crate::ui::events::UiEvent;
use crate::ui::form::{
    ConditionForm, ConditionKind, ControllerKey, NumberField, PhaseForm, Quantity, TransitionForm,
};
use ssg_sim::capture::plan::{Actuation, PhaseEnd};

/// A switch of the plan: absent (its default, off), off or on.
const SWITCH: [(Option<bool>, &str); 3] = [
    (None, "Default (off)"),
    (Some(false), "Off"),
    (Some(true), "On"),
];

pub(super) fn phase_sections(
    ui: &mut egui::Ui,
    ctx: &FieldCtx,
    phases: &mut [PhaseForm],
    out: &mut Vec<UiEvent>,
) {
    let ids: Vec<String> = phases.iter().map(|phase| phase.id.clone()).collect();
    let speed = Quantity::Speed.unit_label(ctx.unit);
    let force = Quantity::Force.unit_label(ctx.unit);
    for (i, phase) in phases.iter_mut().enumerate() {
        section(ui, Some(&format!("Phase · {}", phase.id)), |ui| {
            number_field(
                ui,
                ctx,
                &format!("Max Velocity * ({speed})"),
                "1.0",
                &mut phase.max_velocity,
                &pointer::phase(i, "max_velocity_mps"),
            );
            number_field(
                ui,
                ctx,
                &format!("Max Force * ({force})"),
                "2.0",
                &mut phase.max_force,
                &pointer::phase(i, "max_force_n"),
            );
            number_field(
                ui,
                ctx,
                &format!("Shrink Rate ({speed})"),
                "none",
                &mut phase.shrink_rate,
                &pointer::phase(i, "shrink_rate_mps"),
            );
            phase_keys(ui, ctx, i, phase);
            transitions(ui, ctx, i, &mut phase.transitions, &ids, out);
        });
    }
}

/// S2: the phase keys besides its limits (Q10 row 7, the P7.5 keys) and its
/// controller gains (Q10 row 17), collapsed.
fn phase_keys(ui: &mut egui::Ui, ctx: &FieldCtx, i: usize, phase: &mut PhaseForm) {
    choice_row(
        ui,
        ctx,
        "End",
        &mut phase.end,
        &[
            (None, "None"),
            (Some(PhaseEnd::Success), "Success"),
            (Some(PhaseEnd::Abort), "Abort"),
        ],
        &pointer::phase(i, "end"),
    );
    choice_row(
        ui,
        ctx,
        "Actuation",
        &mut phase.actuation,
        &[
            (None, "Default (circle)"),
            (Some(Actuation::Circle), "Circle"),
            (Some(Actuation::Hold), "Hold"),
        ],
        &pointer::phase(i, "actuation"),
    );
    choice_row(
        ui,
        ctx,
        "Tail Actuation",
        &mut phase.tail_actuation,
        &SWITCH,
        &pointer::phase(i, "tail_actuation"),
    );
    number_field(
        ui,
        ctx,
        &Quantity::Length.labelled("Tail Spacing", ctx.unit),
        "none",
        &mut phase.tail_spacing,
        &pointer::phase(i, "tail_spacing_m"),
    );
    choice_row(
        ui,
        ctx,
        "Hold Plane",
        &mut phase.hold_plane,
        &SWITCH,
        &pointer::phase(i, "hold_plane"),
    );
    choice_row(
        ui,
        ctx,
        "Speed Scales with Sphere",
        &mut phase.velocity_scales_with_sphere,
        &SWITCH,
        &pointer::phase(i, "velocity_scales_with_sphere"),
    );
    egui::CollapsingHeader::new(egui::RichText::new("Controller").size(12.0))
        .id_salt(("phase_controller", i))
        .default_open(false)
        .show(ui, |ui| {
            for (field, key) in phase.controller.iter_mut().zip(ControllerKey::ALL) {
                default_number(
                    ui,
                    ctx,
                    key.label(),
                    key.quantity(),
                    key.default_value(),
                    field,
                    &pointer::phase(i, &format!("controller/{}", key.key())),
                );
            }
        });
}

fn transitions(
    ui: &mut egui::Ui,
    ctx: &FieldCtx,
    phase: usize,
    transitions: &mut [TransitionForm],
    phase_ids: &[String],
    out: &mut Vec<UiEvent>,
) {
    ui.label(egui::RichText::new("Transitions").size(11.0).weak());
    errors_at(ui, ctx, &pointer::phase(phase, "transitions"));
    for (j, transition) in transitions.iter_mut().enumerate() {
        egui::Frame::new()
            .fill(egui::Color32::from_rgba_unmultiplied(18, 26, 44, 102))
            .inner_margin(10.0)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 8.0;
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(format!("Transition {}", j + 1)).size(12.0));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let remove = egui::Button::new("×")
                            .fill(egui::Color32::from_rgba_unmultiplied(153, 38, 38, 128));
                        if ui.add_enabled(ctx.editable, remove).clicked() {
                            out.push(UiEvent::RemoveTransition { phase, index: j });
                        }
                    });
                });
                dropdown(
                    ui,
                    (phase, j, "to"),
                    "To Phase",
                    &mut transition.to,
                    phase_ids,
                    ctx.editable,
                );
                errors_at(ui, ctx, &pointer::transition(phase, j, "to"));
                ui.label(
                    egui::RichText::new("Conditions (all must hold)")
                        .size(11.0)
                        .weak(),
                );
                errors_at(ui, ctx, &pointer::transition(phase, j, "all_of"));
                for condition in &mut transition.conditions {
                    let key = condition.kind.key();
                    let label = condition.kind.label();
                    let quantity = condition.kind.quantity();
                    let at =
                        |rest: &str| pointer::transition(phase, j, &format!("all_of/{key}{rest}"));
                    errors_at(ui, ctx, &at(""));
                    ui.columns(2, |columns| {
                        number_field(
                            &mut columns[0],
                            ctx,
                            &quantity.labelled(&format!("{label} less than"), ctx.unit),
                            "none",
                            &mut condition.less_than,
                            &at("/less_than"),
                        );
                        number_field(
                            &mut columns[1],
                            ctx,
                            &quantity.labelled(&format!("{label} greater than"), ctx.unit),
                            "none",
                            &mut condition.greater_than,
                            &at("/greater_than"),
                        );
                    });
                }
                add_condition(ui, ctx, (phase, j), &mut transition.conditions);
            });
    }
    let add = egui::Button::new(egui::RichText::new("+ Add Transition").weak())
        .fill(ui.visuals().faint_bg_color);
    if ui.add_enabled(ctx.editable, add).clicked() {
        out.push(UiEvent::AddTransition(phase));
    }
}

/// "+ Condition": adds one of the conditions the transition does not have yet;
/// it stays absent until a bound is typed (S2).
fn add_condition(
    ui: &mut egui::Ui,
    ctx: &FieldCtx,
    id: (usize, usize),
    conditions: &mut Vec<ConditionForm>,
) {
    let missing: Vec<ConditionKind> = ConditionKind::ALL
        .into_iter()
        .filter(|kind| conditions.iter().all(|c| c.kind != *kind))
        .collect();
    if missing.is_empty() {
        return;
    }
    ui.add_enabled_ui(ctx.editable, |ui| {
        ui.push_id(("add_condition", id), |ui| {
            ui.menu_button("+ Condition", |ui| {
                for kind in missing {
                    if ui.button(kind.label()).clicked() {
                        conditions.push(ConditionForm {
                            kind,
                            less_than: NumberField::default(),
                            greater_than: NumberField::default(),
                        });
                        ui.close();
                    }
                }
            });
        });
    });
}
