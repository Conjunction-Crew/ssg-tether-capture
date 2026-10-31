//! Branch tests for `capture_state_machine_update` and `assert_finite_state` (P5 S4).
//!
//! The characterization tests pin the state machine (transitions, timers, radius).
//! These pin the controller's force branches: for each branch the **direction** of
//! the force on the tether nodes and its magnitude (the ramp's first step), and the
//! log events of the paths where the controller gives up; then the P7.5 force law
//! (F-31, C2, C7): who is pushed, the ramp, the dead bands. Geometry: the nodes sit
//! at the origin and the capture target at `(0, r, 0)`, so `rel_r` (node to
//! target) points along +Y and the default `up` (the target's +X axis) is
//! perpendicular to it.

use avian3d::prelude::{Forces, LinearVelocity, Mass, Position, ReadRigidBodyForces, Rotation};
use bevy::ecs::system::RunSystemOnce;
use bevy::math::{DQuat, DVec3};
use bevy::prelude::*;

use crate::capture::plan::CapturePlanLibrary;
use crate::capture::{
    BRANCH_DEAD_BAND, CaptureComponent, CaptureForce, CaptureSphereRadius, FORCE_RAMP_S,
};
use crate::orbit::cache::OrbitalCache;
use crate::telemetry::LogLevel;
use crate::test_support::{
    CaptureRig, LogTap, assert_close, assert_close_dvec3, capture_rig, insert_plan_json,
    load_example_plans, rig_set, rig_step, test_app,
};
use crate::time::PHYS_DT;

const MAX_FORCE: f64 = 10.0;
/// The force bound on the first step of a capture or a phase: one step of the
/// ramp (F-31).
const FIRST_STEP_FORCE: f64 = MAX_FORCE * PHYS_DT / FORCE_RAMP_S;
const PLAN_ID: &str = "branches";

/// Two phases with the same limits (1.0 m/s, 10 N), no transitions and the tail
/// pushed in both (`tail_actuation`), so the state never changes under the
/// scripted geometry and every branch test drives both ends.
const BRANCH_PLAN: &str = r#"{"schema_version": 1, "name": "branches", "tether": "T", "phases": [
  {"id": "s", "max_velocity_mps": 1.0, "max_force_n": 10.0, "tail_actuation": true},
  {"id": "capture", "max_velocity_mps": 1.0, "max_force_n": 10.0, "tail_actuation": true}
]}"#;

/// A harness app with [`BRANCH_PLAN`] and the rig attached to it.
fn branch_rig() -> (App, CaptureRig) {
    let mut app = test_app();
    insert_plan_json(&mut app, PLAN_ID, BRANCH_PLAN);
    let rig = capture_rig(&mut app, PLAN_ID);
    (app, rig)
}

/// Capture target at `target`, both nodes at the origin moving at `node_velocity`
/// (the target is at rest, so that is also the nodes' relative velocity).
fn place(app: &mut App, rig: &CaptureRig, target: DVec3, node_velocity: DVec3) {
    let world = app.world_mut();
    world
        .entity_mut(rig.capture)
        .insert((Position(target), LinearVelocity::ZERO));
    for node in [rig.root, rig.tail] {
        world
            .entity_mut(node)
            .insert((Position(DVec3::ZERO), LinearVelocity(node_velocity)));
    }
}

fn set_state(app: &mut App, rig: &CaptureRig, state: &str) {
    app.world_mut()
        .get_mut::<CaptureComponent>(rig.capture)
        .expect("capture component present")
        .current_state = state.to_string();
}

/// The force the controller applied to `node` in its last run: the accumulated
/// acceleration times the node's mass.
fn applied_force(app: &mut App, node: Entity) -> DVec3 {
    let mass = app.world().get::<Mass>(node).expect("node has a Mass").0;
    let acceleration = app
        .world_mut()
        .run_system_once(move |forces: Query<Forces>| {
            forces
                .get(node)
                .expect("tether node has Forces")
                .accumulated_linear_acceleration()
        })
        .expect("force query failed to run");
    acceleration * f64::from(mass)
}

/// `direction` is not normalised: the force is the first step's bound along it.
#[track_caller]
fn assert_force_along(actual: DVec3, direction: DVec3, what: &str) {
    assert_close_dvec3(actual, direction.normalize() * FIRST_STEP_FORCE, 1e-9, what);
}

