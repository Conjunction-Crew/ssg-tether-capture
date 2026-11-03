//! Branch tests for `capture_state_machine_update` and `assert_finite_state` (P5 S4).
//!
//! The characterization tests pin the state machine (transitions, timers, radius).
//! These pin the controller's force branches: for each branch the **direction** of
//! the force on the tether nodes and its magnitude (the ramp's first step), and the
//! log events of the paths where the controller gives up; then the P7.5 force law
//! (F-31, C2, C7): who is pushed, the ramp, the dead bands; and the O2 revision's
//! touches (P7.5 S6): the circling axis, the speed limit that scales with the
//! sphere, the absolute kill band and the sphere floor; and the O4 revision's
//! (P7.5 S9, design D16): the proportional limiter, the plane hold, the hold law
//! and the time-in-phase condition; and the O5 revision's (P7.5 S10, design D18):
//! the tail's lag law, the circle law and the limiter's magnitude rule. Geometry:
//! the nodes sit at
//! the origin and the capture target at `(0, r, 0)`, so `rel_r` (node to target)
//! points along +Y and `up` (the target's `CAPTURE_CIRCLING_AXIS`, +Z) is
//! perpendicular to it: the tangent push `up × rel_r` is −X.

use avian3d::prelude::{
    ComputedMass, Forces, LinearVelocity, Mass, Position, ReadRigidBodyForces, Rotation,
};
use bevy::ecs::system::RunSystemOnce;
use bevy::math::{DQuat, DVec3};
use bevy::prelude::*;

use crate::capture::plan::CapturePlanLibrary;
use crate::capture::{
    BRANCH_DEAD_BAND, CAPTURE_CIRCLING_AXIS, CaptureComponent, CaptureForce, CaptureSphereRadius,
    FORCE_RAMP_S, PLANE_HOLD_BAND_M, RADIAL_RATE_BAND_MPS, RADIUS_HOLD_BAND_M, SPACING_BAND_M,
    SPACING_MAX_HALF_ANGLE, SPACING_RATE_BAND_MPS, circle_law, forward_tangent, tail_lag_weight,
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
/// The tangent push for the geometry above: `CAPTURE_CIRCLING_AXIS × +Y` = −X.
const TANGENT: DVec3 = DVec3::NEG_X;

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
    // (tangent, −X).
    let (root, tail) = drive(DVec3::new(0.0, 25.8, 0.0), DVec3::ZERO);
    assert_force_along(root, DVec3::Y, "root at 25.8 m");
    assert_force_along(tail, TANGENT, "tail at 25.8 m");

    // At 20.2 m the root is inside its band (tangent, −X) and the tail below its
    // inner edge (pushed away, −Y).
    let (root, tail) = drive(DVec3::new(0.0, 20.2, 0.0), DVec3::ZERO);
    assert_force_along(root, TANGENT, "root at 20.2 m");
    assert_force_along(tail, -DVec3::Y, "tail at 20.2 m");
}

#[test]
fn controller_tangent_band_pushes_sideways_around_the_target() {
    // 22 m is inside both bands. `up` = `CAPTURE_CIRCLING_AXIS` = +Z (P7.5 S6,
    // design D14), `rel_r` = +Y: the push is `up × rel_r` = −X, for both nodes in
    // every state but `capture` (the tail too: `tail_actuation`).
    assert_eq!(CAPTURE_CIRCLING_AXIS, DVec3::Z);
    assert_eq!(CAPTURE_CIRCLING_AXIS.cross(DVec3::Y), TANGENT);
    let (root, tail) = drive(DVec3::new(0.0, 22.0, 0.0), DVec3::ZERO);
    assert_force_along(root, TANGENT, "root, tangent");
    assert_force_along(tail, TANGENT, "tail, tangent");
}

#[test]
fn controller_tangent_direction_reverses_for_non_root_nodes_in_capture_state() {
    let (root, tail) = drive_with(DVec3::new(0.0, 22.0, 0.0), DVec3::ZERO, |app, rig| {
        set_state(app, rig, "capture")
    });
    // The root keeps circling; the tail circles the other way about
    // `CAPTURE_CIRCLING_AXIS`, winding the tether around the target.
    assert_force_along(root, TANGENT, "root, capture");
    assert_force_along(tail, -TANGENT, "tail, capture");
}

