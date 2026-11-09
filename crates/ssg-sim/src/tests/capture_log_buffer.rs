//! The capture log's buffer and timestamps (#175, F-26): 1,024 entries with DBG
//! lines evicted first, and one `HH:MM:SS.mmm` stamp shared by the panel's log and
//! the recorder's `log.jsonl`.

use std::sync::{Arc, Mutex};

use bevy::prelude::*;

use crate::recording::inputs::RecordedInput;
use crate::recording::recorder::{RecorderCommand, RecordingSettings};
use crate::recording::{
    Frame, FrameSink, RecordedTransition, RecordingEnd, RecordingHeader, RecordingSink,
};
use crate::telemetry::{
    CAPTURE_LOG_CAPACITY, CaptureLog, LogEntry, LogEvent, LogLevel, log_timestamp,
};
use crate::test_support::{fixed_epoch, load_example_plans, q13, test_app, tick};

fn entry(level: LogLevel, message: String) -> LogEntry {
    LogEntry {
        timestamp: "00:00:00.000".to_string(),
        epoch_utc: "2026-04-03T00:00:00.000000Z".to_string(),
        level,
        source: "test",
        message,
    }
}

#[test]
fn the_default_log_holds_1024_entries() {
    assert_eq!(CAPTURE_LOG_CAPACITY, 1024);
    assert_eq!(CaptureLog::default().max_entries, CAPTURE_LOG_CAPACITY);
}

#[test]
fn a_full_log_evicts_debug_lines_first() {
    let mut log = CaptureLog::default();
    log.push(entry(LogLevel::Info, "phase transition".into()));
    for i in 0..5000 {
        log.push(entry(LogLevel::Debug, format!("dbg {i}")));
    }
    assert_eq!(log.entries.len(), 1024);
    assert_eq!(log.entries[0].message, "phase transition");
    assert_eq!(
        log.entries[1].message, "dbg 3977",
        "the oldest DBG lines went"
    );
    assert_eq!(log.entries[1023].message, "dbg 4999");
}

#[test]
fn a_full_log_keeps_its_non_debug_lines_in_order() {
    let mut log = CaptureLog {
        entries: Default::default(),
        max_entries: 4,
    };
    log.push(entry(LogLevel::Warn, "w".into()));
    log.push(entry(LogLevel::Debug, "d1".into()));
    log.push(entry(LogLevel::Error, "e".into()));
    log.push(entry(LogLevel::Debug, "d2".into()));
    log.push(entry(LogLevel::Info, "i".into()));
    let messages: Vec<_> = log.entries.iter().map(|e| e.message.as_str()).collect();
    assert_eq!(messages, ["w", "e", "d2", "i"], "d1 (the oldest DBG) went");
}

#[test]
fn a_log_without_debug_lines_evicts_the_oldest() {
    let mut log = CaptureLog::default();
    for i in 0..2000 {
        log.push(entry(LogLevel::Info, format!("info {i}")));
    }
    assert_eq!(log.entries.len(), 1024);
    assert_eq!(log.entries[0].message, "info 976", "the first 976 went");
    assert_eq!(log.entries[1023].message, "info 1999");
}

#[test]
fn the_log_timestamp_is_the_time_of_day_to_the_millisecond() {
    // fixed_epoch() = 2026-04-03T16:03:47.002464Z: truncated, not rounded.
    assert_eq!(log_timestamp(fixed_epoch()), "16:03:47.002");
    assert_eq!(log_timestamp(fixed_epoch() + 0.9975), "16:03:47.999");
    assert_eq!(log_timestamp(fixed_epoch() + 0.998), "16:03:48.000");
}

/// A sink that keeps every log entry it is handed, whole.
#[derive(Clone, Default)]
struct EntrySink(Arc<Mutex<Vec<LogEntry>>>);

impl FrameSink for EntrySink {
    fn begin(&mut self, _: &RecordingHeader) {}
    fn frame(&mut self, _: &Frame) {}
    fn transition(&mut self, _: &RecordedTransition) {}
    fn log(&mut self, entry: &LogEntry) {
        self.0.lock().unwrap().push(entry.clone());
    }
    fn inputs(&mut self, _: &[RecordedInput]) {}
    fn bytes(&self) -> u64 {
        0
    }
    fn finish(&mut self, _: &RecordingEnd) {}
}

#[test]
fn the_recorder_and_the_panel_stamp_alike() {
    let sink = EntrySink::default();
    let mut app = test_app();
    load_example_plans(&mut app);
    app.insert_resource(RecordingSettings::default());
    app.insert_resource(RecordingSink(Box::new(sink.clone())));
    q13::build_with_command(&mut app, q13::PLAN_ID);
    app.world_mut()
        .write_message(RecorderCommand::Start { name: "s".into() });
    for _ in 0..3 {
        tick(&mut app);
    }
    app.world_mut().write_message(LogEvent {
        level: LogLevel::Info,
        source: "test",
        message: "stamped twice".into(),
    });
    tick(&mut app);

    let recorded = sink.0.lock().unwrap().clone();
    let recorded = recorded
        .iter()
        .find(|e| e.message == "stamped twice")
        .expect("the recorder logged the line");
    let log = app.world().resource::<CaptureLog>();
    let panel = log
        .entries
        .iter()
        .find(|e| e.message == "stamped twice")
        .expect("the capture log holds the line");
    assert_eq!(recorded.timestamp, panel.timestamp);
    assert_eq!(recorded.epoch_utc, panel.epoch_utc);
    assert_eq!(recorded.timestamp, recorded.epoch_utc[11..23]);
    assert_eq!(recorded.timestamp.len(), "HH:MM:SS.mmm".len());
}
