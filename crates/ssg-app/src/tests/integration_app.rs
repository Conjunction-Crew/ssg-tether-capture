//! App-side integration tests (P3 S2): they need the camera plugin.

use crate::render::camera::{CameraTarget, OrbitCamera, OrbitCameraParams};
use crate::render::celestial::Earth;
use crate::render::floating_origin_visuals::floating_origin_update_visuals;
use crate::render::layers::{
    EARTH_TEXTURE_NORTH_AXIS, MAP_UNITS_TO_M, SCENE_LAYER, earth_mesh_rotation, eci_position_to_map,
};
use crate::tests::app_support::{enter_sim, start_running, test_app, tick};
use crate::ui::hud::capture_metrics;
use avian3d::prelude::*;
use bevy::camera::visibility::RenderLayers;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::DVec3;
use bevy::mesh::VertexAttributeValues;
use bevy::pbr::Atmosphere;
use bevy::prelude::*;
use nalgebra::Vector6;
use ssg_sim::anchor::SimAnchor;
use ssg_sim::orbit::cache::OrbitalCache;
use ssg_sim::orbit::lvlh::LvlhFrame;
use ssg_sim::orbit::{ISS_ORBIT, Orbital};
use ssg_sim::test_support::{disable_relative_gravity, fixed_epoch, fixed_propagator, kepler_eci};
use ssg_sim::tether::TetherRoot;

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

/// A running app with the Earth and a scene camera, so `orbit_camera_track` runs.
fn running_app_with_camera() -> (App, Entity) {
    let mut app = test_app();
    enter_sim(&mut app);
    start_running(&mut app);
    app.world_mut()
        .spawn((Earth, Transform::from_xyz(0.0, 0.0, -1000.0)));
    let camera = app
        .world_mut()
        .spawn((
            Camera3d::default(),
            RenderLayers::layer(SCENE_LAYER),
            Transform::default(),
            OrbitCamera {
                map_params: OrbitCameraParams::default(),
                scene_params: OrbitCameraParams::default(),
            },
        ))
        .id();
    (app, camera)
}

/// A dynamic sphere at `position` (bubble frame), at rest, with an orbit at the ISS
/// state plus `position`; `disabled` adds `RigidBodyDisabled`.
fn spawn_body(app: &mut App, position: DVec3, disabled: bool) -> Entity {
    let eci = kepler_eci(ISS_ORBIT, fixed_epoch())
        + Vector6::new(position.x, position.y, position.z, 0.0, 0.0, 0.0);
    let entity = app
        .world_mut()
        .spawn((
            RigidBody::Dynamic,
            Collider::sphere(0.5),
            Position(position),
            Transform::from_translation(position.as_vec3()),
            LinearVelocity(DVec3::ZERO),
            Orbital {
                propagator: Some(fixed_propagator(eci)),
                ..default()
            },
        ))
        .id();
    if disabled {
        app.world_mut().entity_mut(entity).insert(RigidBodyDisabled);
    }
    entity
}

fn move_camera_target(app: &mut App, from: Entity, to: Entity) {
    app.world_mut().entity_mut(from).remove::<CameraTarget>();
    app.world_mut().entity_mut(to).insert(CameraTarget);
}

fn positions(app: &App, entities: &[Entity]) -> Vec<DVec3> {
    entities
        .iter()
        .map(|&entity| app.world().get::<Position>(entity).unwrap().0)
        .collect()
}

fn scene_focus(app: &App, camera: Entity) -> Vec3 {
    app.world()
        .get::<OrbitCamera>(camera)
        .unwrap()
        .scene_params
        .focus
}

/// The anchor root R (`SimAnchor`, `TetherRoot`, 10 m) and a second tether root B
/// at 60 m, beyond the floating-origin limit. `move_camera` moves `CameraTarget`
/// from R to B after the first tick. Returns the positions and centres of mass
/// after the second tick, and the camera focus before and after the move.
fn origin_scenario(move_camera: bool) -> (Vec<DVec3>, Vec<(DVec3, DVec3)>, Vec3, Vec3) {
    let (mut app, camera) = running_app_with_camera();
    disable_relative_gravity(&mut app);
    let root = spawn_body(&mut app, DVec3::new(10.0, 0.0, 0.0), false);
    let body_b = spawn_body(&mut app, DVec3::new(60.0, 0.0, 0.0), false);
    app.world_mut()
        .entity_mut(root)
        .insert((SimAnchor, TetherRoot, CameraTarget));
    app.world_mut().entity_mut(body_b).insert(TetherRoot);

    tick(&mut app);
    let focus_before = scene_focus(&app, camera);
    if move_camera {
        move_camera_target(&mut app, root, body_b);
    }
    tick(&mut app);
    let focus_after = scene_focus(&app, camera);

    let cache = app.world().resource::<OrbitalCache>();
    let com = [root, body_b].iter().map(|e| cache.com_rv[e]).collect();
    (
        positions(&app, &[root, body_b]),
        com,
        focus_before,
        focus_after,
    )
}

