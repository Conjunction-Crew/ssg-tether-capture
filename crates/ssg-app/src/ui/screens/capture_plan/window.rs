//! The capture-plan form as an egui modal (P8 S4): the same sections, fields
//! and buttons as the rc.3 bevy_ui modal, with "Phase" for "State" (A-3).
//! Fields accept any text; Save validates (P0-001). The overwrite prompt offers
//! **Overwrite** and **Cancel** (P0-002). The modal closes only through
//! Cancel or Save.

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use crate::ui::events::UiEvent;
use crate::ui::form::{NewCapturePlanForm, TransitionForm, UnitSystem};

const PHASES: [&str; 3] = ["approach", "terminal", "capture"];
const CONDITIONS: [&str; 2] = ["less_than", "greater_than"];
const FORM_WIDTH: f32 = 680.0;

pub fn capture_plan_form_ui(
    ui: &mut egui::Ui,
    form: &mut NewCapturePlanForm,
    out: &mut Vec<UiEvent>,
) {
    title_bar(ui, form, out);
    egui::ScrollArea::vertical()
        .auto_shrink([false, true])
        .show(ui, |ui| {
            egui::Frame::new().inner_margin(20.0).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 20.0;
                body(ui, form, out);
            });
        });
    if let Some(path) = form.overwrite_conflict_path.clone() {
        overwrite_prompt(ui.ctx(), &path, out);
    }
}

fn title_bar(ui: &mut egui::Ui, form: &NewCapturePlanForm, out: &mut Vec<UiEvent>) {
    let title = if form.read_only {
        "View Capture Plan"
    } else if form.editing_plan_id.is_some() {
        "Edit Capture Plan"
    } else {
        "New Capture Plan"
    };
    egui::Frame::new()
        .fill(ui.visuals().widgets.open.bg_fill)
        .inner_margin(20.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(title).size(20.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let cancel = egui::Button::new(egui::RichText::new("× Cancel").weak())
                        .fill(ui.visuals().panel_fill);
                    if ui.add(cancel).clicked() {
                        out.push(UiEvent::CloseNewCapturePlanForm);
                    }
                    if !form.read_only && ui.button("Save").clicked() {
                        out.push(UiEvent::SaveCapturePlan);
                    }
                });
            });
        });
}

