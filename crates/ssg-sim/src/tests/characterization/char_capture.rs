//! Characterization of the capture state machine (`capture_state_machine_update`).
//!
//! Every test scripts `(range, relative speed)` rows through the rig from
//! `test_support` and compares the logged transitions with the timeline that the
//! P0 oracle (`plans/stage-0/52-p0-tools/compare_runs.py`) computes for the same
//! rows. No avian stepping happens between rows, so the scripted values are exact.
//!
//! F-24 (conditions are OR-ed, transitions cascade within one step, stale states
//! are not revisited in a step) is pinned here as it is today; P4 (Q14) owns any
//! change.

use crate::capture::CaptureSphereRadius;
use crate::capture::plan::CapturePlanLibrary;
use crate::test_support::{
    CaptureRig, assert_close, capture_rig, fixed_epoch, insert_plan_json, load_example_plans,
    rig_set, rig_step, test_app,
};
use crate::time::{PHYS_DT, WorldTime};
use bevy::prelude::*;

use crate::capture::CaptureComponent;
use crate::capture::plan::build_capture_component;

// Oracle plans (the `compare_runs.py --selftest` plans, with the `name`/`tether`
// fields the app requires added).
const CHAIN_PLAN: &str = r#"{"name": "selftest", "tether": "T", "states": [
  {"id": "a", "parameters": {"max_velocity": 1.0, "max_force": 1.0},
   "transitions": [{"to": "b", "distance": {"less_than": 1000.0}}]},
  {"id": "b", "parameters": {"max_velocity": 1.0, "max_force": 1.0},
   "transitions": [{"to": "c", "distance": {"less_than": 500.0}}]},
  {"id": "c", "parameters": {"max_velocity": 1.0, "max_force": 1.0},
   "transitions": [{"to": "d", "distance": {"less_than": 50.0}},
                   {"to": "b", "distance": {"greater_than": 60.0}}]},
  {"id": "d", "parameters": {"max_velocity": 1.0, "max_force": 1.0},
   "transitions": [{"to": "e", "distance": {"less_than": 1.0, "units": "m"},
                    "time_in_state": {"greater_than": 5.0}},
                   {"to": "e", "relative_velocity": {"less_than": 0.1}}]},
  {"id": "e", "parameters": {"max_velocity": 1.0, "max_force": 1.0}}
]}"#;

const OVERWRITE_PLAN: &str = r#"{"name": "selftest2", "tether": "T", "states": [
  {"id": "x", "parameters": {"max_velocity": 1.0, "max_force": 1.0},
   "transitions": [{"to": "y", "distance": {"less_than": 10.0, "greater_than": 1.0}},
                   {"to": "z", "distance": {"less_than": 10.0}}]},
  {"id": "y", "parameters": {"max_velocity": 1.0, "max_force": 1.0}},
  {"id": "z", "parameters": {"max_velocity": 1.0, "max_force": 1.0}}
]}"#;

const UNKNOWN_TARGET_PLAN: &str = r#"{"name": "selftest3", "tether": "T", "states": [
  {"id": "p", "parameters": {"max_velocity": 1.0, "max_force": 1.0},
   "transitions": [{"to": "nowhere", "distance": {"less_than": 5.0}}]}
]}"#;

const NO_TO_PLAN: &str = r#"{"name": "selftest4", "tether": "T", "states": [
  {"id": "q", "parameters": {"max_velocity": 1.0, "max_force": 1.0},
   "transitions": [{"distance": {"less_than": 50.0}}]}
]}"#;

// Copied from plans/stage-0/52-p0-fixtures/plans/q10_and_condition.json.
const Q10_PLAN: &str = r#"{"name": "Q10 And Condition", "tether": "Tether1", "device": {"type": "tether", "tether_length": 20.0},
 "states": [
  {"id": "far", "parameters": {"max_velocity": 25.0, "max_force": 25.0},
   "transitions": [{"to": "near", "distance": {"less_than": 2000.0}, "relative_velocity": {"less_than": 5.0}}]},
  {"id": "near", "parameters": {"max_velocity": 2.0, "max_force": 3.0}}
]}"#;

