use crate::render::camera::{OrbitCamera, OrbitCameraParams};
use crate::render::celestial::Earth;
use crate::render::layers::SCENE_LAYER;
use crate::sim::anchor::CameraTarget;
use crate::sim::capture::CaptureComponent;
use crate::sim::capture::plan::CapturePlanLibrary;
use crate::sim::floating_origin::MAX_ORIGIN_OFFSET;
use crate::sim::orbit::cache::OrbitalCache;
use crate::sim::orbit::{ISS_ORBIT, Orbit, Orbital};
use crate::sim::tether::TetherRoot;
use crate::sim::time::WorldTime;
use crate::tests::test_support::{
    assert_close, assert_close_dvec3, enter_sim, load_example_plans, log_test_app, start_running,
    test_app, tick,
};
use avian3d::prelude::*;
use bevy::camera::visibility::RenderLayers;
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
fn orbit_camera() {
    let mut app = test_app();
    enter_sim(&mut app);

    // `orbit_camera_track` needs the Earth, a target and a camera on the scene layer.
    // Earth is directly "below" the target (along -Z), so the scene camera's up vector
    // is +Z.
    app.world_mut()
        .spawn((Earth, Transform::from_xyz(10.0, 20.0, -1000.0)));
    app.world_mut()
        .spawn((CameraTarget, Transform::from_xyz(10.0, 20.0, 30.0)));
    let orbit_camera = app
        .world_mut()
        .spawn((
            Camera3d::default(),
            RenderLayers::layer(SCENE_LAYER),
            Transform::from_xyz(0.0, 0.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y),
            OrbitCamera {
                map_params: OrbitCameraParams::default(),
                scene_params: OrbitCameraParams::default(),
            },
        ))
        .id();

    tick(&mut app);

    // The camera tracks the target: focus = target translation, up = away from Earth.
    let camera = app.world().get::<OrbitCamera>(orbit_camera).unwrap();
    assert_eq!(camera.scene_params.focus, Vec3::new(10.0, 20.0, 30.0));
    assert_eq!(camera.scene_params.up, Vec3::Z);

    // With up = +Z, an unrotated Earth (north axis +Z) and zero yaw/pitch,
    // `orbit_camera_track` gives forward = +X, so the camera sits `distance` (30)
    // behind the focus: focus - X * 30.
    let translation = app
        .world()
        .get::<Transform>(orbit_camera)
        .unwrap()
        .translation;
    assert!(
        translation.abs_diff_eq(Vec3::new(-20.0, 20.0, 30.0), 1e-4),
        "camera translation is {translation:?}"
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
            CameraTarget,
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
            CameraTarget,
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
            CameraTarget,
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

use crate::sim::telemetry::{CaptureLog, LogEvent, LogLevel};

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