#[test]
fn controller_tangent_falls_back_to_x_when_up_is_parallel_to_the_range() {
    // Rotate the target so its `up` (rotation × `CAPTURE_CIRCLING_AXIS`) is +Y, the
    // range direction: `rel_r × up` = 0, so the axis falls back to +X and the push
    // is `X × rel_r` = +Z. Without the fallback `up × rel_r` is the zero vector
    // and no force would be applied.
    let rotation = DQuat::from_rotation_x(-std::f64::consts::FRAC_PI_2);
    assert_close_dvec3(rotation * CAPTURE_CIRCLING_AXIS, DVec3::Y, 1e-12, "up");
    let (root, tail) = drive_with(DVec3::new(0.0, 22.0, 0.0), DVec3::ZERO, |app, rig| {
        app.world_mut()
            .entity_mut(rig.capture)
            .insert(Rotation(rotation));
    });
    assert_force_along(root, DVec3::Z, "root, fallback");
    assert_force_along(tail, DVec3::Z, "tail, fallback");
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
    // 3.04 m/s relative speed against a 1.0 m/s limit: the root is in the kill
    // branch, and the proportional limiter (P7.5 S9, design D16) brakes it at full
    // weight beside the pull (0.5 m/s along +Y is below the limit, so the pull
    // keeps its full weight): `Y − v̂`, bounded by the ramp.
    let velocity = DVec3::new(0.0, 0.5, 3.0);
    let (mut app, rig) = branch_rig();
    place(&mut app, &rig, DVec3::new(0.0, 100.0, 0.0), velocity);
    rig_step(&mut app, &rig);
    assert!(capture_force(&app, rig.root).kill, "the kill branch state");
    let root = applied_force(&mut app, rig.root);
    assert_force_along(root, DVec3::Y - velocity.normalize(), "velocity kill");
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
    assert_force_direction(&app, rig.root, TANGENT, "inner left: tangent");

    let outer = 25.0;
    let (mut app, rig) = branch_rig();
    assert!(
        !branches_after(&mut app, &rig, outer * 1.01, 0.0).1,
        "outer: 1.01, fresh"
    );
    assert_force_direction(&app, rig.root, TANGENT, "outer not entered: tangent");
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
    // max_velocity × 0.98 for a body heavy enough that one step's Δv at the
    // phase's force (2 × 10 N × PHYS_DT / 500 kg = 0.0006 m/s) is below the
    // relative band (P7.5 S6: the band is the larger of the two; the light-body
    // case is `controller_kill_band_is_at_least_one_steps_dv_of_the_pushed_body`).
    let (mut app, rig) = branch_rig();
    app.world_mut()
        .entity_mut(rig.root)
        .insert(ComputedMass::new(500.0));
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
fn controller_pushes_the_tail_only_in_the_example_plans_terminal_capture_and_hold_phases() {
    // (a), C7: the root gets a force in every phase; the tail only in the phases
    // where `example_plan` sets `tail_actuation` (P7.5 S6, E9: `terminal` and
    // `capture`; S9, design D16: `hold`), and only those forces reach avian.
    let mut app = test_app();
    load_example_plans(&mut app);
    let rig = capture_rig(&mut app, "example_plan");
    let phases: Vec<String> = app.world().resource::<CapturePlanLibrary>().plans["example_plan"]
        .phases
        .iter()
        .map(|p| p.id.clone())
        .collect();
    assert_eq!(
        phases,
        [
            "rendezvous",
            "approach",
            "final_approach",
            "terminal",
            "capture",
            "hold"
        ]
    );
    for phase in &phases {
        set_state(&mut app, &rig, phase);
        // A range at which this phase's transitions do not fire.
        let range = match phase.as_str() {
            "rendezvous" => 3_000.0,
            "approach" => 1_000.0,
            "final_approach" => 200.0,
            _ => 20.0,
        };
        // The hold law brakes a moving body; a body at rest gets no force.
        let velocity = if phase == "hold" {
            DVec3::new(0.0, 0.0, 0.1)
        } else {
            DVec3::ZERO
        };
        place(&mut app, &rig, DVec3::new(0.0, range, 0.0), velocity);
        let tail_before = applied_force(&mut app, rig.tail);
        run_controller(&mut app);
        let tail_pushed = matches!(phase.as_str(), "terminal" | "capture" | "hold");
        assert_eq!(
            capture_force(&app, rig.tail).applied.length() > 0.0,
            tail_pushed,
            "tail in {phase}"
        );
        assert_close_dvec3(
            applied_force(&mut app, rig.tail) - tail_before,
            capture_force(&app, rig.tail).applied,
            1e-9,
            &format!("what reached avian for the tail in {phase}"),
        );
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
    }
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

/// One phase at 0.5 m/s and 2 N (the example plan's terminal limits), with the
/// speed limit scaling with the sphere or not; no transitions, no shrink.
fn terminal_like_plan(scales: bool) -> String {
    format!(
        r#"{{"schema_version": 1, "name": "terminal", "tether": "T", "phases": [
  {{"id": "t", "max_velocity_mps": 0.5, "max_force_n": 2.0, "velocity_scales_with_sphere": {scales}}}
]}}"#
    )
}

/// A rig on [`terminal_like_plan`] with the sphere at `radius_m` (entered at the
/// initial 25 m) and the root's `ComputedMass` at `root_mass_kg`.
fn terminal_rig(scales: bool, radius_m: f64, root_mass_kg: f64) -> (App, CaptureRig) {
    let mut app = test_app();
    insert_plan_json(&mut app, "terminal", &terminal_like_plan(scales));
    let rig = capture_rig(&mut app, "terminal");
    app.world_mut().resource_mut::<CaptureSphereRadius>().radius = radius_m;
    app.world_mut()
        .entity_mut(rig.root)
        .insert(ComputedMass::new(root_mass_kg));
    (app, rig)
}

