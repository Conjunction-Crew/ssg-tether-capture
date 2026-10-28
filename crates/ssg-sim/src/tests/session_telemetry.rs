//! Tests for the session lifecycle (`session.rs`) and the capture log's wiring
//! (`telemetry.rs`) in the full plugin set (P5 S6).

use bevy::prelude::*;

use crate::capture::{CaptureSphereRadius, INITIAL_CAPTURE_SPHERE_RADIUS_M};
use crate::orbit::cache::OrbitalCache;
use crate::session::{PauseState, UiScreen};
use crate::telemetry::{CaptureLog, DataCollection, LogEvent, LogLevel};
use crate::test_support::{LogTap, assert_close, enter_sim, fixed_epoch, test_app};
use crate::time::WorldTime;

fn leave_sim(app: &mut App) {
    app.world_mut()
        .resource_mut::<NextState<UiScreen>>()
        .set(UiScreen::Home);
    app.update();
}

fn send(app: &mut App, level: LogLevel, source: &'static str, message: &str) {
    app.world_mut().write_message(LogEvent {
        level,
        source,
        message: message.to_string(),
    });
}

fn log_messages(app: &App) -> Vec<String> {
    app.world()
        .resource::<CaptureLog>()
        .entries
        .iter()
        .map(|entry| entry.message.clone())
        .collect()
}

// ---------------------------------------------------------------------------
// session: OnEnter(Sim) / OnExit(Sim)
// ---------------------------------------------------------------------------

#[test]
fn the_session_resources_do_not_exist_before_the_sim_screen() {
    let app = test_app();

    assert!(app.world().get_resource::<OrbitalCache>().is_none());
    assert!(app.world().get_resource::<WorldTime>().is_none());
    assert!(app.world().get_resource::<CaptureSphereRadius>().is_none());
}

#[test]
fn entering_the_sim_creates_the_session_resources_at_their_start_values() {
    let mut app = test_app();

    enter_sim(&mut app);

    let cache = app.world().resource::<OrbitalCache>();
    assert!(cache.tethers.is_empty() && cache.debris.is_empty());
    let time = app.world().resource::<WorldTime>();
    assert_eq!(time.epoch, fixed_epoch());
    assert_eq!(time.start_epoch, fixed_epoch());
    assert_eq!(time.multiplier, 1);
    assert_close(
        app.world().resource::<CaptureSphereRadius>().radius,
        INITIAL_CAPTURE_SPHERE_RADIUS_M,
        0.0,
        "capture sphere radius",
    );
}

#[test]
fn entering_the_sim_resumes_a_paused_clock() {
    let mut app = test_app();
    app.world_mut()
        .resource_mut::<NextState<PauseState>>()
        .set(PauseState::Paused);
    app.update();
    assert_eq!(
        *app.world().resource::<State<PauseState>>().get(),
        PauseState::Paused,
        "precondition"
    );

    enter_sim(&mut app);
    app.update();

    assert_eq!(
        *app.world().resource::<State<PauseState>>().get(),
        PauseState::Running
    );
}

#[test]
fn leaving_the_sim_removes_the_session_resources() {
    let mut app = test_app();
    enter_sim(&mut app);
    assert!(app.world().get_resource::<OrbitalCache>().is_some(), "precondition");

    leave_sim(&mut app);

    assert!(app.world().get_resource::<OrbitalCache>().is_none());
    assert!(app.world().get_resource::<WorldTime>().is_none());
    assert!(app.world().get_resource::<CaptureSphereRadius>().is_none());
}

#[test]
fn reentering_the_sim_starts_from_fresh_session_resources() {
    let mut app = test_app();
    enter_sim(&mut app);
    {
        let world = app.world_mut();
        let body = world.spawn_empty().id();
        world
            .resource_mut::<OrbitalCache>()
            .debris
            .insert("Satellite1".to_string(), body);
        world.resource_mut::<CaptureSphereRadius>().radius = 3.0;
        let mut time = world.resource_mut::<WorldTime>();
        time.multiplier = 64;
        time.epoch = fixed_epoch() + 600.0;
    }
    leave_sim(&mut app);

    enter_sim(&mut app);

    assert!(app.world().resource::<OrbitalCache>().debris.is_empty());
    assert_close(
        app.world().resource::<CaptureSphereRadius>().radius,
        INITIAL_CAPTURE_SPHERE_RADIUS_M,
        0.0,
        "capture sphere radius",
    );
    let time = app.world().resource::<WorldTime>();
    assert_eq!(time.epoch, fixed_epoch());
    assert_eq!(time.multiplier, 1);
}