/// A harness app with the example plans loaded.
fn example_app() -> App {
    let mut app = test_app();
    load_example_plans(&mut app);
    app
}

/// A harness app with one inline plan inserted (without validation).
fn inline_app(plan_id: &str, json: &str) -> App {
    let mut app = test_app();
    insert_plan_json(&mut app, plan_id, json);
    app
}

/// One expected transition: `(row, from, to, reason)`.
type Expected = (usize, &'static str, &'static str, &'static str);

/// Feed `rows` through the rig and return every logged transition as
/// `(row, from, to, reason)`.
fn play(
    app: &mut App,
    rig: &CaptureRig,
    rows: &[(f64, f64)],
) -> Vec<(usize, String, String, String)> {
    let mut events = Vec::new();
    for (row, &(range, speed)) in rows.iter().enumerate() {
        rig_set(app, rig, range, speed);
        for (from, to, reason) in rig_step(app, rig) {
            events.push((row, from, to, reason));
        }
    }
    events
}

#[track_caller]
fn assert_timeline(actual: &[(usize, String, String, String)], expected: &[Expected]) {
    let expected: Vec<(usize, String, String, String)> = expected
        .iter()
        .map(|&(row, from, to, reason)| (row, from.into(), to.into(), reason.into()))
        .collect();
    assert_eq!(actual, expected.as_slice());
}

fn state_of(app: &App, rig: &CaptureRig) -> String {
    app.world()
        .get::<CaptureComponent>(rig.capture)
        .expect("capture component present")
        .current_state
        .clone()
}

fn radius(app: &App) -> f64 {
    app.world().resource::<CaptureSphereRadius>().radius
}

#[test]
fn char_capture_example_plan_timeline() {
    let mut app = example_app();
    let rig = capture_rig(&mut app, "example_plan");
    assert_eq!(state_of(&app, &rig), "rendezvous");
    assert_eq!(radius(&app), 25.0);

    let rows = [
        (3000.0, 1.0),
        (1500.0, 1.0),
        (40.0, 1.0),
        (4.0, 1.0),
        (4.0, 1.0),
    ];

    // Expected timeline from compare_runs.py (P0 oracle) at c4b8cca
    let expected: [Expected; 3] = [
        (1, "rendezvous", "approach", "distance 1500.0 m < 2000.0 m"),
        (2, "approach", "terminal", "distance 40.0 m < 50.0 m"),
        (3, "terminal", "capture", "distance 4.0 m < 5.0 m"),
    ];

    let mut events = Vec::new();
    let mut radii = Vec::new();
    for (row, &(range, speed)) in rows.iter().enumerate() {
        rig_set(&mut app, &rig, range, speed);
        for (from, to, reason) in rig_step(&mut app, &rig) {
            events.push((row, from, to, reason));
        }
        radii.push(radius(&app));
    }
    // F-24: current behaviour, expected to change in P4
    assert_timeline(&events, &expected);
    assert_eq!(state_of(&app, &rig), "capture");

    // The sphere shrinks only in states with a shrink rate. `approach -> terminal`
    // happens in row 2 and the scan then reaches `terminal` in the same row.
    let terminal_step = 0.01 * PHYS_DT;
    let capture_step = 0.005 * PHYS_DT;
    assert_eq!(radii[0], 25.0);
    assert_eq!(radii[1], 25.0);
    assert_close(radii[2], 25.0 - terminal_step, 1e-12, "radius after row 2");
    // Row 3: terminal shrinks, then `terminal -> capture` and `capture` shrinks.
    assert_close(
        radii[3],
        25.0 - 2.0 * terminal_step - capture_step,
        1e-12,
        "radius after row 3",
    );
    assert_close(
        radii[4],
        25.0 - 2.0 * terminal_step - 2.0 * capture_step,
        1e-12,
        "radius after row 4",
    );
}

#[test]
fn char_capture_cascade_within_step() {
    let mut app = example_app();
    let rig = capture_rig(&mut app, "example_plan");
    assert_eq!(state_of(&app, &rig), "rendezvous");

    // Expected timeline from compare_runs.py (P0 oracle) at c4b8cca
    let expected: [Expected; 3] = [
        (0, "rendezvous", "approach", "distance 4.0 m < 2000.0 m"),
        (0, "approach", "terminal", "distance 4.0 m < 50.0 m"),
        (0, "terminal", "capture", "distance 4.0 m < 5.0 m"),
    ];
    let events = play(&mut app, &rig, &[(4.0, 1.0)]);
    // F-24: current behaviour, expected to change in P4
    assert_timeline(&events, &expected);
    assert_eq!(state_of(&app, &rig), "capture");
    // Both the terminal and the capture shrink run inside that one step.
    assert_close(
        radius(&app),
        25.0 - (0.01 + 0.005) * PHYS_DT,
        1e-12,
        "radius after the cascade",
    );

    // Hysteresis row: fresh rig.
    let mut app = example_app();
    let rig = capture_rig(&mut app, "example_plan");

    // Expected timeline from compare_runs.py (P0 oracle) at c4b8cca
    let expected: [Expected; 3] = [
        (0, "rendezvous", "approach", "distance 40.0 m < 2000.0 m"),
        (0, "approach", "terminal", "distance 40.0 m < 50.0 m"),
        (1, "terminal", "approach", "distance 60.0 m > 50.0 m"),
    ];
    // Row 0 reaches `terminal`; row 1 sends it back to `approach`, which precedes
    // `terminal` in the scan so it is not re-scanned in that step; row 2 has no
    // event.
    let events = play(&mut app, &rig, &[(40.0, 1.0), (60.0, 1.0), (60.0, 1.0)]);
    // F-24: current behaviour, expected to change in P4
    assert_timeline(&events, &expected);
    assert_eq!(state_of(&app, &rig), "approach");
}

#[test]
fn char_capture_or_within_transition() {
    // Expected timeline from compare_runs.py (P0 oracle) at c4b8cca
    // Rows (3000, 1): only the velocity condition holds.
    let mut app = inline_app("q10", Q10_PLAN);
    let rig = capture_rig(&mut app, "q10");
    assert_eq!(state_of(&app, &rig), "far");
    let events = play(&mut app, &rig, &[(3000.0, 1.0)]);
    // F-24: current behaviour, expected to change in P4
    assert_timeline(
        &events,
        &[(0, "far", "near", "rel vel 1.00 m/s < 5.00 m/s")],
    );
    assert_eq!(state_of(&app, &rig), "near");

    // Fresh rig, rows (1000, 10): only the distance condition holds.
    let mut app = inline_app("q10", Q10_PLAN);
    let rig = capture_rig(&mut app, "q10");
    assert_eq!(state_of(&app, &rig), "far");
    let events = play(&mut app, &rig, &[(1000.0, 10.0)]);
    // F-24: current behaviour, expected to change in P4
    assert_timeline(
        &events,
        &[(0, "far", "near", "distance 1000.0 m < 2000.0 m")],
    );
    assert_eq!(state_of(&app, &rig), "near");
}

#[test]
fn char_capture_condition_overwrite() {
    let mut app = inline_app("overwrite", OVERWRITE_PLAN);
    let rig = capture_rig(&mut app, "overwrite");
    assert_eq!(state_of(&app, &rig), "x");

    // Expected timeline from compare_runs.py (P0 oracle) at c4b8cca
    let expected: [Expected; 3] = [
        (0, "x", "y", "distance 5.0 m < 10.0 m"),
        (0, "y", "y", "distance 5.0 m > 1.0 m"),
        (0, "y", "z", "distance 5.0 m < 10.0 m"),
    ];
    let events = play(&mut app, &rig, &[(5.0, 0.0)]);
    // F-24: current behaviour, expected to change in P4
    assert_timeline(&events, &expected);
    assert_eq!(state_of(&app, &rig), "z");
}

#[test]
fn char_capture_earlier_state_not_revisited() {
    // Each case is a fresh rig. They start from the plan's first state `a`; the
    // oracle always starts there, so cases that begin later first drive to that
    // state with an earlier row.

    // Forward cascade a -> b -> c at 55 m.
    let mut app = inline_app("chain", CHAIN_PLAN);
    let rig = capture_rig(&mut app, "chain");
    assert_eq!(state_of(&app, &rig), "a");
    // Expected timeline from compare_runs.py (P0 oracle) at c4b8cca
    let events = play(&mut app, &rig, &[(55.0, 9.0)]);
    // F-24: current behaviour, expected to change in P4
    assert_timeline(
        &events,
        &[
            (0, "a", "b", "distance 55.0 m < 1000.0 m"),
            (0, "b", "c", "distance 55.0 m < 500.0 m"),
        ],
    );
    assert_eq!(state_of(&app, &rig), "c");

    // Full cascade a -> b -> c -> d at 10 m.
    let mut app = inline_app("chain", CHAIN_PLAN);
    let rig = capture_rig(&mut app, "chain");
    assert_eq!(state_of(&app, &rig), "a");
    // Expected timeline from compare_runs.py (P0 oracle) at c4b8cca
    let events = play(&mut app, &rig, &[(10.0, 9.0)]);
    // F-24: current behaviour, expected to change in P4
    assert_timeline(
        &events,
        &[
            (0, "a", "b", "distance 10.0 m < 1000.0 m"),
            (0, "b", "c", "distance 10.0 m < 500.0 m"),
            (0, "c", "d", "distance 10.0 m < 50.0 m"),
        ],
    );
    assert_eq!(state_of(&app, &rig), "d");

    // OR trigger from `d` at (100, 0.05): the velocity condition fires while the
    // distance condition does not.
    let mut app = inline_app("chain", CHAIN_PLAN);
    let rig = capture_rig(&mut app, "chain");
    assert_eq!(state_of(&app, &rig), "a");
    // Expected timeline from compare_runs.py (P0 oracle) at c4b8cca
    let events = play(&mut app, &rig, &[(10.0, 9.0), (100.0, 0.05)]);
    // F-24: current behaviour, expected to change in P4
    assert_timeline(
        &events,
        &[
            (0, "a", "b", "distance 10.0 m < 1000.0 m"),
            (0, "b", "c", "distance 10.0 m < 500.0 m"),
            (0, "c", "d", "distance 10.0 m < 50.0 m"),
            (1, "d", "e", "rel vel 0.05 m/s < 0.10 m/s"),
        ],
    );
    assert_eq!(state_of(&app, &rig), "e");

    // Hysteresis c -> b at 70 m: `b` precedes `c` so it is not re-scanned in that
    // step (row 1). On the next row (row 2) `b` fires -> c, then `c` fires -> b.
    let mut app = inline_app("chain", CHAIN_PLAN);
    let rig = capture_rig(&mut app, "chain");
    assert_eq!(state_of(&app, &rig), "a");
    // Expected timeline from compare_runs.py (P0 oracle) at c4b8cca
    let events = play(&mut app, &rig, &[(55.0, 9.0), (70.0, 9.0), (70.0, 9.0)]);
    // F-24: current behaviour, expected to change in P4
    assert_timeline(
        &events,
        &[
            (0, "a", "b", "distance 55.0 m < 1000.0 m"),
            (0, "b", "c", "distance 55.0 m < 500.0 m"),
            (1, "c", "b", "distance 70.0 m > 60.0 m"),
            (2, "b", "c", "distance 70.0 m < 500.0 m"),
            (2, "c", "b", "distance 70.0 m > 60.0 m"),
        ],
    );
    assert_eq!(state_of(&app, &rig), "b");
}

#[test]
fn char_capture_unknown_target_sticks() {
    // Validation would reject this plan, so it goes in without it.
    let mut app = inline_app("unknown", UNKNOWN_TARGET_PLAN);
    let rig = capture_rig(&mut app, "unknown");
    assert_eq!(state_of(&app, &rig), "p");

    // Expected timeline from compare_runs.py (P0 oracle) at c4b8cca
    let events = play(&mut app, &rig, &[(1.0, 0.0), (1.0, 0.0), (1.0, 0.0)]);
    // F-32: current behaviour, expected to change in P4
    assert_timeline(&events, &[(0, "p", "nowhere", "distance 1.0 m < 5.0 m")]);
    // F-32: current behaviour, expected to change in P4
    assert_eq!(state_of(&app, &rig), "nowhere");
}

#[test]
fn char_capture_transition_without_to_skipped() {
    let mut app = inline_app("noto", NO_TO_PLAN);
    let rig = capture_rig(&mut app, "noto");
    assert_eq!(state_of(&app, &rig), "q");

    // Expected timeline from compare_runs.py (P0 oracle) at c4b8cca: no transitions.
    let events = play(&mut app, &rig, &[(1.0, 0.0)]);
    // F-32: current behaviour, expected to change in P4
    assert_timeline(&events, &[]);
    // F-32: current behaviour, expected to change in P4
    assert_eq!(state_of(&app, &rig), "q");
}

#[test]
fn char_capture_elapsed_time_resets() {
    let mut app = example_app();
    let rig = capture_rig(&mut app, "example_plan");

    // `capture_rig` builds the component with `state_enter_time_s = 0.0`; rebuild
    // it with a non-zero entry time to see the value that is passed in.
    let plan = app
        .world()
        .resource::<CapturePlanLibrary>()
        .plans
        .get("example_plan")
        .unwrap()
        .clone();
    let component = build_capture_component("example_plan", &plan, 12.5).unwrap();
    app.world_mut().entity_mut(rig.capture).insert(component);

    let timers = |app: &App| {
        let c = app.world().get::<CaptureComponent>(rig.capture).unwrap();
        (c.state_elapsed_time_s, c.state_enter_time_s)
    };
    assert_eq!(timers(&app), (0.0, 12.5));

    // No transition at 3000 m: elapsed time grows by one fixed step per call and
    // the entry time is untouched.
    for n in 1..=3 {
        rig_set(&mut app, &rig, 3000.0, 1.0);
        assert!(rig_step(&mut app, &rig).is_empty());
        let (elapsed, enter) = timers(&app);
        assert_close(
            elapsed,
            n as f64 * PHYS_DT,
            1e-12,
            "elapsed after a quiet step",
        );
        assert_eq!(enter, 12.5);
    }

    // A transition resets the elapsed time to 0 (after this step's increment) and
    // sets the entry time to the sim time of the transition. The rig never ticks,
    // so move the sim clock 40 s past the start (not 12.5 s, the entry time above)
    // to see the value is set.
    app.world_mut().resource_mut::<WorldTime>().epoch = fixed_epoch() + 40.0;
    rig_set(&mut app, &rig, 1500.0, 1.0);
    assert_eq!(rig_step(&mut app, &rig).len(), 1);
    // F-11: fixed in P4 (`state_enter_time_s` is sim seconds since the start)
    assert_eq!(timers(&app), (0.0, 40.0));

    // The next quiet step counts again from the reset.
    rig_set(&mut app, &rig, 1500.0, 1.0);
    assert!(rig_step(&mut app, &rig).is_empty());
    assert_close(timers(&app).0, PHYS_DT, 1e-12, "elapsed after the reset");
}
