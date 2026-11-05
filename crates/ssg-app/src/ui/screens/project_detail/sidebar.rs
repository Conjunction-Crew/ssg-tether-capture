//! The sim screen's header and sidebar on egui (P8 S5): pure functions of
//! [`SimScreenView`], driven by the kittest tests, and the thin systems that
//! show them. Sections: Project Information, Simulation Controls, Orbital
//! Selection (before Start), Time Warp (after Start), Simulation HUD, Reference.

use bevy::ecs::system::SystemParam;
use bevy_egui::{EguiContexts, egui};

use super::modals::{exit_confirm_ui, restart_prompt_ui};
use super::orbital_selection::orbital_selection_section;
use super::*;
use crate::ui::hud::HudReadouts;
use crate::ui::screens::capture_plan::window::form_fill;

/// The sidebar's width and its cap as a share of the window (rc.3).
const SIDEBAR_WIDTH: f32 = 420.0;
const SIDEBAR_MAX_SHARE: f32 = 0.42;
/// The sync indicator's yellow (rc.3).
const OUT_OF_SYNC_COLOR: egui::Color32 = egui::Color32::from_rgb(255, 217, 0);

/// What the header and the sidebar show.
#[derive(Debug, Clone, Default)]
pub struct SimScreenView {
    pub title: String,
    pub working_directory: String,
    pub main_file: String,
    pub plan_id: Option<String>,
    pub is_example_plan: bool,
    pub in_sync: bool,
    /// `SimState::Running`; otherwise the setup stage.
    pub running: bool,
    pub can_start: bool,
    pub capture_active: bool,
    pub capture_entity: Option<Entity>,
    pub show_satellite_indicator: bool,
    pub time_warp: String,
    pub telemetry: String,
    pub guidance: String,
}

fn full_width_button(ui: &mut egui::Ui, enabled: bool, button: egui::Button) -> egui::Response {
    let size = egui::vec2(ui.available_width(), 40.0);
    ui.add_enabled_ui(enabled, |ui| ui.add_sized(size, button))
        .inner
}

fn soft_button<'a>(ui: &egui::Ui, text: impl Into<egui::WidgetText>) -> egui::Button<'a> {
    egui::Button::new(text).fill(ui.visuals().faint_bg_color)
}

fn section(ui: &mut egui::Ui, title: &str, add_contents: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(ui.visuals().panel_fill)
        .inner_margin(12.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            egui::CollapsingHeader::new(egui::RichText::new(title).size(17.0))
                .default_open(true)
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 8.0;
                    add_contents(ui);
                });
        });
}

/// **Back**, the plan's name, "Project View".
pub fn sim_header_ui(ui: &mut egui::Ui, view: &mut SimScreenView, out: &mut Vec<UiEvent>) {
    ui.horizontal_centered(|ui| {
        let back = soft_button(ui, "Back");
        if ui.add_sized([120.0, 40.0], back).clicked() {
            out.push(UiEvent::ShowExitConfirm);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.weak(egui::RichText::new("Project View").size(12.0));
            ui.centered_and_justified(|ui| {
                ui.label(egui::RichText::new(&view.title).size(24.0));
            });
        });
    });
}

fn project_information(ui: &mut egui::Ui, view: &SimScreenView, out: &mut Vec<UiEvent>) {
    if !view.in_sync {
        ui.horizontal_wrapped(|ui| {
            ui.colored_label(
                OUT_OF_SYNC_COLOR,
                egui::RichText::new("Plan changed — sim out of sync").size(11.0),
            );
            let accent = ui.visuals().hyperlink_color;
            let apply = soft_button(
                ui,
                egui::RichText::new("Apply Live").size(10.0).color(accent),
            );
            if ui.add(apply).clicked() {
                out.push(UiEvent::ApplyPlanLive);
            }
            let reset = egui::Button::new(egui::RichText::new("Reset Sim").size(10.0));
            if ui.add(reset).clicked() {
                out.push(UiEvent::RestartSim);
            }
        });
    }
    ui.label(
        egui::RichText::new(format!("Working Directory: {}", view.working_directory)).size(11.0),
    );
    ui.label(egui::RichText::new(format!("Main File: {}", view.main_file)).size(11.0));
    let label = if view.is_example_plan {
        "View Plan"
    } else {
        "View / Edit Plan"
    };
    if ui.button(egui::RichText::new(label).size(12.0)).clicked()
        && let Some(plan_id) = &view.plan_id
    {
        out.push(UiEvent::EditCapturePlan(plan_id.clone()));
    }
}