/// The controller's state on `node` (what it applied this step, the ramp, the
/// branch states).
fn capture_force(app: &App, node: Entity) -> CaptureForce {
    *app.world()
        .get::<CaptureForce>(node)
        .expect("node has a CaptureForce")
}

/// Runs the controller once and returns the `(level, source, message)` of every
/// log event it wrote.
fn run_controller(app: &mut App) -> Vec<(LogLevel, &'static str, String)> {
    let mut tap = LogTap::new(app.world());
    app.world_mut()
        .run_system_once(crate::capture::capture_state_machine_update)
        .expect("capture_state_machine_update failed to run");
    tap.drain(app.world())
}

/// A fresh rig with the target at `target` and the nodes moving at
/// `node_velocity`, after `setup`; runs the controller once and returns the forces
/// on the root and the tail. A fresh app per call: avian only clears the
/// accumulated acceleration when it steps, and the rig never steps.
fn drive_with(
    target: DVec3,
    node_velocity: DVec3,
    setup: impl FnOnce(&mut App, &CaptureRig),
) -> (DVec3, DVec3) {
    let (mut app, rig) = branch_rig();
    setup(&mut app, &rig);
    place(&mut app, &rig, target, node_velocity);
    rig_step(&mut app, &rig);
    (
        applied_force(&mut app, rig.root),
        applied_force(&mut app, rig.tail),
    )
}

fn drive(target: DVec3, node_velocity: DVec3) -> (DVec3, DVec3) {
    drive_with(target, node_velocity, |_, _| {})
}

#[test]
fn controller_inside_inner_radius_pushes_both_nodes_away() {
    // 10 m < 0.8 × 25 m (and < 0.8 × 26 m for the tail): too close, push away from
    // the target, which is along +Y.
    let (root, tail) = drive(DVec3::new(0.0, 10.0, 0.0), DVec3::ZERO);
    assert_force_along(root, -DVec3::Y, "root, inside");
    assert_force_along(tail, -DVec3::Y, "tail, inside");
}

#[test]
fn controller_outside_radius_pulls_both_nodes_toward_the_target() {
    let (root, tail) = drive(DVec3::new(0.0, 100.0, 0.0), DVec3::ZERO);
    assert_force_along(root, DVec3::Y, "root, outside");
    assert_force_along(tail, DVec3::Y, "tail, outside");
}

#[test]
fn controller_tail_radius_is_one_metre_larger_than_the_roots() {
    // Capture radius 25 m: root band 20..25 m, tail band 20.8..26 m (radius + 1).
    // A fresh body enters a branch past the 2 % dead band: the root's outer edge
    // at 25.5 m and inner edge at 19.6 m, the tail's at 26.52 m and 20.384 m.
    // At 25.8 m the root is outside (pulled in, +Y) and the tail inside its band
    // (tangent, +Z).
    let (root, tail) = drive(DVec3::new(0.0, 25.8, 0.0), DVec3::ZERO);
    assert_force_along(root, DVec3::Y, "root at 25.8 m");
    assert_force_along(tail, DVec3::Z, "tail at 25.8 m");

    // At 20.2 m the root is inside its band (tangent, +Z) and the tail below its
    // inner edge (pushed away, −Y).
    let (root, tail) = drive(DVec3::new(0.0, 20.2, 0.0), DVec3::ZERO);
    assert_force_along(root, DVec3::Z, "root at 20.2 m");
    assert_force_along(tail, -DVec3::Y, "tail at 20.2 m");
}

#[test]
fn controller_tangent_band_pushes_sideways_around_the_target() {
    // 22 m is inside both bands. `up` = +X, `rel_r` = +Y: the push is `up × rel_r`
    // = +Z, for both nodes in every state but `capture`.
    let (root, tail) = drive(DVec3::new(0.0, 22.0, 0.0), DVec3::ZERO);
    assert_force_along(root, DVec3::Z, "root, tangent");
    assert_force_along(tail, DVec3::Z, "tail, tangent");
}

