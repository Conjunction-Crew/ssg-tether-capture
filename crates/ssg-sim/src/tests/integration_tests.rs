use crate::anchor::SimAnchor;
use crate::capture::CaptureComponent;
use crate::capture::plan::CapturePlanLibrary;
use crate::floating_origin::MAX_ORIGIN_OFFSET;
use crate::orbit::cache::OrbitalCache;
use crate::orbit::{ISS_ORBIT, Orbit, Orbital};
use crate::test_support::{
    assert_close, assert_close_dvec3, enter_sim, load_example_plans, log_test_app, start_running,
    test_app, tick,
};
use crate::tether::TetherRoot;
use crate::time::WorldTime;
use avian3d::prelude::*;
use bevy::math::DVec3;
use bevy::prelude::*;
use brahe::utils::DOrbitStateProvider;

#[test]
fn minimal_rigidbody_setup() {
    let mut app = test_app();
    let world = app.world_mut();

    let test_sphere_mesh = Mesh::from(Sphere::new(1.0));

    let sphere_body = world
        .spawn((
            RigidBody::Dynamic,
            Collider::convex_hull_from_mesh(&test_sphere_mesh).unwrap(),
            Transform::from_xyz(40.0, 40.0, 40.0),
        ))
        .id();

    app.update();

    // Avian computes the body's mass from its collider ...
    let mass = app
        .world()
        .get::<ComputedMass>(sphere_body)
        .expect("rigid body has no ComputedMass");
    assert!(mass.value() > 0.0, "computed mass is {}", mass.value());

    // ... and syncs the Position from the Transform.
    assert_close_dvec3(
        app.world().get::<Position>(sphere_body).unwrap().0,
        DVec3::new(40.0, 40.0, 40.0),
        1e-9,
        "rigid body position",
    );
}

#[test]
fn apply_force_to_target() {
    let mut app = test_app();

    enter_sim(&mut app);
    start_running(&mut app);

    let test_sphere_mesh = Mesh::from(Sphere::new(1.0));

    let sphere_body = app
        .world_mut()
        .spawn((
            SimAnchor,
            RigidBody::Dynamic,
            Collider::convex_hull_from_mesh(&test_sphere_mesh).unwrap(),
            Transform::from_xyz(0.0, 0.0, 0.0),
        ))
        .id();

    let capture_body = app
        .world_mut()
        .spawn((
            RigidBody::Dynamic,
            Collider::convex_hull_from_mesh(&test_sphere_mesh).unwrap(),
            Transform::from_xyz(40.0, 40.0, 40.0),
        ))
        .id();

    app.world_mut()
        .resource_mut::<OrbitalCache>()
        .tethers
        .insert("Tether1".to_string(), vec![sphere_body]);

    app.update();
    tick(&mut app);

    // Expect initial velocity of sphere to be zero
    assert_eq!(
        app.world_mut()
            .get::<LinearVelocity>(sphere_body)
            .unwrap()
            .0,
        DVec3::ZERO
    );

    // Load and compile capture plans
    load_example_plans(&mut app);

    // Get plan information
    let capture_plan_lib = app.world().resource::<CapturePlanLibrary>();
    let plan_res = capture_plan_lib.plans.get("example_plan");
    assert!(plan_res.is_some());
    let plan = plan_res.unwrap().clone();

    // Now, mark the entity for capture
    app.world_mut()
        .entity_mut(capture_body)
        .insert(CaptureComponent {
            plan_id: plan.id.clone(),
            current_state: plan
                .states
                .first()
                .expect("No states in the desired plan!")
                .id
                .clone(),
            state_enter_time_s: 0.0,
            state_elapsed_time_s: 0.0,
        });

    // Need a few updates to actually alter the velocity
    tick(&mut app);
    tick(&mut app);
    tick(&mut app);
    tick(&mut app);

    // Expect sphere velocity to have changed
    assert_ne!(
        app.world_mut()
            .get::<LinearVelocity>(sphere_body)
            .unwrap()
            .0,
        DVec3::ZERO
    );
}

#[test]
fn orbit_propagation() {
    let mut app = test_app();

    enter_sim(&mut app);
    start_running(&mut app);

    let test_sphere_mesh = Mesh::from(Sphere::new(1.0));

    let sphere_body = app
        .world_mut()
        .spawn((
            SimAnchor,
            RigidBody::Dynamic,
            Collider::convex_hull_from_mesh(&test_sphere_mesh).unwrap(),
            Transform::from_xyz(40.0, 40.0, 40.0),
            Orbit::FromElements(ISS_ORBIT),
        ))
        .id();

    app.update();

    let params_before = {
        let orbital_o = app.world().get::<Orbital>(sphere_body);
        assert!(orbital_o.is_some());
        let world_time = app.world().get_resource::<WorldTime>().unwrap();
        let propagator_o = orbital_o.unwrap().clone().propagator;
        assert!(propagator_o.is_some());
        propagator_o.unwrap().state_eci(world_time.epoch).unwrap()
    };

    tick(&mut app);

    let params_after = {
        let orbital_o = app.world().get::<Orbital>(sphere_body);
        assert!(orbital_o.is_some());
        let world_time = app.world().get_resource::<WorldTime>().unwrap();
        let propagator_o = orbital_o.unwrap().clone().propagator;
        assert!(propagator_o.is_some());
        propagator_o.unwrap().state_eci(world_time.epoch).unwrap()
    };

    // Expect true orbital positions to have updated
    assert_ne!(params_before, params_after);
}