fn body(ui: &mut egui::Ui, form: &mut NewCapturePlanForm, out: &mut Vec<UiEvent>) {
    let (vel_unit, force_unit, dist_unit) = match form.unit_system {
        UnitSystem::Metric => ("m/s", "N", "m"),
        UnitSystem::Imperial => ("ft/s", "lbf", "ft"),
    };
    let editable = !form.read_only;

    if !form.validation_errors.is_empty() {
        egui::Frame::new()
            .fill(egui::Color32::from_rgba_unmultiplied(153, 26, 26, 128))
            .inner_margin(12.0)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 4.0;
                ui.label("Please fix the following errors:");
                for error in &form.validation_errors {
                    ui.label(format!("• {error}"));
                }
            });
    }

    section(ui, None, |ui| {
        ui.label(egui::RichText::new("Units").size(11.0).weak());
        ui.horizontal(|ui| {
            for (unit, text) in [
                (UnitSystem::Metric, "m (metric)"),
                (UnitSystem::Imperial, "ft (imperial)"),
            ] {
                if ui.add(choice(text, form.unit_system == unit)).clicked() {
                    out.push(UiEvent::SetUnitSystem(unit));
                }
            }
        });
    });

    section(ui, Some("General"), |ui| {
        field(
            ui,
            "Plan Name *",
            "My Capture Plan",
            &mut form.plan_name,
            editable,
        );
    });

    section(ui, Some("Tether"), |ui| {
        field(
            ui,
            "Tether Name *",
            "Tether1",
            &mut form.tether_name,
            editable,
        );
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            ui.label(egui::RichText::new("Tether Type *").size(11.0).weak());
            ui.horizontal(|ui| {
                if ui
                    .add(choice("Tether", form.tether_type == "tether"))
                    .clicked()
                {
                    form.tether_type = "tether".to_string();
                }
                // The net is not available yet.
                ui.add_enabled(false, egui::Button::new("Net (coming soon)"));
            });
        });
        field(
            ui,
            "Tether Length * (m)",
            "20.0",
            &mut form.tether_length,
            editable,
        );
    });

    section(ui, Some("Approach Phase"), |ui| {
        field(
            ui,
            &format!("Max Velocity * ({vel_unit})"),
            "1.0",
            &mut form.approach_max_velocity,
            editable,
        );
        field(
            ui,
            &format!("Max Force * ({force_unit})"),
            "2.0",
            &mut form.approach_max_force,
            editable,
        );
        transitions(
            ui,
            &mut form.approach_transitions,
            true,
            dist_unit,
            editable,
            out,
        );
    });

    section(ui, Some("Terminal Phase"), |ui| {
        field(
            ui,
            &format!("Max Velocity * ({vel_unit})"),
            "0.2",
            &mut form.terminal_max_velocity,
            editable,
        );
        field(
            ui,
            &format!("Max Force * ({force_unit})"),
            "2.0",
            &mut form.terminal_max_force,
            editable,
        );
        field(
            ui,
            &format!("Shrink Rate * ({vel_unit})"),
            "0.125",
            &mut form.terminal_shrink_rate,
            editable,
        );
        transitions(
            ui,
            &mut form.terminal_transitions,
            false,
            dist_unit,
            editable,
            out,
        );
    });

    section(ui, Some("Capture Phase"), |ui| {
        field(
            ui,
            &format!("Max Velocity * ({vel_unit})"),
            "0.1",
            &mut form.capture_max_velocity,
            editable,
        );
        field(
            ui,
            &format!("Max Force * ({force_unit})"),
            "2.0",
            &mut form.capture_max_force,
            editable,
        );
        field(
            ui,
            &format!("Shrink Rate * ({vel_unit})"),
            "0.025",
            &mut form.capture_shrink_rate,
            editable,
        );
    });
}

/// A form section: an accent title over a rule, then its rows.
fn section(ui: &mut egui::Ui, title: Option<&str>, add_rows: impl FnOnce(&mut egui::Ui)) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 10.0;
        if let Some(title) = title {
            ui.label(
                egui::RichText::new(title)
                    .size(15.0)
                    .color(ui.visuals().hyperlink_color),
            );
            ui.separator();
        }
        add_rows(ui);
    });
}

/// A selectable button: the button fill when chosen, the soft fill otherwise.
fn choice(text: &str, selected: bool) -> egui::Button<'_> {
    if selected {
        egui::Button::new(text)
    } else {
        egui::Button::new(egui::RichText::new(text).weak()).fill(egui::Color32::TRANSPARENT)
    }
}

/// A labelled single-line text field. Any text is accepted; Save validates.
fn field(ui: &mut egui::Ui, label: &str, hint: &str, value: &mut String, editable: bool) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 4.0;
        let label = ui.label(egui::RichText::new(label).size(11.0).weak());
        ui.add(
            egui::TextEdit::singleline(value)
                .hint_text(egui::RichText::new(hint).color(ui.visuals().weak_text_color()))
                .interactive(editable)
                .desired_width(f32::INFINITY)
                .margin(egui::vec2(10.0, 8.0)),
        )
        .labelled_by(label.id);
    });
}