#[test]
fn controller_tangent_direction_reverses_for_non_root_nodes_in_capture_state() {
    let (root, tail) = drive_with(DVec3::new(0.0, 22.0, 0.0), DVec3::ZERO, |app, rig| {
        set_state(app, rig, "capture")
    });
    // The root keeps circling; the tail circles the other way, winding the tether
    // around the target.
    assert_force_along(root, DVec3::Z, "root, capture");
    assert_force_along(tail, -DVec3::Z, "tail, capture");
}

#[test]
fn controller_tangent_falls_back_to_x_when_up_is_parallel_to_the_range() {
    // Rotate the target so its `up` (rotation × X) is +Z, and put it 22 m along +Z:
    // `rel_r × up` = 0, so the axis falls back to +X and the push is
    // `X × rel_r` = −Y. Without the fallback `up × rel_r` is the zero vector and
    // no force would be applied.
    let (root, tail) = drive_with(DVec3::new(0.0, 0.0, 22.0), DVec3::ZERO, |app, rig| {
        app.world_mut()
            .entity_mut(rig.capture)
            .insert(Rotation(DQuat::from_rotation_y(
                -std::f64::consts::FRAC_PI_2,
            )));
    });
    assert_force_along(root, -DVec3::Y, "root, fallback");
    assert_force_along(tail, -DVec3::Y, "tail, fallback");
}

#[test]
fn controller_adds_a_braking_term_when_receding_outside_the_radius() {
    // Outside the sphere (100 m) with a relative speed of 0.5 m/s, under the 1.0 m/s
    // limit, so no velocity-kill term.
    // Approaching (the velocity has a +Y component toward the target): only the pull.
    let (root, _) = drive(DVec3::new(0.0, 100.0, 0.0), DVec3::new(0.0, 0.3, 0.4));
    assert_force_along(root, DVec3::Y, "approaching");

    // Receding (−Y component, angle to `rel_r` of about 127° > 90°): the pull plus
    // `−v̂` = (0, 0.6, −0.8).
    let (root, _) = drive(DVec3::new(0.0, 100.0, 0.0), DVec3::new(0.0, -0.3, 0.4));
    assert_force_along(root, DVec3::Y + DVec3::new(0.0, 0.6, -0.8), "receding");
}

#[test]
fn controller_kills_velocity_above_the_state_limit() {
    // 3.04 m/s relative speed against a 1.0 m/s limit: `−2 v̂` joins the pull.
    let velocity = DVec3::new(0.0, 0.5, 3.0);
    let (root, _) = drive(DVec3::new(0.0, 100.0, 0.0), velocity);
    assert_force_along(root, DVec3::Y - 2.0 * velocity.normalize(), "velocity kill");
}

/// Runs the controller `steps` times without advancing the physics and returns
/// the length of the force applied to `node` at each step (from `CaptureForce`,
/// which is per step; avian's accumulator only clears when it steps).
fn force_lengths(app: &mut App, node: Entity, steps: usize) -> Vec<f64> {
    (0..steps)
        .map(|_| {
            run_controller(app);
            capture_force(app, node).applied.length()
        })
        .collect()
}

#[test]
fn controller_force_magnitude_is_at_most_max_force_and_reaches_it_after_the_ramp() {
    // F-31: whatever the branch, the force is the direction (normalised) times the
    // ramped bound, which reaches `max_force` after FORCE_RAMP_S and stays there.
    let ramp_steps = (FORCE_RAMP_S / PHYS_DT).round() as usize;
    for target in [
        DVec3::new(0.0, 10.0, 0.0),
        DVec3::new(0.0, 22.0, 0.0),
        DVec3::new(0.0, 100.0, 0.0),
    ] {
        let (mut app, rig) = branch_rig();
        place(&mut app, &rig, target, DVec3::new(0.0, 0.1, 0.2));
        let lengths = force_lengths(&mut app, rig.root, ramp_steps + 36);
        for (i, length) in lengths.iter().enumerate() {
            assert!(
                *length <= MAX_FORCE * (1.0 + 1e-12),
                "step {} at {target:?}: {length} N > {MAX_FORCE} N",
                i + 1
            );
        }
        assert_close(
            lengths[ramp_steps - 2],
            MAX_FORCE * (ramp_steps - 1) as f64 / ramp_steps as f64,
            1e-9,
            &format!("one step before the end of the ramp at {target:?}"),
        );
        for (i, length) in lengths.iter().enumerate().skip(ramp_steps - 1) {
            assert_close(
                *length,
                MAX_FORCE,
                1e-9,
                &format!("step {} at {target:?}", i + 1),
            );
        }
    }
}