#[test]
fn controller_speed_limit_scales_with_the_sphere_only_in_a_phase_that_asks() {
    // P7.5 S6 (a), design D14: R = 12.5 m of the 25 m the phase was entered at, so
    // the scaled limit is 0.25 m/s; 0.3 m/s is above it and below the phase's
    // 0.5 m/s. 11 m is inside the tangent band of R = 12.5 m (10.2–12.25 m).
    let (mut app, rig) = terminal_rig(true, 12.5, 2.0);
    assert_eq!(
        app.world()
            .get::<CaptureComponent>(rig.capture)
            .unwrap()
            .sphere_radius_at_entry_m,
        25.0
    );
    let (inner, outer, kill) = branches_after(&mut app, &rig, 11.0, 0.3);
    assert!(!inner && !outer, "in the tangent band");
    assert!(kill, "scaled: 0.3 m/s is above 0.5 × 12.5 / 25 = 0.25 m/s");

    let (mut app, rig) = terminal_rig(false, 12.5, 2.0);
    let (inner, outer, kill) = branches_after(&mut app, &rig, 11.0, 0.3);
    assert!(!inner && !outer, "in the tangent band");
    assert!(!kill, "unscaled: 0.3 m/s is below 0.5 m/s");
}

/// `shrink` (the sphere at 0.64 m/s, 0.01 m per step) → `next` below 10 m, which
/// shrinks too; the sphere's floor is `min_radius_m` when given.
fn shrink_plan(min_radius_m: Option<f64>) -> String {
    let sphere = min_radius_m
        .map(|r| format!(r#""capture_sphere": {{"min_radius_m": {r}}},"#))
        .unwrap_or_default();
    format!(
        r#"{{"schema_version": 1, "name": "shrink", "tether": "T", {sphere} "phases": [
  {{"id": "shrink", "max_velocity_mps": 1.0, "max_force_n": 10.0, "shrink_rate_mps": 6.4,
   "transitions": [{{"to": "next", "all_of": {{"distance_m": {{"less_than": 10.0}}}}}}]}},
  {{"id": "next", "max_velocity_mps": 1.0, "max_force_n": 10.0, "shrink_rate_mps": 0.64}}
]}}"#
    )
}

#[test]
fn controller_sets_the_entry_radius_at_the_capture_start_and_at_a_phase_entry() {
    // P7.5 S6 (b): the capture start sets it to the plan's initial radius (here
    // 30 m); a phase entry to the radius when the transition fires, before that
    // step's shrink.
    let mut app = test_app();
    insert_plan_json(
        &mut app,
        "entry",
        r#"{"schema_version": 1, "name": "entry", "tether": "T",
  "capture_sphere": {"initial_radius_m": 30.0}, "phases": [
  {"id": "a", "max_velocity_mps": 1.0, "max_force_n": 10.0,
   "transitions": [{"to": "b", "all_of": {"distance_m": {"less_than": 10.0}}}]},
  {"id": "b", "max_velocity_mps": 1.0, "max_force_n": 10.0, "shrink_rate_mps": 0.64}
]}"#,
    );
    let rig = capture_rig(&mut app, "entry");
    let entry = |app: &App| {
        app.world()
            .get::<CaptureComponent>(rig.capture)
            .unwrap()
            .sphere_radius_at_entry_m
    };
    assert_eq!(entry(&app), 30.0, "the capture start");
    app.world_mut().resource_mut::<CaptureSphereRadius>().radius = 17.0;
    place(&mut app, &rig, DVec3::new(0.0, 50.0, 0.0), DVec3::ZERO);
    run_controller(&mut app);
    assert_eq!(entry(&app), 30.0, "no entry, no change");
    place(&mut app, &rig, DVec3::new(0.0, 5.0, 0.0), DVec3::ZERO);
    run_controller(&mut app);
    let component = app.world().get::<CaptureComponent>(rig.capture).unwrap();
    assert_eq!(component.current_state, "b");
    assert_eq!(entry(&app), 17.0, "the phase entry, before the shrink");
    assert_close(
        app.world().resource::<CaptureSphereRadius>().radius,
        17.0 - 0.01,
        1e-12,
        "the entry step shrank once",
    );
}

#[test]
fn controller_shrinks_the_sphere_down_to_the_plans_floor_and_0_1_by_default() {
    // P7.5 S6 (c), design D13: the sphere shrinks while its radius is above the
    // floor: 0.1 m per step from 4.75 m against 4.5 m stops at 4.45 m; from 0.35 m
    // against the default 0.1 m it stops at 0.05 m.
    for (floor, start, stop) in [(Some(4.5), 4.75, 4.45), (None, 0.35, 0.05)] {
        let mut app = test_app();
        insert_plan_json(&mut app, "shrink", &shrink_plan(floor));
        let rig = capture_rig(&mut app, "shrink");
        app.world_mut().resource_mut::<CaptureSphereRadius>().radius = start;
        place(&mut app, &rig, DVec3::new(0.0, 50.0, 0.0), DVec3::ZERO);
        for _ in 0..20 {
            run_controller(&mut app);
        }
        assert_close(
            app.world().resource::<CaptureSphereRadius>().radius,
            stop,
            1e-9,
            &format!("floor {floor:?}"),
        );
    }
}