fn transitions(
    ui: &mut egui::Ui,
    transitions: &mut [TransitionForm],
    is_approach: bool,
    dist_unit: &str,
    editable: bool,
    out: &mut Vec<UiEvent>,
) {
    ui.label(egui::RichText::new("Transitions").size(11.0).weak());
    let id = if is_approach { "approach" } else { "terminal" };
    for (i, transition) in transitions.iter_mut().enumerate() {
        egui::Frame::new()
            .fill(egui::Color32::from_rgba_unmultiplied(18, 26, 44, 102))
            .inner_margin(10.0)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 8.0;
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(format!("Transition {}", i + 1)).size(12.0));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let remove = egui::Button::new("×")
                            .fill(egui::Color32::from_rgba_unmultiplied(153, 38, 38, 128));
                        if ui.add(remove).clicked() {
                            out.push(if is_approach {
                                UiEvent::RemoveApproachTransition(i)
                            } else {
                                UiEvent::RemoveTerminalTransition(i)
                            });
                        }
                    });
                });
                dropdown(ui, (id, i, "to"), "To Phase", &mut transition.to, &PHASES);
                dropdown(
                    ui,
                    (id, i, "condition"),
                    "Condition",
                    &mut transition.distance_kind,
                    &CONDITIONS,
                );
                field(
                    ui,
                    &format!("Distance ({dist_unit})"),
                    "50.0",
                    &mut transition.distance_value,
                    editable,
                );
            });
    }
    let add = egui::Button::new(egui::RichText::new("+ Add Transition").weak())
        .fill(ui.visuals().faint_bg_color);
    if ui.add(add).clicked() {
        out.push(if is_approach {
            UiEvent::AddApproachTransition
        } else {
            UiEvent::AddTerminalTransition
        });
    }
}

/// A labelled combo box; "—" until a value is chosen.
fn dropdown(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    label: &str,
    value: &mut String,
    options: &[&str],
) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        ui.label(egui::RichText::new(label).size(11.0).weak());
        // rc.3 drew the closed dropdown on the soft panel fill.
        let soft = ui.visuals().faint_bg_color;
        ui.visuals_mut().widgets.inactive.weak_bg_fill = soft;
        let shown = if value.is_empty() {
            "—"
        } else {
            value.as_str()
        };
        egui::ComboBox::from_id_salt(id)
            .selected_text(shown.to_string())
            .width(ui.available_width())
            .show_ui(ui, |ui| {
                for option in options {
                    ui.selectable_value(value, option.to_string(), *option);
                }
            });
    });
}

fn overwrite_prompt(ctx: &egui::Context, path: &str, out: &mut Vec<UiEvent>) {
    egui::Modal::new(egui::Id::new("capture_plan_overwrite")).show(ctx, |ui| {
        ui.set_width(460.0);
        ui.spacing_mut().item_spacing.y = 14.0;
        ui.label(egui::RichText::new("Overwrite Existing Plan?").size(18.0));
        ui.label(egui::RichText::new("A plan already exists at:").weak());
        ui.label(egui::RichText::new(path).color(ui.visuals().hyperlink_color));
        ui.horizontal(|ui| {
            let cancel = egui::Button::new(egui::RichText::new("Cancel").weak())
                .fill(ui.visuals().faint_bg_color);
            if ui.add(cancel).clicked() {
                out.push(UiEvent::CancelOverwriteCapturePlan);
            }
            let overwrite =
                egui::Button::new("Overwrite").fill(egui::Color32::from_rgb(179, 51, 51));
            if ui.add(overwrite).clicked() {
                out.push(UiEvent::ConfirmOverwriteCapturePlan);
            }
        });
    });
}

/// Shows the form as a modal over Home or the sim while it is open.
pub fn capture_plan_form_window(
    mut contexts: EguiContexts,
    mut form: ResMut<NewCapturePlanForm>,
    mut events: MessageWriter<UiEvent>,
) {
    if !form.open {
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    let max_height = ctx.content_rect().height() * 0.88;
    let width = FORM_WIDTH.min(ctx.content_rect().width() * 0.94);
    let mut out = Vec::new();
    egui::Modal::new(egui::Id::new("capture_plan_form"))
        .frame(egui::Frame::new().fill(ctx.style().visuals.panel_fill))
        .show(ctx, |ui| {
            ui.set_width(width);
            ui.set_max_height(max_height);
            capture_plan_form_ui(ui, &mut form, &mut out);
        });
    events.write_batch(out);
}
