use std::path::Path;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_egui::{
    EguiContexts,
    egui::{self, Color32, RichText},
};

use crate::ui::log_ui::CaptureLogUiState;
use ssg_sim::project::export::write_log_jsonl;
use ssg_sim::project::{SelectedProject, WorkingDirectory};
use ssg_sim::telemetry::{CaptureLog, LogEntry, LogEvent, LogLevel};
use ssg_sim::time::WorldTime;

// ─── System ───────────────────────────────────────────────────────────────────

#[derive(SystemParam)]
pub struct TerminalParams<'w> {
    pub capture_log: ResMut<'w, CaptureLog>,
    pub log_ui: ResMut<'w, CaptureLogUiState>,
    pub working_directory: Res<'w, WorkingDirectory>,
    pub selected_project: Res<'w, SelectedProject>,
    pub world_time: Option<Res<'w, WorldTime>>,
    pub log_writer: MessageWriter<'w, LogEvent>,
}

/// Renders the capture log terminal as an egui bottom panel.
///
/// This system must be scheduled *before* `egui_plots` on the
/// `EguiPrimaryContextPass` so that the panel claims its space from
/// `ctx.available_rect()` first, allowing the Data Collection window to
/// be constrained above it automatically.
pub fn egui_terminal_panel(mut contexts: EguiContexts, mut p: TerminalParams) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    // ── Collect filtered entries as owned data ───────────────────────────────
    // Cloning before entering the egui closure avoids a simultaneous immutable
    // borrow on `p.capture_log.entries` and mutable borrow via `clear()`.
    let filtered: Vec<LogEntry> = p
        .capture_log
        .entries
        .iter()
        .filter(|e| p.log_ui.active_filters.contains(&e.level))
        .cloned()
        .collect();

    // ── Detect content changes (same trigger as the old Bevy UI rebuild) ─────
    let needs_auto_scroll = p.capture_log.entries.len() != p.log_ui.last_rendered_count
        || p.log_ui.active_filters != p.log_ui.last_rendered_filter;
    if needs_auto_scroll {
        p.log_ui.is_user_scrolled = false;
        p.log_ui.last_rendered_count = p.capture_log.entries.len();
        p.log_ui.last_rendered_filter = p.log_ui.active_filters.clone();
    }

    // ── Panel ────────────────────────────────────────────────────────────────
    let screen_height = ctx.content_rect().height();
    let mut save = false;
    let TerminalParams {
        capture_log,
        log_ui,
        ..
    } = &mut p;
    let panel_response = terminal_panel(log_ui.is_open, screen_height).show(ctx, |ui| {
        save = terminal_contents(ui, log_ui, capture_log, &filtered);
    });
    p.log_ui.panel_height = panel_response.response.rect.height();

    if save {
        let epoch_str = p
            .world_time
            .as_ref()
            .map(|wt| format!("{}", wt.epoch))
            .unwrap_or_else(|| "unknown_time".to_string());

        // Named after the active plan id (F-26: it was the plan form's
        // name, empty outside the editor).
        match write_log_jsonl(
            Path::new(&p.working_directory.path),
            p.selected_project.project_id.as_deref().unwrap_or_default(),
            &epoch_str,
            &p.capture_log.entries,
        ) {
            Ok(filename) => {
                p.log_writer.write(LogEvent {
                    level: LogLevel::Info,
                    source: "system",
                    message: format!("Log saved to logs/{filename}"),
                });
            }
            Err(e) => {
                p.log_writer.write(LogEvent {
                    level: LogLevel::Error,
                    source: "system",
                    message: format!("Failed to save log: {e}"),
                });
            }
        }
    }
}

// ─── Panel ────────────────────────────────────────────────────────────────────

/// The bottom panel. F-67: it can be resized only while the log is open, so
/// the collapsed bar's divider neither highlights on hover nor drags.
pub(crate) fn terminal_panel(is_open: bool, screen_height: f32) -> egui::TopBottomPanel {
    let min_h = if is_open { screen_height * 0.25 } else { 28.0 };
    egui::TopBottomPanel::bottom("capture_log_terminal")
        .resizable(is_open)
        .min_height(min_h)
        .max_height(screen_height * 0.75)
}

/// The colour of an active filter label, drawn on the selected (accent) fill.
/// F-67 (A-7): darkened from the log rows' colours ([`level_color`], unchanged),
/// which were too light on that fill.
pub(crate) fn filter_color(level: LogLevel) -> Color32 {
    match level {
        LogLevel::Error => Color32::from_rgb(110, 0, 0),
        LogLevel::Warn => Color32::from_rgb(90, 60, 0),
        LogLevel::Info => Color32::from_rgb(10, 20, 50),
        LogLevel::Debug => Color32::from_rgb(40, 50, 75),
    }
}