// ---------------------------------------------------------------------------
// telemetry: collect_log_events in the plugin set
// ---------------------------------------------------------------------------

#[test]
fn entering_the_sim_logs_the_start_epoch_with_an_hh_mm_ss_timestamp() {
    let mut app = test_app();
    assert!(app.world().resource::<CaptureLog>().entries.is_empty());

    enter_sim(&mut app);

    // The dataset load logs a line too; pick the clock's.
    let log = app.world().resource::<CaptureLog>();
    let mut from_time = log.entries.iter().filter(|e| e.source == "time");
    let entry = from_time.next().expect("no log line from the clock");
    assert!(from_time.next().is_none(), "exactly one: {:?}", log_messages(&app));
    assert_eq!(entry.level, LogLevel::Info);
    assert_eq!(entry.source, "time");
    assert_eq!(entry.message, format!("Sim start epoch: {}", fixed_epoch()));
    assert_eq!(
        entry.timestamp, "16:03:47",
        "the time of day of the fixed epoch (2026-04-03 16:03:47 UTC)"
    );
}

#[test]
fn the_log_timestamp_follows_the_sim_clock() {
    let mut app = test_app();
    enter_sim(&mut app);
    app.world_mut().resource_mut::<WorldTime>().epoch = fixed_epoch() + 3723.0;

    send(&mut app, LogLevel::Warn, "test", "an hour later");
    app.update();

    let log = app.world().resource::<CaptureLog>();
    let entry = log.entries.back().unwrap();
    assert_eq!(entry.message, "an hour later");
    assert_eq!(entry.level, LogLevel::Warn);
    assert_eq!(entry.timestamp, "17:05:50", "16:03:47 + 1 h 2 min 3 s");
}

#[test]
fn events_are_logged_in_order_and_each_only_once() {
    let mut app = test_app();
    enter_sim(&mut app);
    app.world_mut().resource_mut::<CaptureLog>().clear();

    send(&mut app, LogLevel::Info, "a", "first");
    send(&mut app, LogLevel::Error, "b", "second");
    app.update();
    app.update();
    app.update();

    let log = app.world().resource::<CaptureLog>();
    let entries: Vec<(LogLevel, &str, &str)> = log
        .entries
        .iter()
        .map(|e| (e.level, e.source, e.message.as_str()))
        .collect();
    assert_eq!(
        entries,
        [(LogLevel::Info, "a", "first"), (LogLevel::Error, "b", "second")]
    );
}

#[test]
fn events_sent_outside_the_sim_screen_are_not_logged() {
    let mut app = test_app();
    send(&mut app, LogLevel::Info, "menu", "from the home screen");
    app.update();
    app.update();
    app.update();

    enter_sim(&mut app);

    let messages = log_messages(&app);
    assert!(
        messages.contains(&format!("Sim start epoch: {}", fixed_epoch())),
        "precondition: the sim's own lines are logged: {messages:?}"
    );
    assert!(
        !messages.iter().any(|m| m == "from the home screen"),
        "the menu event never reaches the log, not even after the sim starts: {messages:?}"
    );
}

#[test]
fn an_event_is_logged_once_and_stays_readable_by_other_readers() {
    // The plugin's reader does not consume the event: a second reader sees it too.
    let mut app = test_app();
    enter_sim(&mut app);
    let mut tap = LogTap::new(app.world());

    send(&mut app, LogLevel::Debug, "x", "seen twice, logged once");
    app.update();

    let tapped = tap.drain(app.world());
    assert_eq!(
        tapped,
        [(LogLevel::Debug, "x", "seen twice, logged once".to_string())]
    );
    assert_eq!(
        log_messages(&app)
            .iter()
            .filter(|m| m.as_str() == "seen twice, logged once")
            .count(),
        1
    );
}

#[test]
fn data_collection_starts_empty() {
    let app = test_app();

    let data = app.world().resource::<DataCollection>();
    assert!(data.velocity.is_empty());
    assert!(data.position.is_empty());
}