#[test]
fn floating_origin_resets() {
    let mut app = test_app();

    enter_sim(&mut app);
    start_running(&mut app);

    // `TetherRoot` is what makes `calculate_com_rv` record a centre of mass for the
    // body; without it the floating origin never fires (F-14, D9).
    let sphere_body = app
        .world_mut()
        .spawn((
            SimAnchor,
            TetherRoot,
            RigidBody::Dynamic,
            // An analytic sphere has its centre of mass exactly at the origin; the convex
            // hull of the sphere mesh is ~3.5e-9 m off, which would break the 1e-9 m checks.
            Collider::sphere(1.0),
            Position::from_xyz(0.0, 0.0, 0.0),
            Orbit::FromElements(ISS_ORBIT),
        ))
        .id();

    let current_transform_o = app.world().get::<Position>(sphere_body);
    assert!(current_transform_o.is_some());
    let current_transform = *current_transform_o.unwrap();

    tick(&mut app);

    let new_transform_o = app.world().get::<Position>(sphere_body);
    assert!(new_transform_o.is_some());
    let new_transform = *new_transform_o.unwrap();

    // Expect position to not have updated
    assert_eq!(current_transform, new_transform);

    // (a) Exactly at the max origin offset nothing moves: the check is `<=`.
    app.world_mut()
        .entity_mut(sphere_body)
        .insert(Position::from_xyz(MAX_ORIGIN_OFFSET, 0.0, 0.0));

    tick(&mut app);

    assert_close_dvec3(
        app.world().get::<Position>(sphere_body).unwrap().0,
        DVec3::new(MAX_ORIGIN_OFFSET, 0.0, 0.0),
        1e-9,
        "position at the max origin offset",
    );

    // (b) Move the object beyond the max origin offset
    app.world_mut()
        .entity_mut(sphere_body)
        .insert(Position::from_xyz(MAX_ORIGIN_OFFSET + 10.0, 0.0, 0.0));

    tick(&mut app);

    // The centre of mass was computed for the body, so the reset really fired ...
    let (com_r, _) = *app
        .world()
        .resource::<OrbitalCache>()
        .com_rv
        .get(&sphere_body)
        .expect("com_rv has no entry for the body, so the origin reset did not run");
    assert_close(
        com_r.x,
        MAX_ORIGIN_OFFSET + 10.0,
        1e-9,
        "centre of mass offset that was reset",
    );

    // ... and the body is back at the origin.
    let reset_position = app.world().get::<Position>(sphere_body).unwrap().0;
    assert_close_dvec3(
        reset_position,
        DVec3::ZERO,
        1e-9,
        "position after the reset",
    );
}

// ─── Capture Log integration tests (A4) ──────────────────────────────────────

use crate::telemetry::{CaptureLog, LogEvent, LogLevel};

#[test]
fn collect_log_events_appends_to_capture_log() {
    let mut app = log_test_app();

    // Emit one LogEvent via a startup system before the Update tick.
    app.add_systems(Startup, |mut writer: MessageWriter<LogEvent>| {
        writer.write(LogEvent {
            level: LogLevel::Info,
            source: "test",
            message: "hello".to_string(),
        });
    });

    app.update(); // Startup runs, then Update (collect_log_events) runs.

    let log = app.world().resource::<CaptureLog>();
    assert_eq!(log.entries.len(), 1);
    assert_eq!(log.entries[0].level, LogLevel::Info);
    assert_eq!(log.entries[0].source, "test");
    assert_eq!(log.entries[0].message, "hello");
}

#[test]
fn collect_log_events_respects_ring_buffer_limit() {
    let mut app = log_test_app();

    // Override default max_entries to something small for the test.
    app.world_mut().resource_mut::<CaptureLog>().max_entries = 5;

    app.add_systems(Startup, |mut writer: MessageWriter<LogEvent>| {
        for i in 0..10u32 {
            writer.write(LogEvent {
                level: LogLevel::Debug,
                source: "test",
                message: format!("msg {i}"),
            });
        }
    });

    app.update();

    let log = app.world().resource::<CaptureLog>();
    assert_eq!(
        log.entries.len(),
        5,
        "ring buffer should cap at max_entries"
    );
    assert_eq!(
        log.entries[0].message, "msg 5",
        "oldest entries should be evicted"
    );
    assert_eq!(log.entries[4].message, "msg 9");
}

/// No NaN reaches avian: the Q13 scenario runs 640 steps in a debug build, where
/// `assert_finite_state` and the controller's `debug_assert!` would panic.
#[test]
fn sim_state_is_finite() {
    let run = crate::test_support::q13::run(crate::test_support::q13::EpochMode::Fixed, 640, false);
    assert!(run.final_sample.range_m.is_finite());
}
