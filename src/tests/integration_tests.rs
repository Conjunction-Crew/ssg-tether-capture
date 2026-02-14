use std::time::Duration;

use crate::components::orbit_camera::OrbitCamera;
use crate::create_app;
use avian3d::collision::CollisionDiagnostics;
use avian3d::dynamics::solver::SolverDiagnostics;
use avian3d::prelude::*;
use bevy::ecs::relationship::RelationshipSourceCollection;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use bevy::{input::InputPlugin, scene::ScenePlugin};

// Minimal test app harness for unit testing
fn test_app() -> App {
    let mut app = create_app();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin::default(),
        InputPlugin,
        ScenePlugin,
    ))
    .init_asset::<Mesh>()
    .init_asset::<StandardMaterial>()
    .init_resource::<CollisionDiagnostics>()
    .init_resource::<SpatialQueryDiagnostics>()
    .init_resource::<SolverDiagnostics>();
    app
}

#[test]
fn minimal_rigidbody_setup() {
    let mut app = test_app();
    let world = app.world_mut();

    let test_sphere_mesh = Mesh::from(Sphere::new(1.0));

    let sphere_body = world.spawn((
        RigidBody::Dynamic,
        Collider::convex_hull_from_mesh(&test_sphere_mesh).unwrap(),
        Transform::from_xyz(40.0, 40.0, 40.0),
    )).id();

    assert!(!sphere_body.is_empty());

    app.update();
}

#[test]
fn orbit_camera() {
    let mut app = test_app();
    let world = app.world_mut();

    let orbit_camera = world.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 0.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y),
        OrbitCamera::default(),
    )).id();

    assert!(!orbit_camera.is_empty());

    app.update();
}

#[test]
fn apply_force_to_target() {
    let mut app = test_app();
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(1.0 / 60.0)));

    let test_sphere_mesh = Mesh::from(Sphere::new(1.0));

    let sphere_body = app.world_mut().spawn((
        RigidBody::Dynamic,
        ConstantForce::new(0.0, 0.0, 0.0),
        Collider::convex_hull_from_mesh(&test_sphere_mesh).unwrap(),
        Transform::from_xyz(40.0, 40.0, 40.0),
    )).id();

    app.world_mut().spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 0.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y),
        OrbitCamera { target: Some(sphere_body), ..default() },
    ));

    app.update();

    // Expect initial velocity of sphere to be zero
    assert_eq!(app.world_mut().get::<LinearVelocity>(sphere_body).unwrap().0, Vec3::ZERO);

    app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::KeyW);

    // Need two updates to actually alter the velocity
    app.update();
    app.update();

    // Expect sphere velocity to have changed
    assert_ne!(app.world_mut().get::<LinearVelocity>(sphere_body).unwrap().0, Vec3::ZERO);
}