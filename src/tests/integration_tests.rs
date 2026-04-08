use crate::components::capture_components::CaptureComponent;
use crate::components::orbit::{Orbit, Orbital};
use crate::components::orbit_camera::{CameraTarget, OrbitCamera, OrbitCameraParams};
use crate::constants::{ISS_ORBIT, MAX_ORIGIN_OFFSET};
use crate::create_app;
use crate::resources::capture_plans::CapturePlanLibrary;
use crate::resources::orbital_cache::OrbitalCache;
use crate::resources::world_time::{self, WorldTime};
use crate::systems::physics::fixed_physics_step;
use crate::ui::screens::home::load_capture_plans;
use crate::ui::state::UiScreen;
use avian3d::collider_tree::ColliderTreeDiagnostics;
use avian3d::collision::CollisionDiagnostics;
use avian3d::dynamics::solver::SolverDiagnostics;
use avian3d::prelude::*;
use bevy::ecs::relationship::RelationshipSourceCollection;
use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use bevy::{input::InputPlugin, scene::ScenePlugin};
use brahe::utils::DOrbitStateProvider;

// Minimal test app harness for unit testing
fn test_app() -> App {
    let mut app = create_app();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin::default(),
        InputPlugin,
        ScenePlugin,
        StatesPlugin,
    ))
    .init_state::<UiScreen>()
    .init_asset::<Mesh>()
    .init_asset::<StandardMaterial>()
    .init_asset::<GizmoAsset>()
    .init_resource::<CollisionDiagnostics>()
    .init_resource::<SpatialQueryDiagnostics>()
    .init_resource::<SolverDiagnostics>()
    .init_resource::<ColliderTreeDiagnostics>()
    .init_resource::<WorldTime>();
    app
}

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

    assert!(!sphere_body.is_empty());
}

#[test]
fn orbit_camera() {
    let mut app = test_app();
    let world = app.world_mut();

    let orbit_camera = world
        .spawn((
            Camera3d::default(),
            Transform::from_xyz(0.0, 0.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y),
            OrbitCamera {
                map_params: OrbitCameraParams::default(),
                scene_params: OrbitCameraParams::default(),
            },
        ))
        .id();

    assert!(!orbit_camera.is_empty());
}

#[test]
fn apply_force_to_target() {
    let mut app = test_app();

    let mut next_screen = app.world_mut().resource_mut::<NextState<UiScreen>>();
    next_screen.set(UiScreen::Sim);

    app.update();

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
    fixed_physics_step(app.world_mut());

    // Expect initial velocity of sphere to be zero
    assert_eq!(
        app.world_mut()
            .get::<LinearVelocity>(sphere_body)
            .unwrap()
            .0,
        DVec3::ZERO
    );

    // Load capture plans
    let mut capture_plan_lib = app.world_mut().resource_mut::<CapturePlanLibrary>();
    load_capture_plans(&mut capture_plan_lib);

    // Get plan information
    let plan_res = capture_plan_lib.plans.get("example_plan");
    assert!(plan_res.is_some());
    let plan = plan_res.unwrap().clone();

    // Now, mark the entity for capture
    app.world_mut().entity_mut(capture_body).insert(CaptureComponent {
        plan_id: plan.name.clone(),
        current_state: plan
            .states
            .get(0)
            .expect("No states in the desired plan!")
            .id
            .clone(),
        state_enter_time_s: 0.0,
        state_elapsed_time_s: 0.0,
    });

    // Need two updates to actually alter the velocity
    fixed_physics_step(app.world_mut());
    fixed_physics_step(app.world_mut());

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

    let mut next_screen = app.world_mut().resource_mut::<NextState<UiScreen>>();
    next_screen.set(UiScreen::Sim);

    app.update();

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

    fixed_physics_step(app.world_mut());

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

    let mut next_screen = app.world_mut().resource_mut::<NextState<UiScreen>>();
    next_screen.set(UiScreen::Sim);

    app.update();

    let test_sphere_mesh = Mesh::from(Sphere::new(1.0));

    let sphere_body = app
        .world_mut()
        .spawn((
            CameraTarget,
            RigidBody::Dynamic,
            Collider::convex_hull_from_mesh(&test_sphere_mesh).unwrap(),
            Position::from_xyz(0.0, 0.0, 0.0),
            Orbit::FromElements(ISS_ORBIT),
        ))
        .id();

    let current_transform_o = app.world().get::<Position>(sphere_body);
    assert!(current_transform_o.is_some());
    let current_transform = current_transform_o.unwrap().clone();

    fixed_physics_step(app.world_mut());

    let new_transform_o = app.world().get::<Position>(sphere_body);
    assert!(new_transform_o.is_some());
    let new_transform = new_transform_o.unwrap().clone();

    // Expect position to not have updated
    assert_eq!(current_transform, new_transform);

    // Move the object beyond the max origin offset
    app.world_mut()
        .entity_mut(sphere_body)
        .insert(Position::from_xyz(MAX_ORIGIN_OFFSET + 10.0, 0.0, 0.0));

    fixed_physics_step(app.world_mut());

    let reset_transform_o = app.world().get::<Position>(sphere_body);
    assert!(reset_transform_o.is_some());
    let reset_transform = reset_transform_o.unwrap().clone();

    // Expect position to have been reset
    assert!(reset_transform.length() < 1.0);
}