#[test]
fn controller_ramp_rises_from_zero_over_force_ramp_s() {
    // (c): the first step's bound is max_force × PHYS_DT / FORCE_RAMP_S, the bound
    // rises by that much per step, is max_force at step 64 and never above.
    let (mut app, rig) = branch_rig();
    place(&mut app, &rig, DVec3::new(0.0, 100.0, 0.0), DVec3::ZERO);
    let lengths = force_lengths(&mut app, rig.root, 100);
    assert_close(lengths[0], FIRST_STEP_FORCE, 1e-12, "step 1");
    for (i, length) in lengths.iter().enumerate().take(64) {
        assert_close(
            *length,
            FIRST_STEP_FORCE * (i + 1) as f64,
            1e-9,
            &format!("step {}", i + 1),
        );
    }
    assert_close(lengths[63], MAX_FORCE, 1e-9, "step 64");
    assert!(
        lengths.iter().all(|l| *l <= MAX_FORCE * (1.0 + 1e-12)),
        "{lengths:?}"
    );
    assert_close(
        capture_force(&app, rig.root).ramp_n,
        MAX_FORCE,
        1e-12,
        "ramp_n held at the bound",
    );
}

#[test]
fn controller_direction_reversal_does_not_restart_the_ramp() {
    // (d), C2: the pull flips from +Y to −Y at step 32 (the target moves to the
    // other side); the bound keeps rising and is max_force at step 64.
    let (mut app, rig) = branch_rig();
    place(&mut app, &rig, DVec3::new(0.0, 100.0, 0.0), DVec3::ZERO);
    let mut lengths = force_lengths(&mut app, rig.root, 31);
    assert_close_dvec3(
        capture_force(&app, rig.root).direction,
        DVec3::Y,
        1e-12,
        "before",
    );
    place(&mut app, &rig, DVec3::new(0.0, -100.0, 0.0), DVec3::ZERO);
    lengths.extend(force_lengths(&mut app, rig.root, 33));
    assert_close_dvec3(
        capture_force(&app, rig.root).direction,
        -DVec3::Y,
        1e-12,
        "after",
    );
    assert_close(
        lengths[31],
        FIRST_STEP_FORCE * 32.0,
        1e-9,
        "step 32, the reversal",
    );
    assert_close(lengths[63], MAX_FORCE, 1e-9, "step 64");
}

/// The branch the root is in after one controller step with the target `range_m`
/// along +Y and the root moving at `speed` along +Z: `(inner, outer, kill)`.
fn branches_after(app: &mut App, rig: &CaptureRig, range_m: f64, speed: f64) -> (bool, bool, bool) {
    place(
        app,
        rig,
        DVec3::new(0.0, range_m, 0.0),
        DVec3::new(0.0, 0.0, speed),
    );
    run_controller(app);
    let state = capture_force(app, rig.root);
    (state.inner, state.outer, state.kill)
}

