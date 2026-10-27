//! Characterization of the physics bubble (`physics_bubble_add_remove`): the
//! enable/disable thresholds and the F-07, F-23 and F-25 defects.
//!
//! Layout: the anchor A (`CameraTarget`) sits at the origin of the floating-origin
//! frame with an ISS orbit. Other bodies carry an orbit whose ECI state is A's plus
//! a known offset and zero relative velocity. Distances are `rb.position.length()`
//! (`propagation.rs:480`, `:505`).

use crate::anchor::CameraTarget;
use crate::orbit::cache::OrbitalCache;
use crate::orbit::{ISS_ORBIT, Orbital};
use crate::telemetry::LogLevel;
use crate::test_support::{
    LogTap, assert_close, assert_close_dvec3, enter_sim, fixed_epoch, fixed_propagator, kepler_eci,
    start_running, test_app, tick,
};
use crate::time::WorldTime;
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

/// The anchor: `CameraTarget`, at the origin, on the ISS orbit.
fn spawn_anchor(app: &mut App) -> Entity {
    let anchor = spawn_body(app, DVec3::ZERO, Some(DVec3::ZERO), false);
    app.world_mut().entity_mut(anchor).insert(CameraTarget);
    anchor
}

/// A dynamic sphere at `position` (floating-origin frame) with, if `eci_offset` is
/// given, an orbit at the ISS state plus that offset; `None` leaves the `Orbital`
/// without a propagator. Every body gets the same components, so bodies spawned
/// one after another iterate in spawn order. `disabled` adds `RigidBodyDisabled`
/// (a different table, which is iterated after the enabled bodies).
fn spawn_body(app: &mut App, position: DVec3, eci_offset: Option<DVec3>, disabled: bool) -> Entity {
    let orbital = Orbital {
        propagator: eci_offset.map(|offset| fixed_propagator(iss_eci_plus(offset))),
        ..default()
    };
    let entity = app
        .world_mut()
        .spawn((
            RigidBody::Dynamic,
            Collider::sphere(0.5),
            Position(position),
            Transform::from_translation(position.as_vec3()),
            LinearVelocity(DVec3::ZERO),
            orbital,
        ))
        .id();
    if disabled {
        app.world_mut().entity_mut(entity).insert(RigidBodyDisabled);
    }
    entity
}

fn is_disabled(app: &App, entity: Entity) -> bool {
    app.world().get::<RigidBodyDisabled>(entity).is_some()
}

fn position(app: &App, entity: Entity) -> DVec3 {
    app.world().get::<Position>(entity).unwrap().0
}

fn velocity(app: &App, entity: Entity) -> DVec3 {
    app.world().get::<LinearVelocity>(entity).unwrap().0
}

fn eci_of(app: &App, entity: Entity) -> Vector6<f64> {
    *app.world()
        .resource::<OrbitalCache>()
        .eci_states
        .get(&entity)
        .expect("no cached ECI state for the entity")
}

fn propagator(app: &App, entity: Entity) -> KeplerianPropagator {
    app.world()
        .get::<Orbital>(entity)
        .unwrap()
        .propagator
        .clone()
        .expect("propagator missing")
}

fn state_at_epoch(app: &App, entity: Entity) -> Vector6<f64> {
    let epoch = app.world().resource::<WorldTime>().epoch;
    propagator(app, entity).state_eci(epoch).unwrap()
}

/// ECI position within 1e-3 m and velocity within 1e-6 m/s (A-3).
#[track_caller]
fn assert_eci_close(actual: &Vector6<f64>, expected: &Vector6<f64>, what: &str) {
    for i in 0..6 {
        let tol = if i < 3 { 1e-3 } else { 1e-6 };
        assert_close(actual[i], expected[i], tol, &format!("{what}[{i}]"));
    }
}

/// `target + (position, velocity)` as an ECI state.
fn plus(state: &Vector6<f64>, position: DVec3, velocity: DVec3) -> Vector6<f64> {
    state
        + Vector6::new(
            position.x, position.y, position.z, velocity.x, velocity.y, velocity.z,
        )
}