fn simulation_controls(ui: &mut egui::Ui, view: &SimScreenView, out: &mut Vec<UiEvent>) {
    if !view.running {
        // Dimmed until both orbits are chosen, with no hover needed (F-36, P0-004).
        let start = soft_button(ui, "Start Simulation");
        if full_width_button(ui, view.can_start, start).clicked() {
            out.push(UiEvent::StartSim);
        }
        return;
    }
    let map = soft_button(ui, "Map View (M)");
    if full_width_button(ui, true, map).clicked() {
        out.push(UiEvent::ToggleMapView);
    }
    let indicator = soft_button(
        ui,
        if view.show_satellite_indicator {
            "Hide Satellite Indicator"
        } else {
            "Show Satellite Indicator"
        },
    );
    if full_width_button(ui, true, indicator).clicked() {
        out.push(UiEvent::ToggleSatelliteIndicator);
    }
    let gizmos = soft_button(ui, "Toggle Capture Gizmos (C)");
    if full_width_button(ui, true, gizmos).clicked() {
        out.push(UiEvent::ToggleCaptureGizmos);
    }
    let cycle = soft_button(ui, "Cycle Target (Tab)");
    if full_width_button(ui, true, cycle).clicked() {
        out.push(UiEvent::CycleCameraTarget);
    }
    // Disabled once a capture is active (rc.3).
    let capture = soft_button(ui, "Capture");
    if full_width_button(ui, !view.capture_active, capture).clicked() {
        out.push(UiEvent::CaptureDebris {
            entity: view.capture_entity,
            plan_id: view.plan_id.clone().unwrap_or_default(),
        });
    }
    if full_width_button(ui, true, egui::Button::new("Reset Sim")).clicked() {
        out.push(UiEvent::RestartSim);
    }
}

fn time_warp(ui: &mut egui::Ui, view: &SimScreenView, out: &mut Vec<UiEvent>) {
    const BUTTON: f32 = 40.0;
    let accent = ui.visuals().hyperlink_color;
    let readout = egui::RichText::new(&view.time_warp)
        .size(30.0)
        .color(accent);
    let width = egui::WidgetText::from(readout.clone())
        .into_galley(
            ui,
            Some(egui::TextWrapMode::Extend),
            f32::INFINITY,
            egui::TextStyle::Body,
        )
        .size()
        .x;
    egui::Frame::new()
        .fill(ui.visuals().faint_bg_color)
        .inner_margin(12.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let spacing = ui.spacing().item_spacing.x;
                let row = width + 2.0 * (BUTTON + spacing);
                ui.add_space(((ui.available_width() - row) / 2.0).max(0.0));
                if ui
                    .add_sized([BUTTON, BUTTON], soft_button(ui, "<"))
                    .clicked()
                {
                    out.push(UiEvent::ChangeTimeWarp { increase: false });
                }
                ui.add(egui::Label::new(readout).extend());
                if ui
                    .add_sized([BUTTON, BUTTON], soft_button(ui, ">"))
                    .clicked()
                {
                    out.push(UiEvent::ChangeTimeWarp { increase: true });
                }
            });
        });
}

fn simulation_hud(ui: &mut egui::Ui, view: &SimScreenView) {
    ui.label(egui::RichText::new(&view.telemetry).size(12.0));
    egui::Frame::new()
        .fill(ui.visuals().faint_bg_color)
        .inner_margin(12.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 8.0;
            let accent = ui.visuals().hyperlink_color;
            ui.label(
                egui::RichText::new("Capture Guidance")
                    .size(13.0)
                    .color(accent),
            );
            ui.label(egui::RichText::new(&view.guidance).size(12.0));
        });
}

/// The sections in the plan's order (A-5).
pub fn sim_sidebar_ui(ui: &mut egui::Ui, view: &mut SimScreenView, out: &mut Vec<UiEvent>) {
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            egui::Frame::new().inner_margin(12.0).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 10.0;
                section(ui, "Project Information", |ui| {
                    project_information(ui, view, out);
                });
                section(ui, "Simulation Controls", |ui| {
                    simulation_controls(ui, view, out);
                });
                if view.running {
                    section(ui, "Time Warp", |ui| time_warp(ui, view, out));
                } else {
                    section(ui, "Orbital Selection", orbital_selection_section);
                }
                section(ui, "Simulation HUD", |ui| simulation_hud(ui, view));
                section(ui, "Reference", |ui| {
                    ui.weak(
                        egui::RichText::new(
                            "Target telemetry is measured against the tether root. Guidance shows the currently active state and the transitions available from it.",
                        )
                        .size(12.0),
                    );
                });
            });
        });
}