#[test]
fn controller_branch_thresholds_have_a_dead_band() {
    // (e): R = 25 m (no shrink), max_velocity = 1 m/s, band 2 %. Each edge: a
    // fresh body just past the threshold keeps its branch; past the band it enters;
    // back just past the threshold it stays; past the band the other way it leaves.
    assert_close(BRANCH_DEAD_BAND, 0.02, 0.0, "the band");
    let inner = 0.8 * 25.0;
    let (mut app, rig) = branch_rig();
    assert!(
        !branches_after(&mut app, &rig, inner * 1.01, 0.0).0,
        "inner: 1.01, fresh"
    );
    assert!(
        !branches_after(&mut app, &rig, inner * 0.99, 0.0).0,
        "inner: 0.99, inside the band"
    );
    assert!(
        branches_after(&mut app, &rig, inner * 0.97, 0.0).0,
        "inner: 0.97 enters"
    );
    assert_force_direction(&app, rig.root, -DVec3::Y, "inner: pushed away");
    assert!(
        branches_after(&mut app, &rig, inner * 1.01, 0.0).0,
        "inner: 1.01 keeps it"
    );
    assert!(
        !branches_after(&mut app, &rig, inner * 1.03, 0.0).0,
        "inner: 1.03 leaves"
    );
    assert_force_direction(&app, rig.root, DVec3::Z, "inner left: tangent");

    let outer = 25.0;
    let (mut app, rig) = branch_rig();
    assert!(
        !branches_after(&mut app, &rig, outer * 1.01, 0.0).1,
        "outer: 1.01, fresh"
    );
    assert_force_direction(&app, rig.root, DVec3::Z, "outer not entered: tangent");
    assert!(
        branches_after(&mut app, &rig, outer * 1.03, 0.0).1,
        "outer: 1.03 enters"
    );
    assert_force_direction(&app, rig.root, DVec3::Y, "outer: pulled in");
    assert!(
        branches_after(&mut app, &rig, outer * 0.99, 0.0).1,
        "outer: 0.99 keeps it"
    );
    assert!(
        !branches_after(&mut app, &rig, outer * 0.97, 0.0).1,
        "outer: 0.97 leaves"
    );

    // The kill, in the tangent band (22 m): entered above max_velocity, left below
    // max_velocity × 0.98.
    let (mut app, rig) = branch_rig();
    assert!(
        !branches_after(&mut app, &rig, 22.0, 0.99).2,
        "kill: 0.99, fresh"
    );
    assert!(
        branches_after(&mut app, &rig, 22.0, 1.01).2,
        "kill: 1.01 enters"
    );
    assert!(
        branches_after(&mut app, &rig, 22.0, 0.99).2,
        "kill: 0.99 keeps it"
    );
    assert!(
        !branches_after(&mut app, &rig, 22.0, 0.97).2,
        "kill: 0.97 leaves"
    );
}

#[track_caller]
fn assert_force_direction(app: &App, node: Entity, direction: DVec3, what: &str) {
    assert_close_dvec3(
        capture_force(app, node).applied.normalize(),
        direction,
        1e-9,
        what,
    );
}

#[test]
fn controller_pushes_only_the_root_with_the_example_plan() {
    // (a), C7: `example_plan` sets `tail_actuation` in no phase, so in every phase
    // the root gets a force and the tail none.
    let mut app = test_app();
    load_example_plans(&mut app);
    let rig = capture_rig(&mut app, "example_plan");
    let phases: Vec<String> = app.world().resource::<CapturePlanLibrary>().plans["example_plan"]
        .phases
        .iter()
        .map(|p| p.id.clone())
        .collect();
    assert!(phases.len() >= 4, "{phases:?}");
    for phase in &phases {
        set_state(&mut app, &rig, phase);
        // A range at which this phase's transitions do not fire.
        let range = match phase.as_str() {
            "rendezvous" => 3_000.0,
            "approach" => 100.0,
            _ => 20.0,
        };
        place(&mut app, &rig, DVec3::new(0.0, range, 0.0), DVec3::ZERO);
        run_controller(&mut app);
        assert_eq!(
            app.world()
                .get::<CaptureComponent>(rig.capture)
                .unwrap()
                .current_state,
            *phase
        );
        assert!(
            capture_force(&app, rig.root).applied.length() > 0.0,
            "root in {phase}"
        );
        assert_eq!(
            capture_force(&app, rig.tail).applied,
            DVec3::ZERO,
            "tail in {phase}"
        );
    }
    assert_eq!(
        applied_force(&mut app, rig.tail),
        DVec3::ZERO,
        "nothing reached avian"
    );
}

/// `rendezvous` (root only) → `capture` (the tail too) below 10 m.
const TAIL_PLAN: &str = r#"{"schema_version": 1, "name": "tail", "tether": "T", "phases": [
  {"id": "rendezvous", "max_velocity_mps": 1.0, "max_force_n": 10.0,
   "transitions": [{"to": "capture", "all_of": {"distance_m": {"less_than": 10.0}}}]},
  {"id": "capture", "max_velocity_mps": 1.0, "max_force_n": 10.0, "tail_actuation": true}
]}"#;