/// `Debug` bubble log messages in `events`.
fn bubble_messages(events: &[(LogLevel, &'static str, String)]) -> Vec<String> {
    events
        .iter()
        .filter(|(level, source, message)| {
            *level == LogLevel::Debug
                && *source == "propagation"
                && (message.starts_with("Physics disabled")
                    || message.starts_with("Physics enabled"))
        })
        .map(|(_, _, message)| message.clone())
        .collect()
}

/// Entities with an `Orbital` and no `CameraTarget`, in the order the bubble system
/// iterates them.
fn bubble_iteration_order(app: &mut App) -> Vec<Entity> {
    app.world_mut()
        .query_filtered::<Entity, (With<Orbital>, Without<CameraTarget>)>()
        .iter(app.world())
        .collect()
}

#[test]
fn char_bubble_stays_enabled_at_109m() {
    let mut app = running_app();
    let anchor = spawn_anchor(&mut app);
    let body = spawn_body(
        &mut app,
        DVec3::new(109.0, 0.0, 0.0),
        Some(DVec3::new(109.0, 0.0, 0.0)),
        false,
    );

    let mut tap = LogTap::new(app.world());
    tick(&mut app);
    let events = tap.drain(app.world());

    // Precondition: the system had what it needs (both states cached) and the body
    // is in the bubble's iteration.
    eci_of(&app, anchor);
    eci_of(&app, body);
    assert_eq!(bubble_iteration_order(&mut app), vec![body]);
    assert!(!is_disabled(&app, body));
    assert!(bubble_messages(&events).is_empty());
    assert_close_dvec3(
        position(&app, body),
        DVec3::new(109.0, 0.0, 0.0),
        1e-9,
        "position",
    );
}

#[test]
fn char_bubble_disables_above_110m() {
    let mut app = running_app();
    let anchor = spawn_anchor(&mut app);
    let body = spawn_body(
        &mut app,
        DVec3::new(111.0, 0.0, 0.0),
        Some(DVec3::new(111.0, 0.0, 0.0)),
        false,
    );
    assert!(!is_disabled(&app, body));

    let mut tap = LogTap::new(app.world());
    tick(&mut app);
    let events = tap.drain(app.world());

    eci_of(&app, anchor);
    assert!(is_disabled(&app, body));
    // The log text always says `0 km` (F-26); do not "fix" it.
    assert_eq!(
        bubble_messages(&events),
        vec![format!(
            "Physics disabled for entity {body:?} (dist 0 km from target)"
        )]
    );
    // Its propagator was rebuilt from the anchor's state plus its relative state
    // (`propagation.rs:488-492`).
    let expected = plus(
        &eci_of(&app, anchor),
        DVec3::new(111.0, 0.0, 0.0),
        velocity(&app, body),
    );
    assert_eq!(velocity(&app, body), DVec3::ZERO);
    assert_eci_close(
        &state_at_epoch(&app, body),
        &expected,
        "rebuilt propagator state",
    );
}

#[test]
fn char_bubble_disabled_body_tracks_relative_state() {
    let mut app = running_app();
    let anchor = spawn_anchor(&mut app);
    let body = spawn_body(
        &mut app,
        DVec3::new(105.0, 0.0, 0.0),
        Some(DVec3::new(105.0, 0.0, 0.0)),
        true,
    );
    assert!(is_disabled(&app, body));

    let mut tap = LogTap::new(app.world());
    tick(&mut app);
    let events = tap.drain(app.world());

    // Still disabled (105 m is not below the 100 m enable radius) and silent.
    assert!(is_disabled(&app, body));
    assert!(bubble_messages(&events).is_empty());

    // Its Position/velocity are its cached ECI state minus the anchor's
    // (`propagation.rs:533-544`).
    let anchor_state = eci_of(&app, anchor);
    let body_state = eci_of(&app, body);
    let relative = body_state - anchor_state;
    assert_close_dvec3(
        position(&app, body),
        DVec3::new(relative[0], relative[1], relative[2]),
        1e-6,
        "relative position",
    );
    assert_close_dvec3(
        velocity(&app, body),
        DVec3::new(relative[3], relative[4], relative[5]),
        1e-6,
        "relative velocity",
    );
    // ... which is the 105 m offset it was given, up to two-body drift.
    assert_close_dvec3(
        position(&app, body),
        DVec3::new(105.0, 0.0, 0.0),
        1e-3,
        "relative position vs the nominal offset",
    );
}

#[test]
fn char_bubble_enables_below_100m_keeps_propagator() {
    let mut app = running_app();
    let anchor = spawn_anchor(&mut app);
    let body = spawn_body(
        &mut app,
        DVec3::new(99.0, 0.0, 0.0),
        Some(DVec3::new(99.0, 0.0, 0.0)),
        true,
    );
    assert!(is_disabled(&app, body));
    let before = propagator(&app, body);

    let mut tap = LogTap::new(app.world());
    tick(&mut app);
    let events = tap.drain(app.world());

    eci_of(&app, anchor);
    assert!(!is_disabled(&app, body));
    assert_eq!(
        bubble_messages(&events),
        vec![format!(
            "Physics enabled for entity {body:?} (dist 0 km from target)"
        )]
    );

    // F-07: current behaviour, expected to change in P4
    // The propagator was not rebuilt (`propagation.rs:515` is commented out), so it
    // still gives the pre-enable state.
    let epoch = app.world().resource::<WorldTime>().epoch;
    assert_eq!(
        propagator(&app, body).state_eci(epoch).unwrap(),
        before.state_eci(epoch).unwrap()
    );
}

/// Anchor, an optional stateless body M spawned first, then an enabled body B at
/// 111 m. Returns `(app, B)`.
fn missing_state_scenario(with_m: bool) -> (App, Entity) {
    let mut app = running_app();
    let anchor = spawn_anchor(&mut app);
    let m = with_m.then(|| spawn_body(&mut app, DVec3::new(20.0, 0.0, 0.0), None, false));
    let b = spawn_body(
        &mut app,
        DVec3::new(111.0, 0.0, 0.0),
        Some(DVec3::new(111.0, 0.0, 0.0)),
        false,
    );

    // Preconditions: B has a cached state to process and M, if any, is iterated
    // before B (the loop's `return` then skips B).
    tick_precondition(&mut app, anchor, m, b);
    (app, b)
}

fn tick_precondition(app: &mut App, _anchor: Entity, m: Option<Entity>, b: Entity) {
    let order = bubble_iteration_order(app);
    match m {
        Some(m) => assert_eq!(order, vec![m, b], "M must be iterated before B"),
        None => assert_eq!(order, vec![b]),
    }
}

#[test]
fn char_bubble_missing_state_stops_loop() {
    // Control: without M, B (111 m) is disabled.
    let (mut app, b) = missing_state_scenario(false);
    let mut tap = LogTap::new(app.world());
    tick(&mut app);
    let events = tap.drain(app.world());
    assert!(is_disabled(&app, b));
    assert_eq!(bubble_messages(&events).len(), 1);

    // With M first, whose `Orbital` has no propagator and so no cached state, the
    // `return` at `propagation.rs:475` ends the loop before B is reached.
    let (mut app, b) = missing_state_scenario(true);
    let mut tap = LogTap::new(app.world());
    tick(&mut app);
    let events = tap.drain(app.world());
    // F-07: current behaviour, expected to change in P4
    assert!(!is_disabled(&app, b));
    // F-07: current behaviour, expected to change in P4
    assert!(bubble_messages(&events).is_empty());
}

#[test]
fn char_bubble_shared_target_rv_accumulates() {
    let mut app = running_app();
    let anchor = spawn_anchor(&mut app);
    let b1 = spawn_body(
        &mut app,
        DVec3::new(111.0, 0.0, 0.0),
        Some(DVec3::new(111.0, 0.0, 0.0)),
        false,
    );
    let b2 = spawn_body(
        &mut app,
        DVec3::new(0.0, 111.0, 0.0),
        Some(DVec3::new(0.0, 111.0, 0.0)),
        false,
    );
    // B3 is already disabled, 300 m from the anchor.
    let b3 = spawn_body(
        &mut app,
        DVec3::new(0.0, 0.0, 300.0),
        Some(DVec3::new(0.0, 0.0, 300.0)),
        true,
    );
    // Precondition: the loop visits B1, B2, then B3.
    assert_eq!(bubble_iteration_order(&mut app), vec![b1, b2, b3]);

    let mut tap = LogTap::new(app.world());
    tick(&mut app);
    let events = tap.drain(app.world());

    assert!(is_disabled(&app, b1));
    assert!(is_disabled(&app, b2));
    assert!(is_disabled(&app, b3));
    assert_eq!(bubble_messages(&events).len(), 2);

    let anchor_state = eci_of(&app, anchor);
    // B1's propagator is rebuilt from the anchor's state plus B1's offset ...
    assert_eci_close(
        &state_at_epoch(&app, b1),
        &plus(&anchor_state, DVec3::new(111.0, 0.0, 0.0), DVec3::ZERO),
        "B1 propagator state",
    );
    // F-23: current behaviour, expected to change in P4
    // ... but B2's includes B1's offset as well: the shared `target_rv` accumulates
    // (`propagation.rs:481-486`).
    assert_eci_close(
        &state_at_epoch(&app, b2),
        &plus(&anchor_state, DVec3::new(111.0, 111.0, 0.0), DVec3::ZERO),
        "B2 propagator state",
    );
    // F-23: current behaviour, expected to change in P4
    // B3 (already disabled, visited after B1 and B2) gets its relative Position
    // against the accumulated `target_rv`, not against the anchor.
    let relative =
        eci_of(&app, b3) - plus(&anchor_state, DVec3::new(111.0, 111.0, 0.0), DVec3::ZERO);
    assert_close_dvec3(
        position(&app, b3),
        DVec3::new(relative[0], relative[1], relative[2]),
        1e-6,
        "B3 relative position",
    );
    assert_close_dvec3(
        position(&app, b3),
        DVec3::new(-111.0, -111.0, 300.0),
        1e-3,
        "B3 relative position vs nominal",
    );
}

#[test]
fn char_bubble_keys_on_camera_target() {
    let mut app = running_app();
    let anchor_a = spawn_anchor(&mut app);
    // B: disabled, 300 m from A along x. T: disabled, 200 m from A along -x.
    let body_b = spawn_body(
        &mut app,
        DVec3::new(300.0, 0.0, 0.0),
        Some(DVec3::new(300.0, 0.0, 0.0)),
        true,
    );
    let body_t = spawn_body(
        &mut app,
        DVec3::new(-200.0, 0.0, 0.0),
        Some(DVec3::new(-200.0, 0.0, 0.0)),
        true,
    );

    tick(&mut app);
    // Relative to A (the anchor), B is at ~(300, 0, 0).
    assert_close_dvec3(
        position(&app, body_b),
        DVec3::new(300.0, 0.0, 0.0),
        1e-3,
        "B relative to A",
    );
    assert!(is_disabled(&app, body_b));
    assert!(is_disabled(&app, body_t));

    // Move `CameraTarget` from A to T, as `orbit_camera_switch_target` does.
    app.world_mut()
        .entity_mut(anchor_a)
        .remove::<CameraTarget>();
    app.world_mut().entity_mut(body_t).insert(CameraTarget);

    let mut tap = LogTap::new(app.world());
    tick(&mut app);
    let events = tap.drain(app.world());

    // F-25: current behaviour, expected to change in P4
    // The bubble is now relative to T: B is ~500 m from T.
    assert_close_dvec3(
        position(&app, body_b),
        DVec3::new(500.0, 0.0, 0.0),
        1e-3,
        "B relative to T",
    );
    assert_close_dvec3(
        position(&app, body_b),
        {
            let r = eci_of(&app, body_b) - eci_of(&app, body_t);
            DVec3::new(r[0], r[1], r[2])
        },
        1e-6,
        "B relative to T (from the cache)",
    );

    // OBSERVED, differs from the plan: the plan predicted that A, "now 200 m from T
    // and enabled", gets disabled. The bubble measures `rb.position.length()` in the
    // frame of the old anchor, where A is still at the origin, so A stays enabled
    // and silent.
    // F-25: current behaviour, expected to change in P4
    assert!(!is_disabled(&app, anchor_a));
    assert_close_dvec3(position(&app, anchor_a), DVec3::ZERO, 1e-9, "A position");
    assert!(bubble_messages(&events).is_empty());
}
