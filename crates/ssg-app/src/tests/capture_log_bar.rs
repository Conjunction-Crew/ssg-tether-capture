//! The Capture Log bar (F-67, P9 S7): collapsed, its divider does not drag and
//! its buttons do not respond; open, both work as before.

use std::path::PathBuf;

use bevy_egui::egui;
use egui_kittest::kittest::{NodeT, Queryable};
use ssg_sim::telemetry::{CaptureLog, LogEntry, LogLevel};

use crate::tests::egui_support::{harness_for, snapshot};
use crate::ui::egui_terminal::{terminal_contents, terminal_panel};
use crate::ui::events::UiEvent;
use crate::ui::log_ui::CaptureLogUiState;

const SIZE: [f32; 2] = [900.0, 400.0];

struct Bar {
    log_ui: CaptureLogUiState,
    capture_log: CaptureLog,
    /// The panel's height on the last frame.
    height: f32,
    /// The cursor egui asked for on the last frame (a resize arrow over a
    /// resizable panel's divider).
    cursor: egui::CursorIcon,
    /// Whether any widget (the divider's resize handle) was being dragged.
    dragging: bool,
}

fn bar(is_open: bool) -> Bar {
    let mut capture_log = CaptureLog::default();
    for (level, message) in [
        (LogLevel::Error, "Capture plan 'ghost' not found"),
        (LogLevel::Warn, "Entity already marked for capture"),
        (LogLevel::Info, "Capture initiated with plan 'example_plan'"),
        (LogLevel::Debug, "Switched to map view"),
    ] {
        capture_log.push(LogEntry {
            timestamp: "19:01:37".to_string(),
            epoch_utc: "2026-11-07T19:01:37.000000Z".to_string(),
            level,
            source: "ui",
            message: message.to_string(),
        });
    }
    Bar {
        log_ui: CaptureLogUiState {
            is_open,
            ..Default::default()
        },
        capture_log,
        height: 0.0,
        cursor: egui::CursorIcon::Default,
        dragging: false,
    }
}

/// As `egui_terminal_panel` draws it, inside the harness's window.
fn bar_ui(ui: &mut egui::Ui, bar: &mut Bar, _out: &mut Vec<UiEvent>) {
    let filtered: Vec<LogEntry> = bar
        .capture_log
        .entries
        .iter()
        .filter(|e| bar.log_ui.active_filters.contains(&e.level))
        .cloned()
        .collect();
    let height = ui.available_height();
    let Bar {
        log_ui,
        capture_log,
        ..
    } = bar;
    let response = terminal_panel(log_ui.is_open, height).show_inside(ui, |ui| {
        terminal_contents(ui, log_ui, capture_log, &filtered);
    });
    bar.height = response.response.rect.height();
    bar.cursor = ui.ctx().output(|o| o.cursor_icon);
    bar.dragging = ui.ctx().dragged_id().is_some();
}

/// What a drag of the panel's top edge did.
#[derive(Debug)]
struct Drag {
    /// The cursor while hovering the divider.
    hover_cursor: egui::CursorIcon,
    /// Whether egui reported a drag in progress halfway through.
    dragged: bool,
    height_before: f32,
    height_after: f32,
}

/// Hovers the panel's top edge, then drags it 120 px up.
fn drag_the_divider(is_open: bool) -> Drag {
    let mut harness = harness_for(bar(is_open), SIZE, bar_ui);
    harness.run();
    let height_before = harness.state().view.height;
    let title = harness.get_by_label("Capture Log").rect();
    // The divider is the panel's top edge, a few px above the toolbar.
    let edge = egui::pos2(title.center().x + 300.0, title.top() - 4.0);
    harness.hover_at(edge);
    harness.run();
    let hover_cursor = harness.state().view.cursor;
    harness.drag_at(edge);
    harness.run();
    let mut dragged = false;
    for step in 1..=6 {
        harness.hover_at(edge - egui::vec2(0.0, 20.0 * step as f32));
        harness.run();
        dragged |= harness.state().view.dragging;
    }
    harness.drop_at(edge - egui::vec2(0.0, 120.0));
    harness.run();
    harness.run();
    Drag {
        hover_cursor,
        dragged,
        height_before,
        height_after: harness.state().view.height,
    }
}

#[test]
fn the_collapsed_bars_divider_neither_highlights_nor_drags() {
    let drag = drag_the_divider(false);
    assert_ne!(
        drag.hover_cursor,
        egui::CursorIcon::ResizeVertical,
        "F-67: no resize highlight while collapsed"
    );
    assert!(!drag.dragged, "F-67: the collapsed divider must not drag");
    assert_eq!(drag.height_after, drag.height_before);
}

#[test]
fn the_open_log_still_resizes_from_its_divider() {
    // The control for the test above: the same hover and drag on an open log.
    let drag = drag_the_divider(true);
    assert_eq!(drag.hover_cursor, egui::CursorIcon::ResizeVertical);
    assert!(drag.dragged);
    assert!(drag.height_after > drag.height_before + 50.0, "{drag:?}");
}

#[test]
fn the_collapsed_bars_buttons_are_disabled_and_ignore_clicks() {
    let mut harness = harness_for(bar(false), SIZE, bar_ui);
    harness.run();
    for label in ["ERR", "WARN", "INFO", "DBG", "Clear", "Save"] {
        let node = harness.get_by_label(label);
        assert!(node.accesskit_node().is_disabled(), "{label} is enabled");
        node.click();
        harness.run();
    }
    let state = &harness.state().view;
    assert_eq!(state.log_ui.active_filters.len(), 4, "no filter toggled");
    assert_eq!(state.capture_log.entries.len(), 4, "Clear did nothing");
}

#[test]
fn the_open_bars_buttons_work() {
    let mut harness = harness_for(bar(true), SIZE, bar_ui);
    harness.run();
    let node = harness.get_by_label("DBG");
    assert!(!node.accesskit_node().is_disabled());
    node.click();
    harness.run();
    assert!(
        !harness
            .state()
            .view
            .log_ui
            .active_filters
            .contains(&LogLevel::Debug)
    );
    harness.get_by_label("Clear").click();
    harness.run();
    assert!(harness.state().view.capture_log.entries.is_empty());
}

/// The S7 (P9) PNGs: the bar collapsed (buttons disabled) and open (the
/// darkened filter colours, the log rows' colours unchanged).
pub(crate) fn p9_s7_log_bar_snapshots() -> Vec<PathBuf> {
    vec![
        snapshot("p9_log_bar_collapsed", bar(false), [900.0, 120.0], bar_ui),
        snapshot("p9_log_bar_open", bar(true), SIZE, bar_ui),
    ]
}
