use std::path::Path;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_egui::{
    EguiContexts,
    egui::{self, Color32, RichText},
};

use crate::resources::capture_log::{CaptureLog, CaptureLogUiState, LogEntry, LogEvent, LogLevel};
use crate::resources::capture_plan_form::NewCapturePlanForm;
use crate::resources::working_directory::WorkingDirectory;
use crate::resources::world_time::WorldTime;

// ─── System ───────────────────────────────────────────────────────────────────

#[derive(SystemParam)]
pub struct TerminalParams<'w> {
    pub capture_log: ResMut<'w, CaptureLog>,
    pub log_ui: ResMut<'w, CaptureLogUiState>,
    pub working_directory: Res<'w, WorkingDirectory>,
    pub form: Res<'w, NewCapturePlanForm>,
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
    let screen_height = ctx.screen_rect().height();
    let min_h = if p.log_ui.is_open {
        screen_height * 0.25
    } else {
        28.0
    };
    let panel_response = egui::TopBottomPanel::bottom("capture_log_terminal")
        .resizable(true)
        .min_height(min_h)
        .max_height(screen_height * 0.75)
        .show(ctx, |ui| {
            // ── Toolbar ──────────────────────────────────────────────────────
            ui.horizontal(|ui| {
                // Collapse/expand toggle — left-anchored, mirrors CollapsingHeader convention
                {
                    let openness = if p.log_ui.is_open { 1.0 } else { 0.0 };
                    let icon_w = ui.spacing().icon_width;
                    let (_, resp) =
                        ui.allocate_exact_size(egui::vec2(icon_w, icon_w), egui::Sense::click());
                    egui::collapsing_header::paint_default_icon(ui, openness, &resp);
                    if resp.clicked() {
                        p.log_ui.is_open = !p.log_ui.is_open;
                    }
                }

                ui.label(
                    RichText::new("Capture Log")
                        .color(Color32::from_rgb(97, 168, 252))
                        .strong(),
                );

                ui.separator();

                // Log-level filter toggle buttons
                for (level, label, active_color) in [
                    (LogLevel::Error, "ERR", Color32::from_rgb(255, 89, 89)),
                    (LogLevel::Warn, "WARN", Color32::from_rgb(255, 217, 0)),
                    (LogLevel::Info, "INFO", Color32::from_rgb(239, 242, 249)),
                    (LogLevel::Debug, "DBG", Color32::from_rgb(153, 168, 199)),
                ] {
                    let is_active = p.log_ui.active_filters.contains(&level);
                    let text = if is_active {
                        RichText::new(label).color(active_color).strong()
                    } else {
                        RichText::new(label).color(Color32::from_gray(90))
                    };
                    if ui.selectable_label(is_active, text).clicked() {
                        if is_active {
                            p.log_ui.active_filters.remove(&level);
                        } else {
                            p.log_ui.active_filters.insert(level);
                        }
                        p.log_ui.last_rendered_filter = p.log_ui.active_filters.clone();
                        p.log_ui.is_user_scrolled = false;
                    }
                }

                ui.separator();

                if ui.button("Clear").clicked() {
                    p.capture_log.clear();
                    p.log_ui.last_rendered_count = 0;
                    p.log_ui.selected_rows = None;
                    p.log_ui.selection_anchor = None;
                    p.log_ui.is_user_scrolled = false;
                }

                if ui.button("Save").clicked() {
                    let epoch_str = p
                        .world_time
                        .as_ref()
                        .map(|wt| format!("{}", wt.epoch))
                        .unwrap_or_else(|| "unknown_time".to_string());

                    match write_log_to_file(
                        &p.capture_log,
                        &p.working_directory.path,
                        &p.form.plan_name,
                        &epoch_str,
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
            });

            // ── Log viewport (only when expanded) ────────────────────────────
            if !p.log_ui.is_open {
                return;
            }

            ui.separator();

            // Keyboard shortcuts: Ctrl+A (select all), Ctrl+C (copy selection)
            let ctrl_held =
                ctx.input(|i: &egui::InputState| i.modifiers.ctrl || i.modifiers.mac_cmd);
            if ctrl_held && ctx.input(|i: &egui::InputState| i.key_pressed(egui::Key::A)) {
                if !filtered.is_empty() {
                    p.log_ui.selection_anchor = Some(0);
                    p.log_ui.selected_rows = Some((0, filtered.len() - 1));
                }
            }
            if ctrl_held && ctx.input(|i: &egui::InputState| i.key_pressed(egui::Key::C)) {
                if let Some((sel_lo, sel_hi)) = p.log_ui.selected_rows {
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
            }

            // Clamp selection to current filtered list size
            if let Some((lo, hi)) = p.log_ui.selected_rows {
                if lo >= filtered.len() {
                    p.log_ui.selected_rows = None;
                    p.log_ui.selection_anchor = None;
                } else if hi >= filtered.len() {
                    p.log_ui.selected_rows = Some((lo, filtered.len().saturating_sub(1)));
                }
            }

            let auto_scroll = !p.log_ui.is_user_scrolled;

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .stick_to_bottom(auto_scroll)
                .show(ui, |ui| {
                    // Detect if the user scrolled upward so we stop auto-scrolling
                    let scroll_delta = ctx.input(|i: &egui::InputState| i.smooth_scroll_delta.y);
                    if scroll_delta > 0.0 {
                        p.log_ui.is_user_scrolled = true;
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

                        let is_selected = p
                            .log_ui
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
                                if let Some(anchor) = p.log_ui.selection_anchor {
                                    let lo = anchor.min(idx);
                                    let hi = anchor.max(idx);
                                    p.log_ui.selected_rows = Some((lo, hi));
                                } else {
                                    p.log_ui.selection_anchor = Some(idx);
                                    p.log_ui.selected_rows = Some((idx, idx));
                                }
                            } else {
                                p.log_ui.selection_anchor = Some(idx);
                                p.log_ui.selected_rows = Some((idx, idx));
                            }
                        }
                    }
                });
        });
    p.log_ui.panel_height = panel_response.response.rect.height();
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

fn write_log_to_file(
    capture_log: &CaptureLog,
    working_dir: &str,
    plan_name: &str,
    epoch_str: &str,
) -> Result<String, String> {
    let logs_dir = Path::new(working_dir).join("logs");
    std::fs::create_dir_all(&logs_dir)
        .map_err(|e| format!("could not create logs directory: {e}"))?;

    let sanitized_plan: String = plan_name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let sanitized_plan = if sanitized_plan.is_empty() {
        "unnamed".to_string()
    } else {
        sanitized_plan
    };

    let sanitized_epoch: String = epoch_str
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();

    let filename = format!("{sanitized_plan}_{sanitized_epoch}.jsonl");
    let filepath = logs_dir.join(&filename);

    let lines: Result<Vec<String>, _> = capture_log
        .entries
        .iter()
        .map(|entry| serde_json::to_string(entry))
        .collect();
    let content = lines.map_err(|e| format!("serialization error: {e}"))?;
    let content = content.join("\n");

    std::fs::write(&filepath, content).map_err(|e| format!("write error: {e}"))?;

    Ok(filename)
}