#[derive(SystemParam)]
pub struct SimScreenParams<'w, 's> {
    contexts: EguiContexts<'w, 's>,
    selected_project: Res<'w, SelectedProject>,
    capture_plan_lib: Res<'w, CapturePlanLibrary>,
    working_directory: Res<'w, WorkingDirectory>,
    sim_state: Res<'w, State<SimState>>,
    sync_state: Res<'w, SimPlanSyncState>,
    selection: Res<'w, OrbitalSelectionState>,
    catalog_ui: Res<'w, SpaceCatalogUiState>,
    readouts: Res<'w, HudReadouts>,
    capture_entities: Query<'w, 's, Entity, With<CaptureComponent>>,
    events: MessageWriter<'w, UiEvent>,
}

/// The header and the sidebar, once the terminal panel has taken its space.
pub fn sim_screen_panels(mut p: SimScreenParams) {
    let plan_id = p.selected_project.project_id.as_deref();
    let mut view = SimScreenView {
        title: plan_id
            .and_then(|id| p.capture_plan_lib.plans.get(id))
            .map(|plan| plan.name.clone())
            .unwrap_or_else(|| "No plan selected".to_string()),
        working_directory: p.working_directory.path.clone(),
        main_file: match plan_id {
            None | Some("") => "Unknown file".to_string(),
            Some(id) => format!("{id}.json"),
        },
        plan_id: plan_id.map(str::to_string),
        is_example_plan: plan_id.is_some_and(|id| !p.capture_plan_lib.user_plans.contains_key(id)),
        in_sync: p.sync_state.in_sync,
        running: *p.sim_state.get() == SimState::Running,
        can_start: p.selection.target.is_some() && p.selection.chaser.is_some(),
        capture_active: !p.capture_entities.is_empty(),
        capture_entity: p.readouts.target_entity,
        show_satellite_indicator: p.catalog_ui.show_satellite_indicator,
        time_warp: p.readouts.time_warp.clone(),
        telemetry: p.readouts.telemetry.clone(),
        guidance: p.readouts.guidance.clone(),
    };
    let Ok(ctx) = p.contexts.ctx_mut() else {
        return;
    };
    let visuals = ctx.style().visuals.clone();
    let mut out = Vec::new();
    egui::TopBottomPanel::top("sim_header")
        .exact_height(SIM_HEADER_HEIGHT)
        .frame(
            egui::Frame::new()
                .fill(visuals.widgets.open.bg_fill)
                .inner_margin(egui::Margin::symmetric(18, 0)),
        )
        .show(ctx, |ui| sim_header_ui(ui, &mut view, &mut out));
    let width = SIDEBAR_WIDTH.min(ctx.content_rect().width() * SIDEBAR_MAX_SHARE);
    egui::SidePanel::right("sim_sidebar")
        .resizable(false)
        .exact_width(width)
        .frame(egui::Frame::new().fill(visuals.faint_bg_color))
        .show(ctx, |ui| sim_sidebar_ui(ui, &mut view, &mut out));
    p.events.write_batch(out);
}

fn show_modal(
    ctx: &egui::Context,
    id: &str,
    out: &mut Vec<UiEvent>,
    body: impl FnOnce(&mut egui::Ui, &mut (), &mut Vec<UiEvent>),
) {
    let frame = egui::Frame::new()
        .fill(form_fill(&ctx.style().visuals))
        .inner_margin(28.0);
    egui::Modal::new(egui::Id::new(id))
        .frame(frame)
        .show(ctx, |ui| body(ui, &mut (), out));
}

/// The exit-confirm modal on the sim screen and the restart prompt wherever
/// a plan save raised it (rc.3: Home too, after a project was opened).
pub fn sim_screen_modals(
    mut contexts: EguiContexts,
    exit_confirm: Res<ExitConfirmOpen>,
    form: Res<NewCapturePlanForm>,
    mut events: MessageWriter<UiEvent>,
) {
    if !exit_confirm.0 && !form.show_restart_prompt {
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    let mut out = Vec::new();
    if exit_confirm.0 {
        show_modal(ctx, "sim_exit_confirm", &mut out, exit_confirm_ui);
    }
    if form.show_restart_prompt {
        show_modal(ctx, "sim_restart_prompt", &mut out, restart_prompt_ui);
    }
    events.write_batch(out);
}
