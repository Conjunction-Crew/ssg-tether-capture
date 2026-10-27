//! Display state for the capture log and the plan load errors.

use std::collections::HashSet;

use bevy::{platform::collections::HashMap, prelude::*};

use crate::sim::telemetry::LogLevel;

// ─── CaptureLogUiState Resource ───────────────────────────────────────────────

/// Transient UI state for the capture log terminal panel.
#[derive(Resource)]
pub struct CaptureLogUiState {
    /// Whether the log viewport is currently expanded.
    pub is_open: bool,
    /// Log levels shown in the viewport; entries with other levels are hidden.
    pub active_filters: HashSet<LogLevel>,
    /// True when the user has manually scrolled up; disables auto-scroll.
    pub is_user_scrolled: bool,
    /// Entry count at the last display rebuild — used to detect new entries.
    pub last_rendered_count: usize,
    /// Filter set at the last display rebuild — used to detect filter changes.
    pub last_rendered_filter: HashSet<LogLevel>,
    /// Row range currently selected (indices into the filtered list).
    pub selected_rows: Option<(usize, usize)>,
    /// Anchor row for shift-click range selection.
    pub selection_anchor: Option<usize>,
    /// Height in pixels of the rendered terminal panel (updated each frame).
    /// Used by other systems to offset UI elements above the terminal.
    pub panel_height: f32,
}

impl Default for CaptureLogUiState {
    fn default() -> Self {
        let mut all = HashSet::new();
        all.insert(LogLevel::Error);
        all.insert(LogLevel::Warn);
        all.insert(LogLevel::Info);
        all.insert(LogLevel::Debug);
        Self {
            is_open: false,
            active_filters: all.clone(),
            is_user_scrolled: false,
            last_rendered_count: 0,
            last_rendered_filter: all,
            selected_rows: None,
            selection_anchor: None,
            panel_height: 0.0,
        }
    }
}

/// Stores per-file validation errors for capture plans that failed to load.
#[derive(Resource, Debug, Default)]
pub struct CapturePlanLoadErrors {
    pub errors: HashMap<String, Vec<String>>,
}
