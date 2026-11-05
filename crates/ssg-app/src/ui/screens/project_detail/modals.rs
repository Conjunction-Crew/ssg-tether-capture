//! The exit-confirm and restart-prompt modals on egui (P8 S5): they close only
//! through their buttons, and the modal area blocks everything behind them.

use bevy_egui::egui;

use crate::ui::events::UiEvent;

/// The modals' width (rc.3).
const MODAL_WIDTH: f32 = 420.0;

fn modal_body(ui: &mut egui::Ui, title: &str, message: &str) {
    ui.set_width(MODAL_WIDTH);
    ui.spacing_mut().item_spacing.y = 14.0;
    ui.label(egui::RichText::new(title).size(20.0));
    ui.weak(egui::RichText::new(message).size(13.0));
}

/// A right-aligned row of buttons that does not stretch the modal.
fn button_row(ui: &mut egui::Ui, add_buttons: impl FnOnce(&mut egui::Ui)) {
    let row = egui::vec2(ui.available_width(), ui.spacing().interact_size.y);
    ui.allocate_ui_with_layout(
        row,
        egui::Layout::right_to_left(egui::Align::Center),
        add_buttons,
    );
}

fn soft_button(ui: &mut egui::Ui, text: egui::RichText) -> egui::Response {
    ui.add(egui::Button::new(text).fill(ui.visuals().faint_bg_color))
}

pub fn exit_confirm_ui(ui: &mut egui::Ui, _view: &mut (), out: &mut Vec<UiEvent>) {
    modal_body(
        ui,
        "Exit Simulation?",
        "Your orbital simulation will be reset. Are you sure you want to exit?",
    );
    button_row(ui, |ui| {
        if ui.button("Exit Sim").clicked() {
            out.push(UiEvent::BackToHome);
        }
        if soft_button(ui, egui::RichText::new("Cancel")).clicked() {
            out.push(UiEvent::CancelExitConfirm);
        }
    });
}

pub fn restart_prompt_ui(ui: &mut egui::Ui, _view: &mut (), out: &mut Vec<UiEvent>) {
    modal_body(
        ui,
        "Plan Changed",
        "The capture plan has been updated. Restart the simulation to apply changes?",
    );
    button_row(ui, |ui| {
        if ui.button("Restart Sim").clicked() {
            out.push(UiEvent::RestartSim);
        }
        let accent = ui.visuals().hyperlink_color;
        if soft_button(ui, egui::RichText::new("Apply Live").color(accent)).clicked() {
            out.push(UiEvent::ApplyPlanLive);
        }
        if soft_button(ui, egui::RichText::new("Continue")).clicked() {
            out.push(UiEvent::DismissRestartPrompt);
        }
    });
}
