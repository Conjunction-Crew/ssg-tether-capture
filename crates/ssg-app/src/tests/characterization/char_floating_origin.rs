//! Characterization of the floating-origin reset (`target_entity_reset_origin`).
//!
//! Constants: `MAX_ORIGIN_OFFSET = 50`, `PHYSICS_ENABLE_RADIUS = 100`,
//! `PHYSICS_DISABLE_RADIUS = 110` (`src/constants.rs`).

use crate::sim::anchor::CameraTarget;
use crate::sim::floating_origin::MAX_ORIGIN_OFFSET;
use crate::sim::orbit::cache::OrbitalCache;
use crate::sim::orbit::{ISS_ORBIT, Orbital};
use crate::sim::telemetry::LogLevel;
use crate::sim::tether::TetherRoot;
use crate::sim::time::{PHYS_DT, WorldTime};
use crate::tests::test_support::{
    LogTap, assert_close, assert_close_dvec3, enter_sim, fixed_epoch, fixed_propagator, kepler_eci,
    start_running, test_app, tick,
};
use avian3d::prelude::*;
use bevy::math::DVec3;
use bevy::prelude::*;
use brahe::KeplerianPropagator;
use brahe::utils::DOrbitStateProvider;
use nalgebra::Vector6;

/// A harness app in `SimState::Running`.
fn running_app() -> App {
    let mut app = test_app();
    enter_sim(&mut app);
    start_running(&mut app);
    app
}

/// The ISS state at the fixed epoch plus `offset` metres (position only).
fn iss_eci_plus(offset: DVec3) -> Vector6<f64> {
    kepler_eci(ISS_ORBIT, fixed_epoch()) + Vector6::new(offset.x, offset.y, offset.z, 0.0, 0.0, 0.0)
}

/// A dynamic sphere with an orbit, at `position` (in the floating-origin frame),
/// moving at `velocity`. The `Transform` matches the `Position` so avian does not
/// sync the position back to the origin on the first step.
fn spawn_body(app: &mut App, position: DVec3, velocity: DVec3, offset_eci: DVec3) -> Entity {
    app.world_mut()
        .spawn((
            RigidBody::Dynamic,
            Collider::sphere(0.5),
            Position(position),
            Transform::from_translation(position.as_vec3()),
            LinearVelocity(velocity),
            Orbital {
                propagator: Some(fixed_propagator(iss_eci_plus(offset_eci))),
                ..default()
            },
        ))
        .id()
}

fn orbital_propagator(app: &App, entity: Entity) -> KeplerianPropagator {
    app.world()
        .get::<Orbital>(entity)
        .expect("Orbital missing")
        .propagator
        .clone()
        .expect("propagator missing")
}

fn position(app: &App, entity: Entity) -> DVec3 {
    app.world().get::<Position>(entity).unwrap().0
}

fn velocity(app: &App, entity: Entity) -> DVec3 {
    app.world().get::<LinearVelocity>(entity).unwrap().0
}

fn com_rv(app: &App, entity: Entity) -> Option<(DVec3, DVec3)> {
    app.world()
        .resource::<OrbitalCache>()
        .com_rv
        .get(&entity)
        .copied()
}

