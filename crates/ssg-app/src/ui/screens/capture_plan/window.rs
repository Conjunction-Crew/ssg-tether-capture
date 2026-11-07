//! The capture-plan form as an egui modal (P8 S4; P9 S1 on the v1 plan): the
//! General and Tether sections, then one section per phase of the plan
//! (`phases.rs`). Fields accept any text; Save validates (P0-001) and each
//! message is shown under the field it names. The overwrite prompt offers
//! **Overwrite** and **Cancel** (P0-002). The modal closes only through Cancel
//! or Save.

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use super::{field_pointers, phases, pointer};
use crate::ui::events::UiEvent;
use crate::ui::form::{FormError, NewCapturePlanForm, NumberField, Quantity, UnitSystem};

const FORM_WIDTH: f32 = 680.0;

/// What every field of the form needs: whether it is editable, the units shown
/// and Save's messages.
pub(super) struct FieldCtx<'a> {
    pub editable: bool,
    pub unit: UnitSystem,
    pub errors: &'a [FormError],
}

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
    let errors = form.validation_errors.clone();
    let ctx = FieldCtx {
        editable: !form.read_only,
        unit: form.unit_system,
        errors: &errors,
    };

    if !errors.is_empty() {
        let on_screen = field_pointers(form);
        let listed: Vec<&FormError> = errors
            .iter()
            .filter(|error| !on_screen.contains(&error.pointer))
            .collect();
        egui::Frame::new()
            .fill(egui::Color32::from_rgba_unmultiplied(153, 26, 26, 128))
            .inner_margin(12.0)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 4.0;
                if listed.is_empty() {
                    ui.label("Please fix the fields marked below.");
                } else {
                    ui.label("Please fix the following errors:");
                    for error in listed {
                        ui.label(format!("• {}: {}", error.pointer, error.message));
                    }
                }
            });
    }

    section(ui, None, |ui| units_toggle(ui, form.unit_system, out));

    section(ui, Some("General"), |ui| {
        field(
            ui,
            &ctx,
            "Plan Name *",
            "My Capture Plan",
            &mut form.plan_name,
            pointer::NAME,
        );
    });

    section(ui, Some("Tether"), |ui| {
        field(
            ui,
            &ctx,
            "Tether Name *",
            "Tether1",
            &mut form.tether_name,
            pointer::TETHER,
        );
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            ui.label(egui::RichText::new("Tether Type").size(11.0).weak());
            ui.horizontal(|ui| {
                // Absent is the default, `tether`.
                let is_tether = matches!(form.tether_type.as_str(), "" | "tether");
                let button = ui.add_enabled(ctx.editable, choice("Tether", is_tether));
                if button.clicked() && !is_tether {
                    form.tether_type = "tether".to_string();
                }
                // The net is not available yet.
                ui.add_enabled(false, egui::Button::new("Net (coming soon)"));
            });
        });
        number_field(
            ui,
            &ctx,
            &format!("Tether Length ({})", Quantity::Length.unit_label(ctx.unit)),
            "20.0",
            &mut form.tether_length,
            pointer::TETHER_LENGTH,
        );
    });

    phases::phase_sections(ui, &ctx, &mut form.phases, out);
}

/// The one Metric/Imperial toggle, here and in the Simulation HUD (OQ3).
pub(crate) fn units_toggle(ui: &mut egui::Ui, current: UnitSystem, out: &mut Vec<UiEvent>) {
    ui.label(egui::RichText::new("Units").size(11.0).weak());
    ui.horizontal(|ui| {
        for (unit, text) in [
            (UnitSystem::Metric, "m (metric)"),
            (UnitSystem::Imperial, "ft (imperial)"),
        ] {
            if ui.add(choice(text, current == unit)).clicked() {
                out.push(UiEvent::SetUnitSystem(unit));
            }
        }
    });
}

/// A form section: an accent title over a rule, then its rows.
pub(super) fn section(
    ui: &mut egui::Ui,
    title: Option<&str>,
    add_rows: impl FnOnce(&mut egui::Ui),
) {
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
pub(super) fn choice(text: &str, selected: bool) -> egui::Button<'_> {
    if selected {
        egui::Button::new(text)
    } else {
        egui::Button::new(egui::RichText::new(text).weak()).fill(egui::Color32::TRANSPARENT)
    }
}

/// A labelled single-line text field and Save's messages about `pointer`.
/// Any text is accepted; Save validates.
pub(super) fn field(
    ui: &mut egui::Ui,
    ctx: &FieldCtx,
    label: &str,
    hint: &str,
    value: &mut String,
    pointer: &str,
) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 4.0;
        let label = ui.label(egui::RichText::new(label).size(11.0).weak());
        ui.add(
            egui::TextEdit::singleline(value)
                .hint_text(egui::RichText::new(hint).color(ui.visuals().weak_text_color()))
                .interactive(ctx.editable)
                .desired_width(f32::INFINITY)
                .margin(egui::vec2(10.0, 8.0)),
        )
        .labelled_by(label.id);
        errors_at(ui, ctx, pointer);
    });
}

/// A number field: [`field`] on its text.
pub(super) fn number_field(
    ui: &mut egui::Ui,
    ctx: &FieldCtx,
    label: &str,
    hint: &str,
    value: &mut NumberField,
    pointer: &str,
) {
    field(ui, ctx, label, hint, &mut value.text, pointer);
}

/// Save's messages about `pointer`, in the error colour.
pub(super) fn errors_at(ui: &mut egui::Ui, ctx: &FieldCtx, pointer: &str) {
    for error in ctx.errors.iter().filter(|error| error.pointer == pointer) {
        ui.label(
            egui::RichText::new(&error.message)
                .size(11.0)
                .color(ui.visuals().error_fg_color),
        );
    }
}

/// A labelled combo box; "—" until a value is chosen.
pub(super) fn dropdown(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    label: &str,
    value: &mut String,
    options: &[String],
    enabled: bool,
) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        ui.label(egui::RichText::new(label).size(11.0).weak());
        // rc.3 drew the closed dropdown on the soft panel fill.
        let soft = ui.visuals().faint_bg_color;
        ui.visuals_mut().widgets.inactive.weak_bg_fill = soft;
        let shown = if value.is_empty() {
            "—".to_string()
        } else {
            value.clone()
        };
        ui.add_enabled_ui(enabled, |ui| {
            egui::ComboBox::from_id_salt(id)
                .selected_text(shown)
                .width(ui.available_width())
                .show_ui(ui, |ui| {
                    for option in options {
                        ui.selectable_value(value, option.clone(), option);
                    }
                });
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

/// The panel colour without its alpha: nothing shows through the form (O3 S2.03).
pub(crate) fn form_fill(visuals: &egui::Visuals) -> egui::Color32 {
    visuals.panel_fill.to_opaque()
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
        .frame(egui::Frame::new().fill(form_fill(&ctx.style().visuals)))
        .show(ctx, |ui| {
            ui.set_width(width);
            ui.set_max_height(max_height);
            capture_plan_form_ui(ui, &mut form, &mut out);
        });
    events.write_batch(out);
}
