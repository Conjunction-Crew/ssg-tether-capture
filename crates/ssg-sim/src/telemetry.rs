//! Telemetry: the capture log and the per-run data collection.

use std::collections::VecDeque;

use bevy::{platform::collections::HashMap, prelude::*};
use brahe::Epoch;
use nalgebra::Vector6;
use serde::Serialize;

use crate::project::format_utc;
use crate::session::UiScreen;
use crate::time::WorldTime;

// ─── Log Level ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
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

#[derive(Debug, Clone, Serialize)]
pub struct LogEntry {
    /// Simulation timestamp formatted as "HH:MM:SS" (UTC).
    pub timestamp: String,
    /// The simulation epoch in the project-file format (`format_utc`, µs).
    pub epoch_utc: String,
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

// ─── LogEvent ─────────────────────────────────────────────────────────────────

/// Bevy event sent by simulation systems to add an entry to the capture log.
#[derive(Message, Debug, Clone)]
pub struct LogEvent {
    pub level: LogLevel,
    /// Short label identifying the emitting subsystem (e.g. `"capture"`).
    pub source: &'static str,
    pub message: String,
}

/// Collects [`LogEvent`]s emitted this frame and appends them to [`CaptureLog`].
pub fn collect_log_events(
    mut events: MessageReader<LogEvent>,
    mut capture_log: ResMut<CaptureLog>,
    world_time: Res<WorldTime>,
) {
    for event in events.read() {
        let full_ts = format!("{}", world_time.epoch);
        // Extract HH:MM:SS from ISO timestamp (chars 11–19 of "YYYY-MM-DDTHH:MM:SS…").
        let timestamp = full_ts
            .get(11..19)
            .map(|s| s.to_string())
            .unwrap_or_else(|| full_ts.clone());
        capture_log.push(LogEntry {
            timestamp,
            epoch_utc: format_utc(world_time.epoch),
            level: event.level,
            source: event.source,
            message: event.message.clone(),
        });
    }
}

/// One ephemeris sample of a capture: the ECI (GCRF) states of the capture
/// target and of the tether root (position in m, velocity in m/s) at `epoch`.
#[derive(Debug, Clone, PartialEq)]
pub struct EphemerisSample {
    pub epoch: Epoch,
    pub target: Vector6<f64>,
    pub chaser: Vector6<f64>,
}

/// Relative range and speed samples per capture entity (the plots and the CSV
/// export read them; the export settings are the app's `CsvExportState`), and the
/// ephemeris samples the OEM export reads.
#[derive(Resource, Debug, Default)]
pub struct DataCollection {
    pub velocity: HashMap<Entity, Vec<(f64, f64)>>,
    pub position: HashMap<Entity, Vec<(f64, f64)>>,
    /// One sample every [`EPHEMERIS_EVERY_STEPS`](crate::capture::EPHEMERIS_EVERY_STEPS)
    /// steps of a running capture.
    pub ephemeris: HashMap<Entity, Vec<EphemerisSample>>,
    // unused today; the P3 component/resource audit decides
    #[allow(dead_code)]
    pub forces: HashMap<Entity, Vec<(f64, f64)>>,
}

pub struct TelemetryPlugin;

impl Plugin for TelemetryPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<LogEvent>()
            .init_resource::<CaptureLog>()
            .init_resource::<DataCollection>()
            .add_systems(Last, collect_log_events.run_if(in_state(UiScreen::Sim)));
    }
}
