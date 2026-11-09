//! The working-directory setup screen on egui (P8 S3). The path is a typed
//! text field that **Browse** also fills; a directory that cannot be created
//! keeps the screen open and shows the error under the field (F-33), and so does a
//! config file that cannot be written, until Select is pressed again (#176).

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use crate::ui::events::UiEvent;
use ssg_sim::project::WorkingDirectory;

/// The last failure to use the selected directory, shown under the path.
#[derive(Resource, Default)]
pub struct WorkingDirectorySetupError(pub Option<String>);

/// What the setup screen shows and edits.
pub struct WorkingDirectorySetupView {
    pub path: String,
    pub error: Option<String>,
}

pub fn working_directory_setup_ui(
    ui: &mut egui::Ui,
    view: &mut WorkingDirectorySetupView,
    out: &mut Vec<UiEvent>,
) {
    ui.add_space((ui.available_height() / 3.0 - 100.0).max(0.0));
    ui.vertical_centered(|ui| {
        egui::Frame::new()
            .fill(ui.visuals().panel_fill)
            .inner_margin(28.0)
            .show(ui, |ui| {
                ui.set_width(ui.available_width().min(560.0));
                ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.spacing_mut().item_spacing.y = 16.0;
                    ui.label(egui::RichText::new("Select Working Directory").size(22.0));
                    ui.weak(
                        egui::RichText::new("Choose where your projects will be stored.")
                            .size(13.0),
                    );
                    let path = ui.add(
                        egui::TextEdit::singleline(&mut view.path)
                            .desired_width(f32::INFINITY)
                            .text_color(ui.visuals().hyperlink_color)
                            .margin(10.0),
                    );
                    // Enter in the field acts as Select (operator's O3 note).
                    if path.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        out.push(UiEvent::WorkingDirectorySelected(view.path.clone()));
                    }
                    if let Some(error) = &view.error {
                        ui.colored_label(ui.visuals().error_fg_color, error);
                    }
                    let row = egui::vec2(ui.available_width(), ui.spacing().interact_size.y);
                    ui.allocate_ui_with_layout(
                        row,
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            if ui.button("Select").clicked() {
                                out.push(UiEvent::WorkingDirectorySelected(view.path.clone()));
                            }
                            if ui
                                .add(egui::Button::new("Browse").fill(ui.visuals().faint_bg_color))
                                .clicked()
                            {
                                out.push(UiEvent::BrowseForWorkingDirectory);
                            }
                        },
                    );
                });
            });
    });
}

pub fn clear_working_directory_setup_error(mut error: ResMut<WorkingDirectorySetupError>) {
    error.0 = None;
}

pub fn working_directory_setup_screen(
    mut contexts: EguiContexts,
    mut working_directory: ResMut<WorkingDirectory>,
    error: Res<WorkingDirectorySetupError>,
    mut events: MessageWriter<UiEvent>,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    let mut view = WorkingDirectorySetupView {
        path: working_directory.pending_path.clone(),
        error: error.0.clone(),
    };
    let mut out = Vec::new();
    egui::CentralPanel::default()
        .frame(egui::Frame::central_panel(&ctx.style()).fill(ctx.style().visuals.extreme_bg_color))
        .show(ctx, |ui| {
            working_directory_setup_ui(ui, &mut view, &mut out)
        });
    if view.path != working_directory.pending_path {
        working_directory.pending_path = view.path;
    }
    events.write_batch(out);
}
