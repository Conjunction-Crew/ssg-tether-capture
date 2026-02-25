use crate::{
    components::{
        orbit::{Earth, Orbital},
        orbit_camera::{CameraTarget, OrbitCamera},
    },
    constants::SCENE_LAYER,
};
use avian3d::prelude::*;
use bevy::{
    camera::visibility::RenderLayers,
    input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit},
    prelude::*,
};

pub fn orbit_camera_input(
    buttons: Res<ButtonInput<MouseButton>>,
    mouse_motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    s: Single<(&mut OrbitCamera, &mut Transform, &RenderLayers), With<Camera3d>>,
) {
    let (mut orbit_cameras, mut transform, render_layers) = s.into_inner();

    let delta = mouse_motion.delta;

    let scroll_y = match scroll.unit {
        MouseScrollUnit::Line => scroll.delta.y,
        MouseScrollUnit::Pixel => scroll.delta.y * 0.01,
    };

    let camera = if render_layers.intersects(&RenderLayers::layer(SCENE_LAYER)) {
        &mut orbit_cameras.scene_params
    } else {
        &mut orbit_cameras.map_params
    };

    if buttons.pressed(MouseButton::Right) && delta != Vec2::ZERO {
        camera.yaw -= delta.x * camera.sensitivity;
        camera.pitch -= delta.y * camera.sensitivity;

        camera.pitch = camera.pitch.clamp(-camera.max_pitch, camera.max_pitch)
    }

    if scroll_y != 0.0 {
        camera.distance -= scroll_y;
        camera.distance = camera
            .distance
            .clamp(camera.min_distance, camera.max_distance);
    }
}

pub fn orbit_camera_track(
    targets: Query<&Transform, (With<CameraTarget>, Without<Camera3d>, Without<Earth>)>,
    cam_q: Single<
        (&mut OrbitCamera, &mut Transform, &RenderLayers),
        (With<Camera3d>, Without<Earth>),
    >,
    earth: Single<&Transform, With<Earth>>,
) {
    let (mut orbit_camera, mut transform, render_layers) = cam_q.into_inner();
    let camera = if render_layers.intersects(&RenderLayers::layer(SCENE_LAYER)) {
        &mut orbit_camera.scene_params
    } else {
        &mut orbit_camera.map_params
    };
    let earth_transform = earth.into_inner();

    let up_frame = Quat::from_rotation_arc(Vec3::Y, camera.up);

    // Adjust the actual transform of the camera
    let new_rot = Quat::from_euler(EulerRot::YXZ, camera.yaw, camera.pitch, 0.0);

    transform.rotation = (up_frame * new_rot).normalize();

    let new_pos = camera.focus - transform.forward() * camera.distance;

    // let delta_pos = new_pos - transform.translation;

    transform.translation = new_pos;

    if render_layers.intersects(&RenderLayers::layer(SCENE_LAYER)) {
        if let Ok(target) = targets.single() {
            orbit_camera.scene_params.focus = target.translation;
            orbit_camera.scene_params.up =
                -(earth_transform.translation - target.translation).normalize_or(Vec3::NEG_Y);
        }
    }
}

pub fn orbit_camera_switch_target(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    bodies: Query<(Entity, Has<CameraTarget>), (With<RigidBody>, With<Orbital>)>,
) {
    if !keyboard_input.just_pressed(KeyCode::Tab) {
        return;
    }

    let mut entities: Vec<(Entity, bool)> = bodies.iter().collect();
    if entities.is_empty() {
        return;
    }

    entities.sort_by_key(|(entity, _)| entity.index());

    let current_index = entities
        .iter()
        .position(|(_, is_target)| *is_target)
        .unwrap_or(0);
    let next_target = entities[(current_index + 1) % entities.len()].0;

    for (entity, is_target) in &entities {
        if *is_target {
            commands.entity(*entity).remove::<CameraTarget>();
        }
    }
    commands.entity(next_target).insert(CameraTarget);
}

pub fn orbit_camera_control_target(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut bodies: Query<&mut ConstantForce>,
    target_q: Query<Entity, With<CameraTarget>>,
    s: Single<(&mut OrbitCamera, &mut Transform), With<Camera3d>>,
    time: Res<Time>,
) {
    let Ok(target_entity) = target_q.single() else {
        return;
    };

    if keyboard_input.pressed(KeyCode::KeyW) {
        let (_camera, transform) = s.into_inner();
        if let Ok(mut v) = bodies.get_mut(target_entity) {
            let force_dir = transform.forward() * time.delta().as_secs_f32() * 10000.0;
            *v = ConstantForce::new(force_dir.x, force_dir.y, force_dir.z);
        }
    } else if keyboard_input.just_released(KeyCode::KeyW) {
        if let Ok(mut v) = bodies.get_mut(target_entity) {
            *v = ConstantForce::new(0.0, 0.0, 0.0);
        }
    }
}
