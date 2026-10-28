//! Characterization of the capture state machine (`capture_state_machine_update`).
//!
//! Every test scripts `(range, relative speed)` rows through the rig from
//! `test_support` and compares the logged transitions with a timeline derived by
//! hand from the Q14 semantics: all conditions of a transition must hold, the
//! current state's transitions are checked in plan order, the first satisfied one
//! fires, and at most one transition fires per step (Q14, fixed in P4). No avian
//! stepping happens between rows, so the scripted values are exact.

use crate::capture::CaptureSphereRadius;
use crate::capture::plan::CapturePlanLibrary;
use crate::test_support::{
    CaptureRig, assert_close, capture_rig, fixed_epoch, insert_plan_json, load_example_plans,
    rig_set, rig_step, test_app,
};
use crate::time::{PHYS_DT, WorldTime};
use bevy::prelude::*;

use crate::capture::plan::build_capture_component;
use crate::capture::{
    CaptureComponent, INITIAL_CAPTURE_SPHERE_RADIUS_M, SimCommand, apply_sim_commands,
};
use crate::orbit::ISS_ORBIT;
use crate::orbit::cache::OrbitalCache;
use crate::orbit::lvlh::LvlhFrame;
use crate::telemetry::DataCollection;
use crate::test_support::kepler_eci;
use avian3d::prelude::{Forces, LinearVelocity, Position, ReadRigidBodyForces};
use bevy::ecs::system::RunSystemOnce;
use bevy::math::DVec3;

// Plans from the P0 oracle's self-test (`compare_runs.py --selftest`, with the
// `name`/`tether` fields the app requires added). The oracle itself encodes the
// pre-Q14 semantics and is historical.
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
fn capture_example_plan_timeline() {
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

    // Each row satisfies exactly one transition, so the timeline is one transition
    // per row from row 1.
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
    // Q14: fixed in P4
    assert_timeline(&events, &expected);
    assert_eq!(state_of(&app, &rig), "capture");

    // One shrink per row, at the rate of the state current after the row's check:
    // `terminal` from row 2 (the row it is entered), `capture` from row 3.
    let terminal_step = 0.01 * PHYS_DT;
    let capture_step = 0.005 * PHYS_DT;
    assert_eq!(radii[0], 25.0);
    assert_eq!(radii[1], 25.0);
    assert_close(radii[2], 25.0 - terminal_step, 1e-12, "radius after row 2");
    assert_close(
        radii[3],
        25.0 - terminal_step - capture_step,
        1e-12,
        "radius after row 3",
    );
    assert_close(
        radii[4],
        25.0 - terminal_step - 2.0 * capture_step,
        1e-12,
        "radius after row 4",
    );
}

#[test]
fn transition_requires_all_conditions() {
    // `far -> near` needs distance < 2000 m AND relative velocity < 5 m/s.
    let mut app = inline_app("q10", Q10_PLAN);
    let rig = capture_rig(&mut app, "q10");
    assert_eq!(state_of(&app, &rig), "far");

    // Row 0: only the velocity holds; row 1: only the distance holds; row 2: both.
    let events = play(
        &mut app,
        &rig,
        &[(3000.0, 1.0), (1000.0, 10.0), (1000.0, 1.0)],
    );
    // Q14: fixed in P4
    assert_timeline(
        &events,
        &[(
            2,
            "far",
            "near",
            "distance 1000.0 m < 2000.0 m, rel vel 1.00 m/s < 5.00 m/s",
        )],
    );
    assert_eq!(state_of(&app, &rig), "near");
}

#[test]
fn transition_one_per_step() {
    // At 10 m every chain state's first transition holds; one fires per row.
    let mut app = inline_app("chain", CHAIN_PLAN);
    let rig = capture_rig(&mut app, "chain");
    assert_eq!(state_of(&app, &rig), "a");

    let events = play(&mut app, &rig, &[(10.0, 9.0), (10.0, 9.0), (10.0, 9.0)]);
    // Q14: fixed in P4
    assert_timeline(
        &events,
        &[
            (0, "a", "b", "distance 10.0 m < 1000.0 m"),
            (1, "b", "c", "distance 10.0 m < 500.0 m"),
            (2, "c", "d", "distance 10.0 m < 50.0 m"),
        ],
    );
    assert_eq!(state_of(&app, &rig), "d");
}

