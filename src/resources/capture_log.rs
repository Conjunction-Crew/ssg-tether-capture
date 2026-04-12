use std::collections::{HashSet, VecDeque};

use bevy::prelude::*;

// ─── Log Level ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
}

impl LogLevel {
    pub fn label(&self) -> &'static str {
        match self {
            LogLevel::Error => "ERR",
            LogLevel::Warn => "WARN",
            LogLevel::Info => "INFO",
            LogLevel::Debug => "DBG",
        }
    }
}

// ─── Log Entry ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct LogEntry {
    /// Simulation timestamp formatted as "HH:MM:SS" (UTC).
    pub timestamp: String,
    pub level: LogLevel,
    /// Short identifier for the subsystem that emitted this entry.
    pub source: &'static str,
    pub message: String,
}

// ─── CaptureLog Resource ──────────────────────────────────────────────────────

/// Global ring-buffer of log entries written by simulation subsystems.
#[derive(Resource)]
pub struct CaptureLog {
    pub entries: VecDeque<LogEntry>,
    pub max_entries: usize,
}

impl Default for CaptureLog {
    fn default() -> Self {
        Self {
            entries: VecDeque::new(),
            max_entries: 256,
        }
    }
}

impl CaptureLog {
    /// Append an entry, evicting the oldest if the buffer is full.
    pub fn push(&mut self, entry: LogEntry) {
        if self.entries.len() >= self.max_entries {
            self.entries.pop_front();
        }
        self.entries.push_back(entry);
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

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
        }
    }
}

// ─── LogEvent ─────────────────────────────────────────────────────────────────

/// Bevy event sent by simulation systems to add an entry to the capture log.
#[derive(Message, Debug, Clone)]
pub struct LogEvent {
    pub level: LogLevel,
    /// Short label identifying the emitting subsystem (e.g. `"capture"`).
    pub source: &'static str,
    pub message: String,
}
