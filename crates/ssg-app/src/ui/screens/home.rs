//! The Home screen on egui (P8 S3): the header, the Working Directory panel,
//! the user and example plan cards and the load-error banner. Everything below
//! the header scrolls when it does not fit the window (F-37).

use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiTextureHandle, egui};

use crate::ui::events::UiEvent;
use crate::ui::form::{NewCapturePlanForm, SimPlanSyncState};
use crate::ui::log_ui::CapturePlanLoadErrors;
use crate::ui::theme::UiTheme;
use bevy::platform::collections::HashMap;
use ssg_sim::capture::plan::{CapturePlan, CapturePlanLibrary};
use ssg_sim::project::WorkingDirectory;

const CARD_WIDTH: f32 = 340.0;
const CARD_MIN_HEIGHT: f32 = 148.0;
const CARD_PADDING: f32 = 14.0;

/// One plan card: the file stem (the plan id) and the plan's name.
pub struct PlanCard {
    pub id: String,
    pub name: String,
}

/// What the Home screen shows.
pub struct HomeView {
    pub working_directory: String,
    pub user_plans: Vec<PlanCard>,
    pub example_plans: Vec<PlanCard>,
    /// Load errors by file stem.
    pub load_errors: Vec<(String, Vec<String>)>,
    pub logo: Option<egui::TextureId>,
    /// The body's maximum width (`UiTheme::content_max_width`).
    pub content_max_width: f32,
}

fn plan_count_label(count: usize, what: &str) -> String {
    format!("{count} {what}{}", if count == 1 { "" } else { "s" })
}

pub fn home_ui(ui: &mut egui::Ui, view: &mut HomeView, out: &mut Vec<UiEvent>) {
    header(ui, view.logo);
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            ui.add_space(20.0);
            let width = ui.available_width().min(view.content_max_width);
            let margin = (ui.available_width() - width) / 2.0;
            ui.horizontal(|ui| {
                ui.add_space(margin);
                ui.vertical(|ui| {
                    ui.set_width(width);
                    ui.spacing_mut().item_spacing.y = 16.0;
                    body(ui, view, out);
                });
            });
            ui.add_space(20.0);
        });
}

fn header(ui: &mut egui::Ui, logo: Option<egui::TextureId>) {
    egui::Frame::new()
        .fill(ui.visuals().widgets.open.bg_fill)
        .inner_margin(egui::Margin::symmetric(24, 16))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.set_min_height(84.0 - 32.0);
            ui.vertical_centered(|ui| {
                ui.horizontal(|ui| {
                    // Centre the logo and titles as one row.
                    let row_width = 50.0 + 14.0 + 260.0;
                    ui.add_space(((ui.available_width() - row_width) / 2.0).max(0.0));
                    ui.spacing_mut().item_spacing.x = 14.0;
                    if let Some(logo) = logo {
                        ui.add(egui::Image::new((logo, egui::vec2(50.0, 50.0))));
                    }
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        ui.label(egui::RichText::new("Tether Capture").size(30.0));
                        ui.weak(egui::RichText::new("Conjunction Crew").size(14.0));
                    });
                });
            });
        });
}

fn body(ui: &mut egui::Ui, view: &HomeView, out: &mut Vec<UiEvent>) {
    egui::Frame::new()
        .fill(ui.visuals().faint_bg_color)
        .inner_margin(16.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 8.0;
            ui.label(egui::RichText::new("Working Directory").size(17.0));
            ui.weak(
                egui::RichText::new("Set the location where your projects are stored").size(12.0),
            );
            ui.label(egui::RichText::new(&view.working_directory).size(13.0));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add(muted_button(
                        "Change Directory",
                        12.0,
                        ui.visuals().panel_fill,
                    ))
                    .clicked()
                {
                    out.push(UiEvent::ChangeWorkingDirectory);
                }
            });
        });

    // A warning banner for any capture plan files that failed to load.
    if !view.load_errors.is_empty() {
        egui::Frame::new()
            .fill(egui::Color32::from_rgba_unmultiplied(140, 38, 38, 217))
            .inner_margin(12.0)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 4.0;
                ui.colored_label(
                    egui::Color32::from_rgb(255, 217, 217),
                    egui::RichText::new("Warning: Some capture plan files could not be loaded")
                        .size(13.0),
                );
                for (file_stem, file_errors) in &view.load_errors {
                    for error in file_errors {
                        ui.colored_label(
                            egui::Color32::from_rgb(255, 191, 191),
                            egui::RichText::new(format!("  \u{2022} {file_stem}: {error}"))
                                .size(11.0),
                        );
                    }
                }
            });
    }

    ui.add_space(10.0);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("My Capture Plans").size(24.0));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("+ New Plan").clicked() {
                out.push(UiEvent::OpenNewCapturePlanForm);
            }
            ui.weak(
                egui::RichText::new(
                    plan_count_label(view.user_plans.len(), "plan") + " in working directory",
                )
                .size(12.0),
            );
        });
    });
    plan_cards(ui, &view.user_plans, true, out);

    ui.add_space(10.0);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Example Capture Plans").size(24.0));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.weak(
                egui::RichText::new(plan_count_label(view.example_plans.len(), "example plan"))
                    .size(12.0),
            );
        });
    });
    plan_cards(ui, &view.example_plans, false, out);
}

