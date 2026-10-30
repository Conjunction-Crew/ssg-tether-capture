//! The end event (P7 S2, Q10 row 7, E4): entering a phase with `end` writes one
//! `CaptureEnded` and one log line; nothing else changes.

use bevy::ecs::message::Messages;
use bevy::prelude::*;

use crate::capture::CaptureEnded;
use crate::capture::plan::{CapturePlanLibrary, PhaseEnd};
use crate::test_support::{
    LogTap, current_step, insert_plan_json, load_example_plans, q13, test_app, tick,
};
use crate::time::PHYS_DT;

/// Two phases; the transition to the `end: abort` phase fires at the first
/// controller step (any range is above 0 m).
pub const SHORT_PLAN: &str = r#"{"schema_version": 1, "name": "short", "tether": "Tether1",
  "phases": [
    {"id": "go", "max_velocity_mps": 1.0, "max_force_n": 1.0,
     "transitions": [{"to": "stop", "all_of": {"distance_m": {"greater_than": 0.0}}}]},
    {"id": "stop", "max_velocity_mps": 1.0, "max_force_n": 1.0, "end": "abort"}
  ]}"#;

/// One phase, an end phase: the capture ends at its start.
const ENDS_AT_START: &str = r#"{"schema_version": 1, "name": "at_start", "tether": "Tether1",
  "phases": [{"id": "done", "max_velocity_mps": 1.0, "max_force_n": 1.0, "end": "success"}]}"#;

/// Runs `plan` on the Q13 pair for `ticks` ticks after the start and returns
/// every `CaptureEnded` as `(step, phase, outcome)` and the end log lines.
fn ended_events(
    plan_id: &str,
    json: &str,
    ticks: u64,
) -> (Vec<(u64, String, PhaseEnd)>, Vec<String>) {
    let mut app = test_app();
    load_example_plans(&mut app);
    insert_plan_json(&mut app, plan_id, json);
    let mut cursor = app
        .world()
        .resource::<Messages<CaptureEnded>>()
        .get_cursor_current();
    let mut tap = LogTap::new(app.world());
    let mut events = Vec::new();
    let mut lines = Vec::new();
    let mut collect = |app: &App, events: &mut Vec<_>, lines: &mut Vec<String>| {
        let start = app.world().resource::<crate::time::WorldTime>().start_epoch;
        for e in cursor.read(app.world().resource::<Messages<CaptureEnded>>()) {
            let step = ((e.epoch - start) / PHYS_DT).round() as u64;
            events.push((step, e.phase.clone(), e.outcome));
        }
        lines.extend(
            tap.drain(app.world())
                .into_iter()
                .filter(|(_, _, m)| m.starts_with("Capture ended"))
                .map(|(_, _, m)| m),
        );
    };
    // `build_with_command` runs its updates one by one; messages live two updates,
    // so collecting after it misses nothing.
    q13::build_with_command(&mut app, plan_id);
    collect(&app, &mut events, &mut lines);
    for _ in 0..ticks {
        tick(&mut app);
        collect(&app, &mut events, &mut lines);
    }
    assert_eq!(current_step(&app), 3 + ticks);
    (events, lines)
}

#[test]
fn entering_an_end_phase_writes_one_capture_ended() {
    let (events, lines) = ended_events("short", SHORT_PLAN, 64);
    // The capture starts after step 2; the controller's first step is 3.
    println!("short plan: CaptureEnded {events:?}; log {lines:?}");
    assert_eq!(events, vec![(3, "stop".to_string(), PhaseEnd::Abort)]);
    assert_eq!(lines, vec!["Capture ended: abort (phase stop)".to_string()]);
}

#[test]
fn a_first_phase_with_end_ends_at_the_capture_start() {
    let (events, lines) = ended_events("at_start", ENDS_AT_START, 8);
    assert_eq!(events, vec![(2, "done".to_string(), PhaseEnd::Success)]);
    assert_eq!(
        lines,
        vec!["Capture ended: success (phase done)".to_string()]
    );
}

#[test]
fn the_example_plan_compiles_its_end_phase() {
    let mut app = test_app();
    load_example_plans(&mut app);
    let library = app.world().resource::<CapturePlanLibrary>();
    let plan = &library.compiled_plans[q13::PLAN_ID];
    let ends: Vec<(&str, Option<PhaseEnd>)> =
        plan.states.iter().map(|s| (s.id.as_str(), s.end)).collect();
    assert_eq!(
        ends,
        vec![
            ("rendezvous", None),
            ("approach", None),
            ("terminal", None),
            ("capture", Some(PhaseEnd::Success)),
        ]
    );
}