#[test]
fn transition_no_cascade() {
    // At 4 m every example-plan transition towards `capture` holds; one fires per
    // row, and the sphere shrinks only from the row `terminal` is entered.
    let mut app = example_app();
    let rig = capture_rig(&mut app, "example_plan");
    assert_eq!(state_of(&app, &rig), "rendezvous");

    let mut events = Vec::new();
    let mut radii = Vec::new();
    for row in 0..3 {
        rig_set(&mut app, &rig, 4.0, 1.0);
        for (from, to, reason) in rig_step(&mut app, &rig) {
            events.push((row, from, to, reason));
        }
        radii.push(radius(&app));
    }
    // Q14: fixed in P4
    assert_timeline(
        &events,
        &[
            (0, "rendezvous", "approach", "distance 4.0 m < 2000.0 m"),
            (1, "approach", "terminal", "distance 4.0 m < 50.0 m"),
            (2, "terminal", "capture", "distance 4.0 m < 5.0 m"),
        ],
    );
    assert_eq!(state_of(&app, &rig), "capture");

    let terminal_step = 0.01 * PHYS_DT;
    let capture_step = 0.005 * PHYS_DT;
    assert_eq!(radii[0], 25.0);
    assert_close(radii[1], 25.0 - terminal_step, 1e-12, "radius after row 1");
    assert_close(
        radii[2],
        25.0 - terminal_step - capture_step,
        1e-12,
        "radius after row 2",
    );
}

#[test]
fn transition_first_satisfied_wins() {
    // At 5 m both of `x`'s transitions hold; the first in plan order fires, and `y`
    // has no transitions, so it stays.
    let mut app = inline_app("overwrite", OVERWRITE_PLAN);
    let rig = capture_rig(&mut app, "overwrite");
    assert_eq!(state_of(&app, &rig), "x");

    let events = play(&mut app, &rig, &[(5.0, 0.0), (5.0, 0.0)]);
    // Q14: fixed in P4
    assert_timeline(
        &events,
        &[(
            0,
            "x",
            "y",
            "distance 5.0 m < 10.0 m, distance 5.0 m > 1.0 m",
        )],
    );
    assert_eq!(state_of(&app, &rig), "y");
}