/// The toolbar and, while the log is open, the log viewport. Returns whether
/// Save was clicked (the caller writes the file).
pub(crate) fn terminal_contents(
    ui: &mut egui::Ui,
    log_ui: &mut CaptureLogUiState,
    capture_log: &mut CaptureLog,
    filtered: &[LogEntry],
) -> bool {
    let mut save = false;
    let ctx = ui.ctx().clone();

    // ── Toolbar ──────────────────────────────────────────────────────────────
    ui.horizontal(|ui| {
        // Collapse/expand toggle — left-anchored, mirrors CollapsingHeader convention
        {
            let openness = if log_ui.is_open { 1.0 } else { 0.0 };
            let icon_w = ui.spacing().icon_width;
            let (_, resp) =
                ui.allocate_exact_size(egui::vec2(icon_w, icon_w), egui::Sense::click());
            egui::collapsing_header::paint_default_icon(ui, openness, &resp);
            if resp.clicked() {
                log_ui.is_open = !log_ui.is_open;
            }
        }

        ui.label(
            RichText::new("Capture Log")
                .color(Color32::from_rgb(97, 168, 252))
                .strong(),
        );

        ui.separator();

        // F-67: while the log is collapsed the buttons are shown but disabled,
        // so they neither highlight on hover nor respond to clicks.
        let is_open = log_ui.is_open;
        ui.add_enabled_ui(is_open, |ui| {
            // Log-level filter toggle buttons
            for (level, label) in [
                (LogLevel::Error, "ERR"),
                (LogLevel::Warn, "WARN"),
                (LogLevel::Info, "INFO"),
                (LogLevel::Debug, "DBG"),
            ] {
                let is_active = log_ui.active_filters.contains(&level);
                let text = if is_active {
                    RichText::new(label).color(filter_color(level)).strong()
                } else {
                    RichText::new(label).color(Color32::from_gray(90))
                };
                if ui.selectable_label(is_active, text).clicked() {
                    if is_active {
                        log_ui.active_filters.remove(&level);
                    } else {
                        log_ui.active_filters.insert(level);
                    }
                    log_ui.last_rendered_filter = log_ui.active_filters.clone();
                    log_ui.is_user_scrolled = false;
                }
            }

            ui.separator();

            if ui.button("Clear").clicked() {
                capture_log.clear();
                log_ui.last_rendered_count = 0;
                log_ui.selected_rows = None;
                log_ui.selection_anchor = None;
                log_ui.is_user_scrolled = false;
            }

            save |= ui.button("Save").clicked();
        });
    });

    // ── Log viewport (only when expanded) ────────────────────────────────────
    if !log_ui.is_open {
        return save;
    }

    ui.separator();

    // Keyboard shortcuts: Ctrl+A (select all), Ctrl+C (copy selection)
    let ctrl_held = ctx.input(|i: &egui::InputState| i.modifiers.ctrl || i.modifiers.mac_cmd);
    if ctrl_held
        && ctx.input(|i: &egui::InputState| i.key_pressed(egui::Key::A))
        && !filtered.is_empty()
    {
        log_ui.selection_anchor = Some(0);
        log_ui.selected_rows = Some((0, filtered.len() - 1));
    }
    if ctrl_held
        && ctx.input(|i: &egui::InputState| i.key_pressed(egui::Key::C))
        && let Some((sel_lo, sel_hi)) = log_ui.selected_rows
    {
        let lo = sel_lo.min(sel_hi);
        let hi = sel_lo.max(sel_hi);
        let mut text = String::new();
        for idx in lo..=hi {
            if let Some(entry) = filtered.get(idx) {
                text.push_str(&format!(
                    "[{}] [{:>4}] {}: {}\n",
                    entry.timestamp,
                    entry.level.label(),
                    entry.source,
                    entry.message,
                ));
            }
        }
        if !text.is_empty() {
            ctx.copy_text(text);
        }
    }

    // Clamp selection to current filtered list size
    if let Some((lo, hi)) = log_ui.selected_rows {
        if lo >= filtered.len() {
            log_ui.selected_rows = None;
            log_ui.selection_anchor = None;
        } else if hi >= filtered.len() {
            log_ui.selected_rows = Some((lo, filtered.len().saturating_sub(1)));
        }
    }

    let auto_scroll = !log_ui.is_user_scrolled;

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .stick_to_bottom(auto_scroll)
        .show(ui, |ui| {
            // Detect if the user scrolled upward so we stop auto-scrolling
            let scroll_delta = ctx.input(|i: &egui::InputState| i.smooth_scroll_delta.y);
            if scroll_delta > 0.0 {
                log_ui.is_user_scrolled = true;
            }

            for (idx, entry) in filtered.iter().enumerate() {
                let text_color = level_color(entry.level);
                let row_text = format!(
                    "[{}] [{:>4}] {}: {}",
                    entry.timestamp,
                    entry.level.label(),
                    entry.source,
                    entry.message,
                );

                let is_selected = log_ui
                    .selected_rows
                    .map(|(lo, hi)| idx >= lo && idx <= hi)
                    .unwrap_or(false);

                let label = ui.selectable_label(
                    is_selected,
                    RichText::new(&row_text)
                        .color(text_color)
                        .monospace()
                        .size(11.0),
                );

                if label.clicked() {
                    let shift = ctx.input(|i: &egui::InputState| i.modifiers.shift);
                    if shift {
                        if let Some(anchor) = log_ui.selection_anchor {
                            let lo = anchor.min(idx);
                            let hi = anchor.max(idx);
                            log_ui.selected_rows = Some((lo, hi));
                        } else {
                            log_ui.selection_anchor = Some(idx);
                            log_ui.selected_rows = Some((idx, idx));
                        }
                    } else {
                        log_ui.selection_anchor = Some(idx);
                        log_ui.selected_rows = Some((idx, idx));
                    }
                }
            }
        });
    save
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn level_color(level: LogLevel) -> Color32 {
    match level {
        LogLevel::Error => Color32::from_rgb(255, 89, 89),
        LogLevel::Warn => Color32::from_rgb(255, 217, 0),
        LogLevel::Info => Color32::from_rgb(239, 242, 249),
        LogLevel::Debug => Color32::from_rgb(153, 168, 199),
    }
}