/// rc.3's secondary buttons: muted text on a panel fill.
fn muted_button(text: &str, size: f32, fill: egui::Color32) -> egui::Button<'_> {
    egui::Button::new(egui::RichText::new(text).size(size).weak()).fill(fill)
}

fn plan_cards(ui: &mut egui::Ui, cards: &[PlanCard], editable: bool, out: &mut Vec<UiEvent>) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(12.0, 12.0);
        for card in cards {
            plan_card(ui, card, editable, out);
        }
    });
}

/// A clickable card (opens the plan) with the name, an **edit** button for
/// user plans, and the file stem at the bottom.
fn plan_card(ui: &mut egui::Ui, card: &PlanCard, editable: bool, out: &mut Vec<UiEvent>) {
    // A fixed size, so the wrapping layout moves a card that does not fit to
    // the next row instead of squeezing it.
    let size = egui::vec2(CARD_WIDTH.min(ui.max_rect().width()), CARD_MIN_HEIGHT);
    let width = size.x - 2.0 * CARD_PADDING;
    let response = ui
        .allocate_ui_with_layout(size, egui::Layout::top_down(egui::Align::Min), |ui| {
            ui.scope_builder(
                egui::UiBuilder::new()
                    .id_salt(&card.id)
                    .sense(egui::Sense::click()),
                |ui| {
                    let fill = if ui.response().hovered() {
                        ui.visuals().faint_bg_color
                    } else {
                        ui.visuals().panel_fill
                    };
                    egui::Frame::new()
                        .fill(fill)
                        .inner_margin(CARD_PADDING)
                        .show(ui, |ui| {
                            ui.style_mut().interaction.selectable_labels = false;
                            ui.set_width(width);
                            ui.set_min_height(CARD_MIN_HEIGHT - 2.0 * CARD_PADDING);
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new(&card.name).size(18.0));
                                if editable {
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if ui
                                                .add(muted_button(
                                                    "edit",
                                                    11.0,
                                                    ui.visuals().faint_bg_color,
                                                ))
                                                .clicked()
                                            {
                                                out.push(UiEvent::EditCapturePlan(card.id.clone()));
                                            }
                                        },
                                    );
                                }
                            });
                            // The stem sits at the bottom of the card.
                            ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                                ui.weak(egui::RichText::new(&card.id).size(12.0));
                            });
                        });
                },
            )
            .response
        })
        .inner;
    if response.clicked() {
        out.push(UiEvent::OpenProject(card.id.clone()));
    }
}

/// Loads the example and user plans when Home opens.
pub fn load_home_plans(
    mut capture_plan_lib: ResMut<CapturePlanLibrary>,
    mut capture_plan_load_errors: ResMut<CapturePlanLoadErrors>,
    working_directory: Res<WorkingDirectory>,
) {
    capture_plan_load_errors.errors = capture_plan_lib.load_all(
        &crate::resolve_assets_dir().join("example_capture_plans"),
        std::path::Path::new(&working_directory.path),
    );
}

fn cards(plans: &HashMap<String, CapturePlan>) -> Vec<PlanCard> {
    plans
        .iter()
        .map(|(id, plan)| PlanCard {
            id: id.clone(),
            name: plan.name.clone(),
        })
        .collect()
}

pub fn home_screen(
    mut contexts: EguiContexts,
    asset_server: Res<AssetServer>,
    theme: Res<UiTheme>,
    capture_plan_lib: Res<CapturePlanLibrary>,
    capture_plan_load_errors: Res<CapturePlanLoadErrors>,
    working_directory: Res<WorkingDirectory>,
    sync_state: Res<SimPlanSyncState>,
    form: Res<NewCapturePlanForm>,
    mut logo: Local<Option<egui::TextureId>>,
    mut events: MessageWriter<UiEvent>,
) {
    // When restarting the sim we pass through Home only to trigger cleanup
    // and reload; nothing is drawn so the home screen never flashes. While the
    // bevy_ui plan form is open it covers Home, so Home is not drawn over it.
    if sync_state.restart_requested || form.open {
        return;
    }
    let logo = *logo.get_or_insert_with(|| {
        contexts.add_image(EguiTextureHandle::Strong(
            asset_server.load("logo/tether-capture.iconset/icon_128x128.png"),
        ))
    });
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    let mut view = HomeView {
        working_directory: working_directory.path.clone(),
        user_plans: cards(&capture_plan_lib.user_plans),
        example_plans: cards(&capture_plan_lib.example_plans),
        load_errors: capture_plan_load_errors
            .errors
            .iter()
            .map(|(stem, errors)| (stem.clone(), errors.clone()))
            .collect(),
        logo: Some(logo),
        content_max_width: match theme.content_max_width {
            Val::Px(width) => width,
            _ => f32::INFINITY,
        },
    };
    let mut out = Vec::new();
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(ctx.style().visuals.extreme_bg_color))
        .show(ctx, |ui| home_ui(ui, &mut view, &mut out));
    events.write_batch(out);
}