#[test]
fn controller_kill_band_is_at_least_one_steps_dv_of_the_pushed_body() {
    // P7.5 S6 (e), design D7 (C4): 2 N on a 2 kg root gives a band of
    // 2 × 2 N × PHYS_DT / 2 kg = 0.03125 m/s, wider than 2 % of 0.5 m/s (0.010).
    // In the kill it stays at 0.48 m/s and leaves at 0.46 (the relative band would
    // have left at 0.489). 22 m is inside the tangent band of R = 25 m.
    let (mut app, rig) = terminal_rig(false, 25.0, 2.0);
    assert!(!branches_after(&mut app, &rig, 22.0, 0.49).2, "0.49, fresh");
    assert!(branches_after(&mut app, &rig, 22.0, 0.51).2, "0.51 enters");
    assert!(
        branches_after(&mut app, &rig, 22.0, 0.48).2,
        "0.48 keeps it"
    );
    assert!(!branches_after(&mut app, &rig, 22.0, 0.46).2, "0.46 leaves");

    // The scaled limit 0.1 m/s (R = 5 m of 25) keeps the band at 0.03125, taken
    // from the unscaled limit: entered above 0.1, kept at 0.08, left at 0.06.
    // 4.5 m is inside the tangent band of R = 5 m (4.08–4.9 m).
    let (mut app, rig) = terminal_rig(true, 5.0, 2.0);
    assert!(
        !branches_after(&mut app, &rig, 4.5, 0.09).2,
        "scaled: 0.09, fresh"
    );
    assert!(
        branches_after(&mut app, &rig, 4.5, 0.11).2,
        "scaled: 0.11 enters"
    );
    assert!(
        branches_after(&mut app, &rig, 4.5, 0.08).2,
        "scaled: 0.08 keeps it"
    );
    assert!(
        !branches_after(&mut app, &rig, 4.5, 0.06).2,
        "scaled: 0.06 leaves"
    );

    // A 500 kg root (a stated `chaser.mass_kg`) under the same phase: one step is
    // 0.000125 m/s, so the relative band (0.01 m/s) governs: kept at 0.495, left
    // at 0.489.
    let (mut app, rig) = terminal_rig(false, 25.0, 500.0);
    assert!(
        branches_after(&mut app, &rig, 22.0, 0.51).2,
        "500 kg: 0.51 enters"
    );
    assert!(
        branches_after(&mut app, &rig, 22.0, 0.495).2,
        "500 kg: 0.495 keeps it"
    );
    assert!(
        !branches_after(&mut app, &rig, 22.0, 0.489).2,
        "500 kg: 0.489 leaves"
    );
}

/// A rig on one phase at 0.5 m/s and 2 N with the extra phase keys `keys` (e.g.
/// `, "hold_plane": true`), the root's `ComputedMass` at 2 kg: the kill band is
/// `2 × 2 N × PHYS_DT / 2 kg` = 0.03125 m/s.
fn d16_rig(keys: &str) -> (App, CaptureRig) {
    let mut app = test_app();
    insert_plan_json(
        &mut app,
        "d16",
        &format!(
            r#"{{"schema_version": 1, "name": "d16", "tether": "T", "phases": [
  {{"id": "p", "max_velocity_mps": 0.5, "max_force_n": 2.0{keys}}}
]}}"#
        ),
    );
    let rig = capture_rig(&mut app, "d16");
    app.world_mut()
        .entity_mut(rig.root)
        .insert(ComputedMass::new(2.0));
    (app, rig)
}

const D16_KILL_BAND: f64 = 2.0 * 2.0 * PHYS_DT / 2.0;

/// Places the target at 22 m along +Y (inside the tangent band of R = 25 m: the
/// branch push is `TANGENT`), the root `height_m` above the circling plane
/// (along `CAPTURE_CIRCLING_AXIS`, +Z) moving at `velocity`; runs the controller
/// once and returns the root's applied force over its ramped bound.
fn root_force_over_ramp(app: &mut App, rig: &CaptureRig, height_m: f64, velocity: DVec3) -> DVec3 {
    place(app, rig, DVec3::new(0.0, 22.0, 0.0), velocity);
    app.world_mut()
        .entity_mut(rig.root)
        .insert(Position(DVec3::new(0.0, 0.0, height_m)));
    run_controller(app);
    let state = capture_force(app, rig.root);
    assert!(
        state.applied.length() <= state.ramp_n * (1.0 + 1e-12),
        "|{:?}| above the ramp {}",
        state.applied,
        state.ramp_n
    );
    state.applied / state.ramp_n
}