#[test]
fn origin_ignores_camera_target() {
    // F-25: fixed in P4
    let (control_positions, control_com, _, _) = origin_scenario(false);
    let (positions, com, focus_before, focus_after) = origin_scenario(true);

    // The camera follows the move: from R (10 m) to B (60 m).
    assert_eq!(focus_before, Vec3::new(10.0, 0.0, 0.0));
    assert_eq!(focus_after, Vec3::new(60.0, 0.0, 0.0));
    // The sim does not: no reset on B's 60 m, the same state as without the move.
    assert_eq!(positions, control_positions);
    assert_eq!(com, control_com);
    assert_eq!(positions[1], DVec3::new(60.0, 0.0, 0.0));
}

/// The anchor A (`SimAnchor`, `CameraTarget`) at the origin, B disabled at 300 m and
/// T disabled at -200 m. `move_camera` moves `CameraTarget` from A to T after the
/// first tick. Returns the positions after the second tick.
fn bubble_scenario(move_camera: bool) -> Vec<DVec3> {
    let (mut app, _camera) = running_app_with_camera();
    let anchor = spawn_body(&mut app, DVec3::ZERO, false);
    app.world_mut()
        .entity_mut(anchor)
        .insert((SimAnchor, CameraTarget));
    let body_b = spawn_body(&mut app, DVec3::new(300.0, 0.0, 0.0), true);
    let body_t = spawn_body(&mut app, DVec3::new(-200.0, 0.0, 0.0), true);

    tick(&mut app);
    if move_camera {
        move_camera_target(&mut app, anchor, body_t);
    }
    tick(&mut app);
    assert_eq!(
        app.world().get::<CameraTarget>(body_t).is_some(),
        move_camera
    );
    positions(&app, &[anchor, body_b, body_t])
}

#[test]
fn bubble_ignores_camera_target() {
    // F-25: fixed in P4
    let control = bubble_scenario(false);
    let moved = bubble_scenario(true);

    // The bubble stays centred on the anchor A: B is still ~300 m from it, not
    // ~500 m from T, exactly as without the camera move.
    assert_eq!(moved, control);
    assert!(
        (moved[1] - DVec3::new(300.0, 0.0, 0.0)).length() < 1e-3,
        "B relative to A is {:?}",
        moved[1]
    );
}

#[test]
fn frame_eci_to_scene_round_trip() {
    // F-08: fixed in P4. The map is drawn in ECI axes: scaling only, no rotation.
    for v in [
        Vec3::new(6_524_834.0, -1_234_567.0, 2_345_678.0),
        Vec3::new(-3_000_000.0, 4_000_000.0, -5_000_000.0),
        Vec3::new(1_000.0, 2_000.0, 3_000.0),
    ] {
        let back = eci_position_to_map(v) * MAP_UNITS_TO_M as f32;
        assert!(
            (back - v).length() <= 1e-6 * v.length(),
            "{v:?} -> map -> {back:?}"
        );
    }
}

#[test]
fn scene_nadir_matches_physics_nadir() {
    // F-08: fixed in P4. The Earth is drawn on the target's physical nadir, -r̂.
    let mut app = test_app();
    enter_sim(&mut app);
    let r_t = DVec3::new(-2_801_000.0, 5_412_000.0, 3_300_000.0);
    let earth = app.world_mut().spawn((Earth, Transform::default())).id();
    let target = app
        .world_mut()
        .spawn((CameraTarget, Transform::from_xyz(3.0, -4.0, 5.0)))
        .id();
    app.world_mut().spawn((
        Camera3d::default(),
        RenderLayers::layer(SCENE_LAYER),
        Atmosphere {
            bottom_radius: 6_360_000.0,
            top_radius: 6_460_000.0,
            world_position: Vec3::ZERO,
            ground_albedo: Vec3::splat(0.3),
            medium: Handle::default(),
        },
    ));
    app.world_mut()
        .resource_mut::<OrbitalCache>()
        .eci_states
        .insert(target, Vector6::new(r_t.x, r_t.y, r_t.z, 0.0, 7_600.0, 0.0));

    app.world_mut()
        .run_system_once(floating_origin_update_visuals)
        .expect("floating_origin_update_visuals failed to run");

    let world = app.world_mut();
    let earth_translation = world.get::<Transform>(earth).unwrap().translation;
    let target_translation = world.get::<Transform>(target).unwrap().translation;
    let nadir = (earth_translation - target_translation).normalize();
    let expected = -r_t.normalize().as_vec3();
    assert!(
        (nadir - expected).length() < 1e-6,
        "scene nadir {nadir:?}, physical nadir {expected:?}"
    );
    let atmosphere = world
        .query::<&Atmosphere>()
        .single(world)
        .expect("one atmosphere")
        .clone();
    assert_eq!(atmosphere.world_position, earth_translation);
}