#[test]
fn controller_pushes_the_tail_only_in_a_phase_with_tail_actuation() {
    // (b), C7: no tail force in `rendezvous`; after the transition into `capture`
    // the tail's force follows the ramp from 0 at that entry.
    let mut app = test_app();
    insert_plan_json(&mut app, "tail", TAIL_PLAN);
    let rig = capture_rig(&mut app, "tail");
    place(&mut app, &rig, DVec3::new(0.0, 50.0, 0.0), DVec3::ZERO);
    let tail = force_lengths(&mut app, rig.tail, 40);
    assert!(
        tail.iter().all(|l| *l == 0.0),
        "tail in rendezvous: {tail:?}"
    );
    assert_eq!(applied_force(&mut app, rig.tail), DVec3::ZERO);

    place(&mut app, &rig, DVec3::new(0.0, 5.0, 0.0), DVec3::ZERO);
    let tail = force_lengths(&mut app, rig.tail, 70);
    assert_eq!(
        app.world()
            .get::<CaptureComponent>(rig.capture)
            .unwrap()
            .current_state,
        "capture"
    );
    for (i, length) in tail.iter().enumerate() {
        let expected = (FIRST_STEP_FORCE * (i + 1) as f64).min(MAX_FORCE);
        assert_close(
            *length,
            expected,
            1e-9,
            &format!("tail, step {} of capture", i + 1),
        );
    }
}

#[test]
fn controller_phase_entry_restarts_the_ramp() {
    // (g): the root is at the bound after 64 steps of `rendezvous`; the step the
    // transition into `capture` fires, its bound is one ramp step again.
    let mut app = test_app();
    insert_plan_json(&mut app, "tail", TAIL_PLAN);
    let rig = capture_rig(&mut app, "tail");
    place(&mut app, &rig, DVec3::new(0.0, 50.0, 0.0), DVec3::ZERO);
    let root = force_lengths(&mut app, rig.root, 80);
    assert_close(root[79], MAX_FORCE, 1e-9, "root at the bound in rendezvous");
    place(&mut app, &rig, DVec3::new(0.0, 5.0, 0.0), DVec3::ZERO);
    let root = force_lengths(&mut app, rig.root, 2);
    assert_close(root[0], FIRST_STEP_FORCE, 1e-12, "root, the entry step");
    assert_close(
        root[1],
        2.0 * FIRST_STEP_FORCE,
        1e-12,
        "root, the step after",
    );
}

#[test]
fn controller_without_capture_force_pushes_nothing_and_warns() {
    // Every capture start inserts `CaptureForce` on the root and the tail; a pushed
    // node without it is reported, as a node without forces is.
    let (mut app, rig) = branch_rig();
    place(&mut app, &rig, DVec3::new(0.0, 100.0, 0.0), DVec3::ZERO);
    app.world_mut()
        .entity_mut(rig.tail)
        .remove::<CaptureForce>();
    let logs = run_controller(&mut app);
    assert_eq!(
        logs,
        vec![(
            LogLevel::Warn,
            "capture",
            "Failed to apply force to tether node 1 (no CaptureForce)".to_string()
        )]
    );
    assert_eq!(applied_force(&mut app, rig.tail), DVec3::ZERO);
    assert_force_along(applied_force(&mut app, rig.root), DVec3::Y, "root");
}

/// A plan whose first phase has a shrink rate and a transition at 10 m, so a
/// skipped state check shows as a missing transition and a missing shrink; the
/// tail is pushed (`tail_actuation`).
const ROOTLESS_PLAN: &str = r#"{"schema_version": 1, "name": "rootless", "tether": "T", "phases": [
  {"id": "s", "max_velocity_mps": 1.0, "max_force_n": 10.0, "shrink_rate_mps": 0.5,
   "tail_actuation": true,
   "transitions": [{"to": "t", "all_of": {"distance_m": {"less_than": 10.0}}}]},
  {"id": "t", "max_velocity_mps": 1.0, "max_force_n": 10.0, "tail_actuation": true}
]}"#;