/// Number of "Floating origin reset" Debug events in `events`.
fn reset_events(events: &[(LogLevel, &'static str, String)]) -> usize {
    events
        .iter()
        .filter(|(level, source, message)| {
            *level == LogLevel::Debug
                && *source == "propagation"
                && message.starts_with("Floating origin reset (epoch: ")
        })
        .count()
}

/// ECI position within 1e-3 m and velocity within 1e-6 m/s (A-3).
#[track_caller]
fn assert_eci_close(actual: &Vector6<f64>, expected: &Vector6<f64>, what: &str) {
    for i in 0..6 {
        let tol = if i < 3 { 1e-3 } else { 1e-6 };
        assert_close(actual[i], expected[i], tol, &format!("{what}[{i}]"));
    }
}

#[test]
fn char_origin_no_reset_at_max_offset() {
    let mut app = running_app();
    let anchor = spawn_body(
        &mut app,
        DVec3::new(MAX_ORIGIN_OFFSET, 0.0, 0.0),
        DVec3::ZERO,
        DVec3::ZERO,
    );
    app.world_mut()
        .entity_mut(anchor)
        .insert((CameraTarget, TetherRoot));
    let before = orbital_propagator(&app, anchor);

    let mut tap = LogTap::new(app.world());
    tick(&mut app);
    let events = tap.drain(app.world());

    // Precondition: the centre of mass was computed and sits exactly at the limit.
    let (com_r, _) = com_rv(&app, anchor).expect("com_rv has no entry for the anchor");
    assert_close(com_r.length(), MAX_ORIGIN_OFFSET, 1e-9, "|com_r|");

    // The comparison is `<=` (`propagation.rs:422`), so nothing resets.
    assert_eq!(reset_events(&events), 0);
    assert_close_dvec3(
        position(&app, anchor),
        DVec3::new(MAX_ORIGIN_OFFSET, 0.0, 0.0),
        1e-9,
        "anchor position",
    );
    // The propagator was not rebuilt: it still gives the old state at the new epoch.
    let epoch = app.world().resource::<WorldTime>().epoch;
    let after = orbital_propagator(&app, anchor);
    assert_eq!(
        after.state_eci(epoch).unwrap(),
        before.state_eci(epoch).unwrap()
    );
}

#[test]
fn char_origin_reset_above_max_offset() {
    let mut app = running_app();
    let anchor = spawn_body(
        &mut app,
        DVec3::new(50.5, 0.0, 0.0),
        DVec3::new(0.25, 0.0, 0.0),
        DVec3::ZERO,
    );
    app.world_mut()
        .entity_mut(anchor)
        .insert((CameraTarget, TetherRoot));
    // B: an enabled body with an orbit, 10 m from the origin.
    let body_b = spawn_body(
        &mut app,
        DVec3::new(10.0, 0.0, 0.0),
        DVec3::ZERO,
        DVec3::new(-40.5, 0.0, 0.0),
    );
    // C: a disabled body without an orbit.
    let body_c = app
        .world_mut()
        .spawn((
            RigidBody::Dynamic,
            Collider::sphere(0.5),
            Position::from_xyz(30.0, 0.0, 0.0),
            Transform::from_xyz(30.0, 0.0, 0.0),
            RigidBodyDisabled,
        ))
        .id();

    let anchor_before = orbital_propagator(&app, anchor);
    let b_before = orbital_propagator(&app, body_b);
    let c_position_before = position(&app, body_c);

    let mut tap = LogTap::new(app.world());
    tick(&mut app);
    let events = tap.drain(app.world());

    // Precondition: the system ran, i.e. a centre of mass exists for the anchor and
    // the reset event was logged exactly once.
    let (com_r, com_v) = com_rv(&app, anchor).expect("com_rv has no entry for the anchor");
    assert_eq!(reset_events(&events), 1);

    // F-13: order-dependent (ManualPhysics chain in PhysicsSystems::Last)
    // The reset runs after the avian step, so `com_r` includes this tick's motion.
    assert_close(com_r.x, 50.5 + 0.25 * PHYS_DT, 1e-9, "com_r.x");
    assert_close(com_v.x, 0.25, 1e-9, "com_v.x");

    // The anchor is back at the origin, at rest.
    assert_close_dvec3(position(&app, anchor), DVec3::ZERO, 1e-9, "anchor position");
    assert_close_dvec3(velocity(&app, anchor), DVec3::ZERO, 1e-9, "anchor velocity");

    // B moved by -com_r / -com_v; C (disabled) did not move.
    assert_close_dvec3(
        position(&app, body_b),
        DVec3::new(10.0, 0.0, 0.0) - com_r,
        1e-9,
        "B position",
    );
    assert_close_dvec3(velocity(&app, body_b), -com_v, 1e-9, "B velocity");
    assert_close_dvec3(
        position(&app, body_c),
        c_position_before,
        1e-9,
        "C position",
    );

    // Each enabled orbital's propagator state at the epoch is its old state plus
    // (com_r, com_v); the shift is the same for both.
    let epoch = app.world().resource::<WorldTime>().epoch;
    let shift = Vector6::new(com_r.x, com_r.y, com_r.z, com_v.x, com_v.y, com_v.z);
    for (entity, old, label) in [(anchor, anchor_before, "anchor"), (body_b, b_before, "B")] {
        let expected = old.state_eci(epoch).unwrap() + shift;
        let actual = orbital_propagator(&app, entity).state_eci(epoch).unwrap();
        assert_eci_close(&actual, &expected, &format!("{label} propagator state"));
    }
    // C has no orbit and gets none.
    assert!(app.world().get::<Orbital>(body_c).is_none());
    assert!(app.world().get::<RigidBodyDisabled>(body_c).is_some());
}

#[test]
fn char_origin_requires_tether_root() {
    let mut app = running_app();
    // `CameraTarget` without `TetherRoot`, beyond the max offset. The `Transform`
    // matches, so the position stays at 60 m and a missing reset is visible.
    let anchor = spawn_body(
        &mut app,
        DVec3::new(60.0, 0.0, 0.0),
        DVec3::ZERO,
        DVec3::ZERO,
    );
    app.world_mut().entity_mut(anchor).insert(CameraTarget);
    assert!(position(&app, anchor).length() > MAX_ORIGIN_OFFSET);

    let mut tap = LogTap::new(app.world());
    tick(&mut app);
    let events = tap.drain(app.world());

    // F-25: current behaviour, expected to change in P4
    assert!(
        app.world().resource::<OrbitalCache>().com_rv.is_empty(),
        "com_rv has entries without a TetherRoot"
    );
    // F-25: current behaviour, expected to change in P4
    assert_eq!(reset_events(&events), 0);
    assert_close_dvec3(
        position(&app, anchor),
        DVec3::new(60.0, 0.0, 0.0),
        1e-9,
        "anchor position",
    );
}

#[test]
fn char_origin_keys_on_camera_target() {
    let mut app = running_app();
    let body_a = spawn_body(
        &mut app,
        DVec3::new(60.0, 0.0, 0.0),
        DVec3::ZERO,
        DVec3::ZERO,
    );
    let body_b = spawn_body(
        &mut app,
        DVec3::new(10.0, 0.0, 0.0),
        DVec3::ZERO,
        DVec3::ZERO,
    );
    app.world_mut().entity_mut(body_a).insert(TetherRoot);
    app.world_mut()
        .entity_mut(body_b)
        .insert((TetherRoot, CameraTarget));

    let mut tap = LogTap::new(app.world());
    tick(&mut app);
    let events = tap.drain(app.world());

    // Precondition: both roots have a centre of mass (so A's 60 m would reset if
    // it were the target), and B (10 m) is the target.
    assert_close(
        com_rv(&app, body_a).unwrap().0.length(),
        60.0,
        1e-9,
        "|com_r| A",
    );
    assert_close(
        com_rv(&app, body_b).unwrap().0.length(),
        10.0,
        1e-9,
        "|com_r| B",
    );
    // F-25: current behaviour, expected to change in P4
    assert_eq!(reset_events(&events), 0);
    assert_close_dvec3(
        position(&app, body_a),
        DVec3::new(60.0, 0.0, 0.0),
        1e-9,
        "A position before the move",
    );

    // Move `CameraTarget` from B to A, as `orbit_camera_switch_target` does.
    app.world_mut().entity_mut(body_b).remove::<CameraTarget>();
    app.world_mut().entity_mut(body_a).insert(CameraTarget);

    let mut tap = LogTap::new(app.world());
    tick(&mut app);
    let events = tap.drain(app.world());

    // F-25: current behaviour, expected to change in P4
    assert_eq!(reset_events(&events), 1);
    assert_close_dvec3(
        position(&app, body_a),
        DVec3::ZERO,
        1e-9,
        "A after the reset",
    );
    assert_close_dvec3(
        position(&app, body_b),
        DVec3::new(10.0 - 60.0, 0.0, 0.0),
        1e-9,
        "B after the reset",
    );
}