#[test]
fn controller_limiter_fades_the_push_at_the_limit_and_brakes_above_it() {
    // P7.5 S9 (a), design D16 (F-31): a 2 kg root under 2 N, limit 0.5 m/s, kill
    // band 0.03125 m/s, pushed along `TANGENT` (−X).
    assert_close(D16_KILL_BAND, 0.03125, 0.0, "the kill band");
    let (mut app, rig) = d16_rig("");
    // At the limit along the push: no push and no brake.
    let f = root_force_over_ramp(&mut app, &rig, 0.0, TANGENT * 0.5);
    assert_close_dvec3(f, DVec3::ZERO, 1e-12, "0.5 m/s along the push");
    // At the limit sideways to the push: the full push (nothing along it yet).
    let f = root_force_over_ramp(&mut app, &rig, 0.0, DVec3::new(0.0, 0.0, 0.5));
    assert_close_dvec3(f, TANGENT, 1e-12, "0.5 m/s sideways");
    // 0.1 m/s above the limit: (0.6 − 0.5) / 0.03125 clamps to 1, a brake of the
    // full ramp along −v̂, and no push.
    let f = root_force_over_ramp(&mut app, &rig, 0.0, TANGENT * 0.6);
    assert_close_dvec3(f, -TANGENT, 1e-12, "0.6 m/s along the push");
    // 0.01 m/s below the limit: (0.5 − 0.49) / 0.03125 = 0.32 of the push.
    let f = root_force_over_ramp(&mut app, &rig, 0.0, TANGENT * 0.49);
    assert_close_dvec3(f, TANGENT * 0.32, 1e-12, "0.49 m/s along the push");
    // Never above the ramp, whatever the velocity (checked in the helper).
    for speed in [0.0, 0.2, 0.47, 0.5, 0.53, 0.8, 3.0] {
        for direction in [TANGENT, -TANGENT, DVec3::Y, DVec3::new(-0.6, 0.0, 0.8)] {
            root_force_over_ramp(&mut app, &rig, 0.0, direction * speed);
        }
    }
}

