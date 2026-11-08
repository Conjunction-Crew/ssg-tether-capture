//! One form section per phase of the plan, in file order (P9 S1, F-15): its
//! limits and its transitions, each with every condition it has; all of a
//! transition's conditions must hold (Q10 row 4). Phase ids are not editable and
//! phases are not added, removed or reordered here (A-1, E1).

use bevy_egui::egui;

use super::pointer;
use super::window::{FieldCtx, dropdown, errors_at, number_field, section};
use crate::ui::events::UiEvent;
use crate::ui::form::{PhaseForm, Quantity, TransitionForm};

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
            transitions(ui, ctx, i, &mut phase.transitions, &ids, out);
        });
    }
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
                    let unit = condition.kind.quantity().unit_label(ctx.unit);
                    let at =
                        |rest: &str| pointer::transition(phase, j, &format!("all_of/{key}{rest}"));
                    errors_at(ui, ctx, &at(""));
                    ui.columns(2, |columns| {
                        number_field(
                            &mut columns[0],
                            ctx,
                            &format!("{label} less than ({unit})"),
                            "none",
                            &mut condition.less_than,
                            &at("/less_than"),
                        );
                        number_field(
                            &mut columns[1],
                            ctx,
                            &format!("{label} greater than ({unit})"),
                            "none",
                            &mut condition.greater_than,
                            &at("/greater_than"),
                        );
                    });
                }
            });
    }
    let add = egui::Button::new(egui::RichText::new("+ Add Transition").weak())
        .fill(ui.visuals().faint_bg_color);
    if ui.add_enabled(ctx.editable, add).clicked() {
        out.push(UiEvent::AddTransition(phase));
    }
}