#[test]
fn controller_without_a_root_body_skips_the_state_check_but_forces_the_other_nodes() {
    let mut app = test_app();
    insert_plan_json(&mut app, "rootless", ROOTLESS_PLAN);
    let rig = capture_rig(&mut app, "rootless");
    // The tether's first node is an entity with no rigid body.
    let dead_root = app.world_mut().spawn_empty().id();
    app.world_mut()
        .resource_mut::<OrbitalCache>()
        .tethers
        .insert("T".to_string(), vec![dead_root, rig.tail]);
    place(&mut app, &rig, DVec3::new(0.0, 5.0, 0.0), DVec3::ZERO);

    let events = rig_step(&mut app, &rig);

    // 5 m is under the transition's 10 m, but the root-relative range is unknown, so
    // the state is not checked: no transition, no sphere shrink.
    assert!(events.is_empty(), "unexpected transitions {events:?}");
    let component = app.world().get::<CaptureComponent>(rig.capture).unwrap();
    assert_eq!(component.current_state, "s");
    assert_eq!(
        app.world().resource::<CaptureSphereRadius>().radius,
        25.0,
        "no shrink without a root-relative range"
    );
    // The tail is still driven with the current state's limits: 5 m < 0.8 × 26 m,
    // pushed away along −Y.
    assert_force_along(applied_force(&mut app, rig.tail), -DVec3::Y, "tail");
}

#[test]
fn controller_with_an_empty_tether_only_advances_the_state_timer() {
    let (mut app, rig) = branch_rig();
    app.world_mut()
        .resource_mut::<OrbitalCache>()
        .tethers
        .insert("T".to_string(), Vec::new());
    rig_set(&mut app, &rig, 10.0, 0.0);

    let logs = run_controller(&mut app);

    assert!(logs.is_empty(), "unexpected log events {logs:?}");
    let component = app.world().get::<CaptureComponent>(rig.capture).unwrap();
    assert_eq!(component.current_state, "s");
    assert_close(component.state_elapsed_time_s, PHYS_DT, 1e-12, "elapsed");
}

#[test]
fn controller_reports_a_node_whose_forces_cannot_be_applied() {
    let (mut app, rig) = branch_rig();
    place(&mut app, &rig, DVec3::new(0.0, 100.0, 0.0), DVec3::ZERO);
    // The tail still matches the controller's rigid-body query but not `Forces`
    // (avian adds the integration data when it sets a body up).
    app.world_mut()
        .entity_mut(rig.tail)
        .remove::<avian3d::dynamics::integrator::VelocityIntegrationData>();

    let logs = run_controller(&mut app);

    assert_eq!(
        logs,
        vec![(
            LogLevel::Warn,
            "capture",
            "Failed to apply force to tether node 1 (RigidBody unavailable)".to_string()
        )]
    );
    // The root, node 0, was driven as usual.
    assert_force_along(applied_force(&mut app, rig.root), DVec3::Y, "root");
}

#[test]
fn controller_logs_an_error_when_the_plans_tether_is_not_registered() {
    let (mut app, rig) = branch_rig();
    app.world_mut()
        .resource_mut::<OrbitalCache>()
        .tethers
        .clear();
    rig_set(&mut app, &rig, 10.0, 0.0);

    let logs = run_controller(&mut app);

    assert_eq!(
        logs,
        vec![(
            LogLevel::Error,
            "capture",
            "Tether 'T' not found for plan 'branches'".to_string()
        )]
    );
    // The capture goes on (the tether may appear later); only this step is lost.
    let component = app.world().get::<CaptureComponent>(rig.capture).unwrap();
    assert_eq!(component.current_state, "s");
    assert_close(component.state_elapsed_time_s, PHYS_DT, 1e-12, "elapsed");
}

#[test]
fn controller_aborts_the_capture_when_the_compiled_plan_is_missing() {
    let (mut app, rig) = branch_rig();
    app.world_mut()
        .resource_mut::<CapturePlanLibrary>()
        .compiled_plans
        .remove(PLAN_ID);

    let logs = run_controller(&mut app);

    assert_eq!(
        logs,
        vec![(
            LogLevel::Error,
            "capture",
            "Compiled plan 'branches' not found — capture aborted".to_string()
        )]
    );
    assert!(
        app.world().get::<CaptureComponent>(rig.capture).is_none(),
        "the capture component is removed"
    );
}