#[test]
fn controller_plane_hold_pulls_toward_the_circling_plane_only_in_a_phase_that_asks() {
    // P7.5 S9 (b), design D16: `−up × clamp(h / PLANE_HOLD_BAND_M, −1, 1)` beside the
    // branch term, before normalisation: 2 m above the plane weight 1, 0.5 m above
    // weight 0.5, below the plane the other way; no term without the key.
    assert_close(PLANE_HOLD_BAND_M, 1.0, 0.0, "the band");
    let up = CAPTURE_CIRCLING_AXIS;
    let (mut app, rig) = d16_rig(r#", "hold_plane": true"#);
    for (height, expected) in [
        (2.0, TANGENT - up),
        (0.5, TANGENT - 0.5 * up),
        (-0.5, TANGENT + 0.5 * up),
        (0.0, TANGENT),
    ] {
        let f = root_force_over_ramp(&mut app, &rig, height, DVec3::ZERO);
        assert_close_dvec3(f, expected.normalize(), 1e-9, &format!("{height} m"));
    }
    let (mut app, rig) = d16_rig("");
    let f = root_force_over_ramp(&mut app, &rig, 2.0, DVec3::ZERO);
    assert_close_dvec3(f, TANGENT, 1e-9, "no key");
}

#[test]
fn controller_hold_law_brakes_to_rest_and_holds_the_plane() {
    // P7.5 S9 (c), design D16: in a `hold` phase the branches are skipped and the
    // limit is 0: `−v̂ × clamp(|v| / kill_band, 0, 1)` plus the plane term, then
    // bounded by the ramp (not normalised).
    let up = CAPTURE_CIRCLING_AXIS;
    let (mut app, rig) = d16_rig(r#", "actuation": "hold""#);
    let f = root_force_over_ramp(&mut app, &rig, 0.0, DVec3::ZERO);
    assert_close_dvec3(f, DVec3::ZERO, 1e-12, "at rest");
    let velocity = DVec3::new(0.06, 0.0, 0.08);
    let f = root_force_over_ramp(&mut app, &rig, 0.0, velocity);
    assert_close_dvec3(f, -velocity.normalize(), 1e-12, "0.1 m/s: the full ramp");
    let f = root_force_over_ramp(&mut app, &rig, 0.0, TANGENT * 0.01);
    assert_close_dvec3(f, -TANGENT * 0.32, 1e-12, "0.01 m/s: 0.32 of it");
    // Without the key the height changes nothing.
    let f = root_force_over_ramp(&mut app, &rig, 2.0, DVec3::ZERO);
    assert_close_dvec3(f, DVec3::ZERO, 1e-12, "2 m above, no key");
    assert!(capture_force(&app, rig.root).kill, "the limit is read as 0");

    let (mut app, rig) = d16_rig(r#", "actuation": "hold", "hold_plane": true"#);
    let f = root_force_over_ramp(&mut app, &rig, 2.0, DVec3::ZERO);
    assert_close_dvec3(f, -up, 1e-12, "2 m above: the plane term");
    let f = root_force_over_ramp(&mut app, &rig, 0.5, DVec3::ZERO);
    assert_close_dvec3(f, -0.5 * up, 1e-12, "0.5 m above: half of it");
    let f = root_force_over_ramp(&mut app, &rig, 2.0, TANGENT * 0.1);
    assert_close_dvec3(
        f,
        (-TANGENT - up).normalize(),
        1e-12,
        "moving and above: the sum, bounded",
    );
}

/// The ends for the lag law's cases: the target at the origin, the root on +X
/// (`tail_radius − 1` out) and the tail on its sphere of `tail_radius`, `theta`
/// about +Z from the root (positive: behind it, since the root's forward tangent
/// there is −Y), each moving at the given speed along its own forward tangent.
/// Returns each end's `(rel_r, rel_v)` (from the end to the target).
fn lag_ends(
    tail_radius: f64,
    theta: f64,
    root_speed: f64,
    tail_speed: f64,
) -> ((DVec3, DVec3), (DVec3, DVec3)) {
    let up = CAPTURE_CIRCLING_AXIS;
    let root_r = -DVec3::X * (tail_radius - 1.0);
    let tail_r = -DVec3::new(theta.cos(), theta.sin(), 0.0) * tail_radius;
    (
        (root_r, forward_tangent(root_r, up) * root_speed),
        (tail_r, forward_tangent(tail_r, up) * tail_speed),
    )
}

/// The angle of arc that spans `chord` on a sphere of `radius`.
fn arc_of(chord: f64, radius: f64) -> f64 {
    2.0 * (chord / (2.0 * radius)).asin()
}

#[test]
fn tail_lag_weight_holds_the_chord_behind_the_root() {
    // P7.5 S10 (a), design D18: `w = clamp((c_lag − c*) / 3 m − Δv / 0.3 m/s, −1, 1)`
    // on a 50 m tail sphere with `tail_spacing_m` 12 m.
    assert_eq!((SPACING_BAND_M, SPACING_RATE_BAND_MPS), (3.0, 0.3));
    let up = CAPTURE_CIRCLING_AXIS;
    let w = |chord: f64, root_speed: f64, tail_speed: f64, spacing: f64| {
        let (root, tail) = lag_ends(50.0, arc_of(chord, 50.0), root_speed, tail_speed);
        tail_lag_weight(root, tail, up, 50.0, spacing)
    };
    assert_close(
        w(12.0, 0.0, 0.0, 12.0),
        0.0,
        1e-12,
        "12 m behind, same speed",
    );
    assert_close(
        w(15.0, 0.0, 0.0, 12.0),
        1.0,
        1e-12,
        "15 m behind: saturated",
    );
    assert_close(w(9.0, 0.0, 0.0, 12.0), -1.0, 1e-12, "9 m behind");
    assert_close(w(12.0, 0.5, 0.8, 12.0), -1.0, 1e-12, "0.3 m/s faster");
    assert_close(w(12.0, 0.5, 0.35, 12.0), 0.5, 1e-12, "0.15 m/s slower");
    // θ is positive behind the root, negative ahead of it.
    assert_close(w(1.5, 0.0, 0.0, 1e-3), 1.499 / 3.0, 1e-12, "1.5 m behind");
    assert_close(w(-1.5, 0.0, 0.0, 1e-3), -1.501 / 3.0, 1e-12, "1.5 m ahead");

    // On a 5 m sphere 12 m (or 19 m) of chord is held at 150° of arc.
    assert_close(SPACING_MAX_HALF_ANGLE, 75.0_f64.to_radians(), 0.0, "75°");
    let on_5m = |theta_deg: f64, spacing: f64| {
        let (root, tail) = lag_ends(5.0, theta_deg.to_radians(), 0.0, 0.0);
        tail_lag_weight(root, tail, up, 5.0, spacing)
    };
    assert_close(on_5m(150.0, 12.0), 0.0, 1e-12, "150° with 12 m");
    assert_close(on_5m(150.0, 19.0), 0.0, 1e-12, "150° with 19 m");
    let at_120 = (2.0 * 5.0 * 60f64.to_radians().sin() - 2.0 * 5.0 * 75f64.to_radians().sin())
        / SPACING_BAND_M;
    assert_close(on_5m(120.0, 12.0), at_120, 1e-12, "120° with 12 m");
}

/// The tail pushed in every phase, `tail_spacing_m` 0.9 m: `p` the branch law, then
/// `capture` (the counter-tangent wrap) and `hold` (the brake); `shrink` the circle
/// law (0.05 m/s); `branches` the same as `p` without the key.
const D18_PLAN: &str = r#"{"schema_version": 1, "name": "d18", "tether": "T", "phases": [
  {"id": "p", "max_velocity_mps": 1.0, "max_force_n": 10.0, "tail_actuation": true,
   "tail_spacing_m": 0.9},
  {"id": "capture", "max_velocity_mps": 1.0, "max_force_n": 10.0, "tail_actuation": true,
   "tail_spacing_m": 0.9},
  {"id": "hold", "max_velocity_mps": 0.0, "max_force_n": 10.0, "tail_actuation": true,
   "tail_spacing_m": 0.9, "actuation": "hold"},
  {"id": "shrink", "max_velocity_mps": 1.0, "max_force_n": 10.0, "tail_actuation": true,
   "shrink_rate_mps": 0.05},
  {"id": "branches", "max_velocity_mps": 1.0, "max_force_n": 10.0, "tail_actuation": true}
]}"#;

/// Runs the controller once in `state` with the target at `target` and both ends
/// at the origin at rest; returns the root's and the tail's applied force over
/// their ramped bounds.
fn d18_forces_over_ramp(state: &str, target: DVec3) -> (DVec3, DVec3) {
    let mut app = test_app();
    insert_plan_json(&mut app, "d18", D18_PLAN);
    let rig = capture_rig(&mut app, "d18");
    set_state(&mut app, &rig, state);
    place(&mut app, &rig, target, DVec3::ZERO);
    run_controller(&mut app);
    let (root, tail) = (capture_force(&app, rig.root), capture_force(&app, rig.tail));
    (root.applied / root.ramp_n, tail.applied / tail.ramp_n)
}

#[test]
fn controller_lag_law_weights_the_tail_tangent_only_where_it_applies() {
    // P7.5 S10 (a), design D18: both ends at one point (θ = 0, the chord 0) 22 m
    // from the target, in the tangent branch: `w = (0 − 0.9) / 3 = −0.3`, so the
    // tail is pushed back along its tangent at 0.3 of the ramp (the magnitude rule)
    // while the root keeps its full tangent push.
    let target = DVec3::new(0.0, 22.0, 0.0);
    let (root, tail) = d18_forces_over_ramp("p", target);
    assert_close_dvec3(root, TANGENT, 1e-12, "the root: no lag term");
    assert_close_dvec3(tail, TANGENT * -0.3, 1e-12, "the tail: the lag weight");
    // Not in `capture` (the counter-tangent), `hold` (the brake), or without the key.
    let (root, tail) = d18_forces_over_ramp("capture", target);
    assert_close_dvec3(root, TANGENT, 1e-12, "capture: the root");
    assert_close_dvec3(tail, -TANGENT, 1e-12, "capture: the counter-tangent");
    let (root, tail) = d18_forces_over_ramp("hold", target);
    assert_close_dvec3(root, DVec3::ZERO, 1e-12, "hold: the root at rest");
    assert_close_dvec3(tail, DVec3::ZERO, 1e-12, "hold: the tail at rest");
    let (_, tail) = d18_forces_over_ramp("branches", target);
    assert_close_dvec3(tail, TANGENT, 1e-12, "no key: the full tangent");
}

#[test]
fn circle_law_holds_the_radius_against_the_shrink() {
    // P7.5 S10 (b), design D18, on a 25 m sphere shrinking at 0.05 m/s; the target
    // at +Y (`r̂_in`), the tangent `TANGENT`:
    // `t × (1 − |w_r|) + r̂_in × clamp(w_r + w_v, −1, 1)`.
    assert_eq!((RADIUS_HOLD_BAND_M, RADIAL_RATE_BAND_MPS), (2.0, 0.1));
    let r_in = DVec3::Y;
    let law = |range: f64, rel_v: DVec3, tangent: DVec3, lag: bool| {
        circle_law(r_in * range, rel_v, 25.0, 0.05, tangent, lag)
    };
    // 2 m outside at rest (w_r 1, w_v 0.5): inward at full weight, no tangent.
    assert_close_dvec3(
        law(27.0, DVec3::ZERO, TANGENT, false),
        r_in,
        1e-12,
        "2 m out",
    );
    // On R closing at the shrink rate (w_r 0, w_v 0): the tangent only.
    assert_close_dvec3(
        law(25.0, r_in * 0.05, TANGENT, false),
        TANGENT,
        1e-12,
        "on R",
    );
    // On R moving outward at 0.1 m/s (w_v 1): inward at full weight beside the tangent.
    assert_close_dvec3(
        law(25.0, -r_in * 0.1, TANGENT, false),
        TANGENT + r_in,
        1e-12,
        "on R, outward",
    );
    // 1 m inside closing at the shrink rate (w_r −0.5): out at half, half the tangent.
    assert_close_dvec3(
        law(24.0, r_in * 0.05, TANGENT, false),
        TANGENT * 0.5 - r_in * 0.5,
        1e-12,
        "1 m in",
    );
    // Under the lag law the tangent term keeps its weight off the radius.
    assert_close_dvec3(
        law(26.0, DVec3::ZERO, TANGENT * 0.3, true),
        TANGENT * 0.3 + r_in,
        1e-12,
        "1 m out, the lag law",
    );
}

#[test]
fn controller_circle_law_replaces_the_branches_in_a_shrinking_phase() {
    // P7.5 S10 (b), design D18: 19 m from the target, inside both ends' inner edges
    // (0.8 R): the branch law pushes straight out at full weight; the circle law at
    // rest (w_r −1, w_v 0.5) pushes out at half weight and no tangent, while the
    // branch state is still updated.
    let target = DVec3::new(0.0, 19.0, 0.0);
    let (root, tail) = d18_forces_over_ramp("branches", target);
    assert_close_dvec3(root, -DVec3::Y, 1e-12, "the branch law: the root");
    assert_close_dvec3(tail, -DVec3::Y, 1e-12, "the branch law: the tail");
    let mut app = test_app();
    insert_plan_json(&mut app, "d18", D18_PLAN);
    let rig = capture_rig(&mut app, "d18");
    set_state(&mut app, &rig, "shrink");
    place(&mut app, &rig, target, DVec3::ZERO);
    run_controller(&mut app);
    assert!(
        app.world().resource::<CaptureSphereRadius>().radius < 25.0,
        "the sphere shrinks"
    );
    for (node, what) in [(rig.root, "root"), (rig.tail, "tail")] {
        let state = capture_force(&app, node);
        assert_close_dvec3(
            state.applied / state.ramp_n,
            -DVec3::Y * 0.5,
            1e-12,
            &format!("the circle law: the {what}"),
        );
        assert!(state.inner, "the {what}'s inner branch state is updated");
    }
}

#[test]
fn controller_magnitude_rule_keeps_a_proportional_push_and_the_unit_branch_push() {
    // P7.5 S10 (c), design D18: a composed vector of length 0.3 gives 0.3 of the
    // ramp at zero speed (the lag weight above); a branch push whose normalised
    // direction is a hair short of unit length (|d̂|² = 0.9999999999999999, the
    // target at (10, 20, 0)) gives exactly the S9 force `d̂ × ramp`.
    let (_, tail) = d18_forces_over_ramp("p", DVec3::new(0.0, 22.0, 0.0));
    assert_close(tail.length(), 0.3, 1e-12, "0.3 of the ramp");

    let (mut app, rig) = branch_rig();
    let target = DVec3::new(10.0, 20.0, 0.0);
    place(&mut app, &rig, target, DVec3::ZERO);
    run_controller(&mut app);
    let d = CAPTURE_CIRCLING_AXIS.cross(target).normalize();
    assert!(d.length() < 1.0, "the case bites: |d| = {}", d.length());
    let state = capture_force(&app, rig.root);
    assert_eq!(state.applied, d * state.ramp_n, "bit for bit the S9 force");
}

/// `a` → `capture` below 10 m; `capture` → `hold` after 90 s in phase.
const TIME_PLAN: &str = r#"{"schema_version": 1, "name": "time", "tether": "T", "phases": [
  {"id": "a", "max_velocity_mps": 1.0, "max_force_n": 10.0,
   "transitions": [{"to": "capture", "all_of": {"distance_m": {"less_than": 10.0}}}]},
  {"id": "capture", "max_velocity_mps": 1.0, "max_force_n": 10.0,
   "transitions": [{"to": "hold", "all_of": {"time_in_phase_s": {"greater_than": 90.0}}}]},
  {"id": "hold", "max_velocity_mps": 0.0, "max_force_n": 10.0, "actuation": "hold",
   "end": "success"}
]}"#;

