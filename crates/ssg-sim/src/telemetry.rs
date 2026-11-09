//! Telemetry: the capture log and the per-run data collection.

use std::collections::VecDeque;

use bevy::{platform::collections::HashMap, prelude::*};
use brahe::Epoch;
use nalgebra::Vector6;
use serde::Serialize;

use crate::project::format_utc;
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
    /// Simulation timestamp formatted as "HH:MM:SS.mmm" (UTC; [`log_timestamp`]).
    pub timestamp: String,
    /// The simulation epoch in the project-file format (`format_utc`, µs).
    pub epoch_utc: String,
    pub level: LogLevel,
    /// Short identifier for the subsystem that emitted this entry.
    pub source: &'static str,
    pub message: String,
}

// ─── CaptureLog Resource ──────────────────────────────────────────────────────

/// The number of entries the capture log holds (#175).
pub const CAPTURE_LOG_CAPACITY: usize = 1024;

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
            max_entries: CAPTURE_LOG_CAPACITY,
        }
    }
}

impl CaptureLog {
    /// Append an entry. A full buffer evicts its oldest DBG entry, or its oldest
    /// entry when it holds no DBG entry (#175).
    pub fn push(&mut self, entry: LogEntry) {
        if self.entries.len() >= self.max_entries {
            match self.entries.iter().position(|e| e.level == LogLevel::Debug) {
                Some(i) => {
                    self.entries.remove(i);
                }
                None => {
                    self.entries.pop_front();
                }
            }
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

/// The [`LogEntry::timestamp`] of an entry written with no sim session (F-42):
/// there is no sim clock to read.
pub const NO_SIM_TIME: &str = "--:--:--.---";

/// The [`LogEntry::timestamp`] of an entry at `epoch`: `HH:MM:SS.mmm`, the time of
/// day of [`format_utc`] truncated to milliseconds (the same instant as
/// `epoch_utc`). The capture log and the recorder's `log.jsonl` both use it.
pub fn log_timestamp(epoch: Epoch) -> String {
    let full = format_utc(epoch);
    full.get(11..23)
        .map_or_else(|| full.clone(), str::to_string)
}

/// Collects [`LogEvent`]s emitted this frame and appends them to [`CaptureLog`],
/// on every screen (F-42). Outside a sim session the entry has no sim time:
/// [`NO_SIM_TIME`] and an empty `epoch_utc`.
pub fn collect_log_events(
    mut events: MessageReader<LogEvent>,
    mut capture_log: ResMut<CaptureLog>,
    world_time: Option<Res<WorldTime>>,
) {
    for event in events.read() {
        let (timestamp, epoch_utc) = match &world_time {
            Some(world_time) => (
                log_timestamp(world_time.epoch),
                format_utc(world_time.epoch),
            ),
            None => (NO_SIM_TIME.to_string(), String::new()),
        };
        capture_log.push(LogEntry {
            timestamp,
            epoch_utc,
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
            .add_systems(Last, collect_log_events);
    }
}