/// `rel vel > limit` and `distance > limit` transitions, alone and together.
const GREATER_THAN_PLAN: &str = r#"{"name": "greater", "tether": "T", "states": [
  {"id": "a", "parameters": {"max_velocity": 1.0, "max_force": 10.0},
   "transitions": [{"to": "b", "relative_velocity": {"greater_than": 5.0}}]},
  {"id": "b", "parameters": {"max_velocity": 1.0, "max_force": 10.0},
   "transitions": [{"to": "c", "relative_velocity": {"greater_than": 5.0},
                    "distance": {"greater_than": 100.0}}]},
  {"id": "c", "parameters": {"max_velocity": 1.0, "max_force": 10.0}}
]}"#;

#[test]
fn transition_fires_on_relative_velocity_greater_than() {
    let mut app = test_app();
    insert_plan_json(&mut app, "greater", GREATER_THAN_PLAN);
    let rig = capture_rig(&mut app, "greater");

    // 4 m/s is under the limit: no transition. 6 m/s is over it.
    rig_set(&mut app, &rig, 50.0, 4.0);
    assert!(rig_step(&mut app, &rig).is_empty());
    rig_set(&mut app, &rig, 50.0, 6.0);
    assert_eq!(
        rig_step(&mut app, &rig),
        vec![(
            "a".to_string(),
            "b".to_string(),
            "rel vel 6.00 m/s > 5.00 m/s".to_string()
        )]
    );

    // `b -> c` needs both conditions: fast enough but too near, then both.
    rig_set(&mut app, &rig, 50.0, 6.0);
    assert!(rig_step(&mut app, &rig).is_empty());
    rig_set(&mut app, &rig, 150.0, 6.0);
    assert_eq!(
        rig_step(&mut app, &rig),
        vec![(
            "b".to_string(),
            "c".to_string(),
            "distance 150.0 m > 100.0 m, rel vel 6.00 m/s > 5.00 m/s".to_string()
        )]
    );
}

#[cfg(debug_assertions)]
mod finite_state {
    use super::*;
    use crate::capture::assert_finite_state;
    use crate::test_support::fixed_epoch;
    use crate::time::WorldTime;
    use avian3d::prelude::{RigidBody, RigidBodyDisabled};

    fn finite_state_app() -> App {
        let mut app = App::new();
        app.insert_resource(WorldTime::starting_at(fixed_epoch()));
        app
    }

    #[test]
    fn assert_finite_state_accepts_finite_bodies() {
        let mut app = finite_state_app();
        app.world_mut().spawn((
            RigidBody::Dynamic,
            Position(DVec3::new(1.0, 2.0, 3.0)),
            LinearVelocity(DVec3::new(0.0, -1.0, 0.5)),
        ));
        app.world_mut()
            .run_system_once(assert_finite_state)
            .expect("finite bodies must not panic");
    }

    #[test]
    fn assert_finite_state_ignores_disabled_bodies() {
        let mut app = finite_state_app();
        app.world_mut().spawn((
            RigidBody::Dynamic,
            RigidBodyDisabled,
            Position(DVec3::new(f64::NAN, 0.0, 0.0)),
            LinearVelocity::ZERO,
        ));
        app.world_mut()
            .run_system_once(assert_finite_state)
            .expect("a disabled body is not checked");
    }

    #[test]
    #[should_panic(expected = "non-finite state for")]
    fn assert_finite_state_panics_on_a_non_finite_position() {
        let mut app = finite_state_app();
        app.world_mut().spawn((
            RigidBody::Dynamic,
            Position(DVec3::new(0.0, f64::INFINITY, 0.0)),
            LinearVelocity::ZERO,
        ));
        app.world_mut()
            .run_system_once(assert_finite_state)
            .expect("the system must panic before it returns");
    }

    #[test]
    #[should_panic(expected = "at step 3: position")]
    fn assert_finite_state_names_the_step_of_a_non_finite_velocity() {
        let mut app = finite_state_app();
        app.world_mut().resource_mut::<WorldTime>().epoch = fixed_epoch() + 3.0 * PHYS_DT;
        app.world_mut().spawn((
            RigidBody::Dynamic,
            Position::default(),
            LinearVelocity(DVec3::new(f64::NAN, 0.0, 0.0)),
        ));
        app.world_mut()
            .run_system_once(assert_finite_state)
            .expect("the system must panic before it returns");
    }
}