#[test]
fn transition_hysteresis_next_step() {
    // `c -> b` above 60 m and `b -> c` below 500 m: at 70 m the pair alternates one
    // transition per row.
    let mut app = inline_app("chain", CHAIN_PLAN);
    let rig = capture_rig(&mut app, "chain");
    assert_eq!(state_of(&app, &rig), "a");

    let events = play(
        &mut app,
        &rig,
        &[
            (55.0, 9.0),
            (55.0, 9.0),
            (70.0, 9.0),
            (70.0, 9.0),
            (70.0, 9.0),
        ],
    );
    // Q14: fixed in P4
    assert_timeline(
        &events,
        &[
            (0, "a", "b", "distance 55.0 m < 1000.0 m"),
            (1, "b", "c", "distance 55.0 m < 500.0 m"),
            (2, "c", "b", "distance 70.0 m > 60.0 m"),
            (3, "b", "c", "distance 70.0 m < 500.0 m"),
            (4, "c", "b", "distance 70.0 m > 60.0 m"),
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
    // F-32: current behaviour, expected to change in P6
    assert_timeline(&events, &[(0, "p", "nowhere", "distance 1.0 m < 5.0 m")]);
    // F-32: current behaviour, expected to change in P6
    assert_eq!(state_of(&app, &rig), "nowhere");
}

#[test]
fn char_capture_transition_without_to_skipped() {
    let mut app = inline_app("noto", NO_TO_PLAN);
    let rig = capture_rig(&mut app, "noto");
    assert_eq!(state_of(&app, &rig), "q");

    // Expected timeline from compare_runs.py (P0 oracle) at c4b8cca: no transitions.
    let events = play(&mut app, &rig, &[(1.0, 0.0)]);
    // F-32: current behaviour, expected to change in P6
    assert_timeline(&events, &[]);
    // F-32: current behaviour, expected to change in P6
    assert_eq!(state_of(&app, &rig), "q");
}

#[test]
fn capture_elapsed_time_resets() {
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

/// The capture component `capture_rig` attaches, for `plan_id`.
fn component_for(app: &App, plan_id: &str) -> CaptureComponent {
    let plan = app
        .world()
        .resource::<CapturePlanLibrary>()
        .plans
        .get(plan_id)
        .unwrap()
        .clone();
    build_capture_component(plan_id, &plan, 0.0).unwrap()
}

#[test]
fn capture_missing_rigid_body_continues() {
    #[derive(Component)]
    struct PadA;
    #[derive(Component)]
    struct PadB;
    #[derive(Component)]
    struct PadC;

    let mut app = example_app();
    // The controller's query visits archetypes in Bevy's component-index order (a
    // fixed-hasher map keyed by archetype id), not in spawn order. Five padding
    // archetypes before the rig give a layout where M is visited first; the
    // precondition below checks it.
    for combo in 1..=5u32 {
        let mut pad = app.world_mut().spawn_empty();
        if combo & 1 != 0 {
            pad.insert(PadA);
        }
        if combo & 2 != 0 {
            pad.insert(PadB);
        }
        if combo & 4 != 0 {
            pad.insert(PadC);
        }
    }
    let rig = capture_rig(&mut app, "example_plan");
    // M: a capture entity without a rigid body.
    let component = component_for(&app, "example_plan");
    let m = app.world_mut().spawn(component).id();
    let order: Vec<Entity> = app
        .world_mut()
        .query::<(Entity, &mut CaptureComponent)>()
        .iter(app.world())
        .map(|(entity, _)| entity)
        .collect();
    assert_eq!(order, vec![m, rig.capture], "M must be visited first");

    // F-39: fixed in P4. M is skipped; the rig's entity still transitions.
    let events = play(&mut app, &rig, &[(1500.0, 1.0)]);
    assert_timeline(
        &events,
        &[(0, "rendezvous", "approach", "distance 1500.0 m < 2000.0 m")],
    );
    assert_eq!(state_of(&app, &rig), "approach");
}

#[test]
fn capture_zero_vector_force_is_zero() {
    let mut app = example_app();
    let rig = capture_rig(&mut app, "example_plan");
    // The capture entity on top of both nodes, at rest relative to them: inside 80 %
    // of the radius, `rel_r == 0` and `rel_v == 0`, so the controller's direction is
    // the zero vector.
    for entity in [rig.capture, rig.root, rig.tail] {
        app.world_mut()
            .entity_mut(entity)
            .insert((Position(DVec3::ZERO), LinearVelocity::ZERO));
    }
    rig_step(&mut app, &rig);

    // F-11: fixed in P4 (was `normalize()`: NaN)
    let accelerations = app
        .world_mut()
        .run_system_once(move |forces: Query<Forces>| {
            [rig.root, rig.tail].map(|node| {
                forces
                    .get(node)
                    .expect("tether node has Forces")
                    .accumulated_linear_acceleration()
            })
        })
        .unwrap();
    for acceleration in accelerations {
        assert!(acceleration.is_finite(), "acceleration {acceleration:?}");
        assert_eq!(acceleration, DVec3::ZERO);
    }
}

#[test]
fn capture_sphere_radius_reset_per_capture() {
    let mut app = example_app();
    let rig = capture_rig(&mut app, "example_plan");
    assert_eq!(radius(&app), INITIAL_CAPTURE_SPHERE_RADIUS_M);

    // The first capture shrank the radius.
    app.world_mut().resource_mut::<CaptureSphereRadius>().radius = 7.0;

    // A second capture, for a new entity, starts from the initial radius again.
    let second = app.world_mut().spawn_empty().id();
    app.world_mut().write_message(SimCommand::StartCapture {
        target: Some(second),
        plan_id: "example_plan".to_string(),
    });
    app.world_mut()
        .run_system_once(apply_sim_commands)
        .expect("apply_sim_commands failed to run");

    // F-11: fixed in P4
    assert_eq!(radius(&app), INITIAL_CAPTURE_SPHERE_RADIUS_M);
    assert!(app.world().get::<CaptureComponent>(second).is_some());
    assert!(app.world().get::<CaptureComponent>(rig.capture).is_none());
}

/// The rig in `approach` (max velocity 2.0 m/s) with the capture target at rest
/// at the bubble origin on the ISS orbit, and both tether nodes 3,536 m
/// along-track of it moving at `node_velocity(frame, offset)`. Runs the controller
/// once; returns the logged relative speed, the root's applied acceleration and
/// the root's offset from the target.
fn lvlh_rig(node_velocity: fn(&LvlhFrame, DVec3) -> DVec3) -> (f64, DVec3, DVec3) {
    let mut app = example_app();
    let rig = capture_rig(&mut app, "example_plan");
    let target_state = kepler_eci(ISS_ORBIT, fixed_epoch());
    let frame = LvlhFrame::from_eci(&target_state).unwrap();
    let offset = frame.along * 3_536.0;
    let world = app.world_mut();
    world
        .resource_mut::<OrbitalCache>()
        .eci_states
        .insert(rig.capture, target_state);
    world
        .resource_mut::<DataCollection>()
        .velocity
        .insert(rig.capture, Vec::new());
    world
        .entity_mut(rig.capture)
        .insert((Position(DVec3::ZERO), LinearVelocity::ZERO));
    world
        .get_mut::<CaptureComponent>(rig.capture)
        .unwrap()
        .current_state = "approach".to_string();
    for node in [rig.root, rig.tail] {
        world.entity_mut(node).insert((
            Position(offset),
            LinearVelocity(node_velocity(&frame, offset)),
        ));
    }

    assert!(
        rig_step(&mut app, &rig).is_empty(),
        "no transition at 3,536 m"
    );
    let speed = app.world().resource::<DataCollection>().velocity[&rig.capture][0].1;
    let acceleration = app
        .world_mut()
        .run_system_once(move |forces: Query<Forces>| {
            forces
                .get(rig.root)
                .expect("root has Forces")
                .accumulated_linear_acceleration()
        })
        .unwrap();
    (speed, acceleration, offset)
}

#[test]
fn capture_uses_lvlh_relative_velocity() {
    // F-35: fixed in P4. Nodes at rest in the target's LVLH frame (co-rotating,
    // `v = omega × r`): zero relative speed, so no velocity-kill term; the force
    // points straight at the target.
    let (speed, acceleration, offset) = lvlh_rig(|frame, offset| frame.omega.cross(offset));
    assert!(speed < 1e-9, "LVLH rest: relative speed {speed} m/s");
    let to_target = -offset.normalize();
    assert!(
        acceleration.normalize().dot(to_target) > 1.0 - 1e-12,
        "LVLH rest: acceleration {acceleration:?} is not toward the target"
    );

    // ECI-equal velocities: the nodes move relative to the LVLH frame at
    // |omega × r| ≈ 4 m/s > 2.0 m/s, so the velocity-kill term fires.
    let (speed, acceleration, offset) = lvlh_rig(|_, _| DVec3::ZERO);
    let frame = LvlhFrame::from_eci(&kepler_eci(ISS_ORBIT, fixed_epoch())).unwrap();
    let expected = frame.omega.cross(offset).length();
    assert!(expected > 3.5, "precondition: ECI artefact {expected} m/s");
    assert_close(speed, expected, 1e-9, "ECI-equal: relative speed");
    assert!(
        acceleration.normalize().dot(-offset.normalize()) < 0.9,
        "ECI-equal: acceleration {acceleration:?} has no velocity-kill component"
    );
}