#[test]
fn transition_fires_on_time_in_phase_and_a_phase_entry_resets_the_clock() {
    // P7.5 S9 (d), design D16: the time in phase is `state_elapsed_time_s`, advanced
    // by one step before the check.
    let mut app = test_app();
    insert_plan_json(&mut app, "time", TIME_PLAN);
    let rig = capture_rig(&mut app, "time");
    let set_elapsed = |app: &mut App, seconds: f64| {
        app.world_mut()
            .get_mut::<CaptureComponent>(rig.capture)
            .unwrap()
            .state_elapsed_time_s = seconds - PHYS_DT;
    };
    let elapsed = |app: &App| {
        app.world()
            .get::<CaptureComponent>(rig.capture)
            .unwrap()
            .state_elapsed_time_s
    };

    // A phase entry resets the clock: 1,000 s in `a`, then the range condition.
    set_elapsed(&mut app, 1_000.0);
    rig_set(&mut app, &rig, 5.0, 0.0);
    assert_eq!(
        rig_step(&mut app, &rig),
        vec![(
            "a".to_string(),
            "capture".to_string(),
            "distance 5.0 m < 10.0 m".to_string()
        )]
    );
    assert_eq!(elapsed(&app), 0.0, "the entry resets it");
    assert!(rig_step(&mut app, &rig).is_empty(), "one step into capture");
    assert_eq!(elapsed(&app), PHYS_DT);

    set_elapsed(&mut app, 89.99);
    assert!(rig_step(&mut app, &rig).is_empty(), "89.99 s");
    set_elapsed(&mut app, 90.01);
    assert_eq!(
        rig_step(&mut app, &rig),
        vec![(
            "capture".to_string(),
            "hold".to_string(),
            "time in phase 90.0 s > 90.0 s".to_string()
        )]
    );
    assert_eq!(elapsed(&app), 0.0);
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
    // The tail closes at the phase's shrink rate (0.5 m/s), so the circle law's
    // rate term is 0 and its radius term (21 m inside R + 1) is at full weight
    // (design D18; at rest the two cancel).
    place(
        &mut app,
        &rig,
        DVec3::new(0.0, 5.0, 0.0),
        DVec3::new(0.0, 0.5, 0.0),
    );

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
    // The tail is still driven with the current state's limits: 5 m inside its
    // 26 m sphere, pushed away along −Y.
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