#[test]
fn earth_mesh_pole_is_eci_z() {
    // F-08: fixed in P4. The UV sphere's first vertex is its pole, texture row 0
    // (north); the Earth's mesh rotation must put it on ECI +Z.
    let mesh = Sphere::new(1.0).mesh().uv(8, 4);
    let Some(VertexAttributeValues::Float32x3(positions)) =
        mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        panic!("UV sphere without Float32x3 positions");
    };
    let Some(VertexAttributeValues::Float32x2(uvs)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0) else {
        panic!("UV sphere without Float32x2 UVs");
    };
    assert_eq!(uvs[0][1], 0.0, "vertex 0 is not on texture row 0");
    let pole = Vec3::from(positions[0]).normalize();

    assert!(
        (pole - EARTH_TEXTURE_NORTH_AXIS).length() < 1e-6,
        "mesh pole {pole:?}"
    );
    let rotated = earth_mesh_rotation() * pole;
    assert!(
        (rotated - Vec3::Z).length() < 1e-6,
        "Earth pole in the scene: {rotated:?}"
    );
}

/// The HUD metrics for a capture target at rest at the bubble origin on the ISS
/// orbit and a reference body 3,536 m along-track of it moving at
/// `reference_velocity(frame, offset)`: `(relative speed, closing speed, range,
/// |omega × offset|)`.
fn hud_metrics(reference_velocity: fn(&LvlhFrame, DVec3) -> DVec3) -> (f64, f64, f64, f64) {
    let mut app = test_app();
    let state = kepler_eci(ISS_ORBIT, fixed_epoch());
    let frame = LvlhFrame::from_eci(&state).unwrap();
    let offset = frame.along * 3_536.0;
    let target = app
        .world_mut()
        .spawn((
            RigidBody::Dynamic,
            Collider::sphere(1.0),
            Position(DVec3::ZERO),
            LinearVelocity::ZERO,
        ))
        .id();
    let reference = app
        .world_mut()
        .spawn((
            RigidBody::Dynamic,
            Collider::sphere(0.5),
            Position(offset),
            LinearVelocity(reference_velocity(&frame, offset)),
        ))
        .id();
    // Both enabled: their cached state is the frame origin, here the target's.
    let mut cache = OrbitalCache::default();
    cache.eci_states.insert(target, state);
    cache.eci_states.insert(reference, state);
    app.world_mut().insert_resource(cache);

    let metrics = app
        .world_mut()
        .run_system_once(
            move |bodies: Query<(RigidBodyQueryReadOnly, Has<RigidBodyDisabled>)>,
                  cache: Res<OrbitalCache>| {
                capture_metrics(&bodies, Some(target), Some(reference), &cache)
                    .map(|m| (m.relative_speed_m_s, m.closing_speed_m_s, m.range_m))
            },
        )
        .unwrap()
        .expect("metrics for two cached bodies");
    (
        metrics.0,
        metrics.1,
        metrics.2,
        frame.omega.cross(offset).length(),
    )
}

#[test]
fn hud_relative_speed_is_lvlh() {
    // F-35: fixed in P4. At rest in the target's LVLH frame: zero relative speed,
    // although the ECI velocities differ by |omega × r| ≈ 4 m/s.
    let (speed, closing, range, artefact) = hud_metrics(|frame, offset| frame.omega.cross(offset));
    assert!(artefact > 3.5, "precondition: ECI artefact {artefact} m/s");
    assert!(speed < 1e-9, "LVLH rest: relative speed {speed} m/s");
    assert!(
        closing.abs() < 1e-9,
        "LVLH rest: closing speed {closing} m/s"
    );
    assert!((range - 3_536.0).abs() < 1e-9, "range {range} m");

    // ECI-equal velocities: moving at |omega × r| relative to the LVLH frame; the
    // closing speed is unchanged (omega × r is perpendicular to r).
    let (speed, closing, _, artefact) = hud_metrics(|_, _| DVec3::ZERO);
    assert!(
        (speed - artefact).abs() < 1e-9,
        "ECI-equal: relative speed {speed} m/s, expected {artefact}"
    );
    assert!(
        closing.abs() < 1e-9,
        "ECI-equal: closing speed {closing} m/s"
    );
}
