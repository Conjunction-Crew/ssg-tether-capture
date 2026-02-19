use crate::{
    components::{
        orbit::Orbital,
        orbit_camera::{OrbitCamera, OrbitCameraParams},
    },
    constants::{MAP_LAYER, SCENE_LAYER},
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

    // Adjust the actual transform of the camera
    let new_rot = Quat::from_euler(EulerRot::YXZ, camera.yaw, camera.pitch, 0.0);
    transform.rotation = new_rot;

    let new_pos = camera.focus - transform.forward() * camera.distance;

    let delta_pos = new_pos - transform.translation;

    transform.translation += delta_pos;
}

pub fn orbit_camera_track(
    targets: Query<&Transform, Without<Camera3d>>,
    cam_q: Single<(&mut OrbitCamera, &RenderLayers), With<Camera3d>>,
) {
    let (mut orbit_cameras, render_layers) = cam_q.into_inner();

    let camera = if render_layers.intersects(&RenderLayers::layer(SCENE_LAYER)) {
        &mut orbit_cameras.scene_params
    } else {
        &mut orbit_cameras.map_params
    };

    if let Some(entity) = camera.target {
        if let Ok(target) = targets.get(entity) {
            camera.focus = target.translation;
        }
    }
}

pub fn orbit_camera_switch_target(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    bodies: Query<Entity, (With<RigidBody>, With<Orbital>)>,
    cam_q: Single<(&mut OrbitCamera, &RenderLayers), With<Camera3d>>,
) {
    if keyboard_input.just_pressed(KeyCode::Tab) {
        let mut entities: Vec<Entity> = bodies.iter().collect();

        if entities.is_empty() {
            return;
        }

        entities.sort_by_key(|e| e.index());

        let (mut orbit_cameras, render_layers) = cam_q.into_inner();

        let camera = if render_layers.intersects(&RenderLayers::layer(SCENE_LAYER)) {
            &mut orbit_cameras.scene_params
        } else {
            &mut orbit_cameras.map_params
        };

        camera.target = Some(match camera.target {
            None => entities[0],
            Some(t) => {
                let i = entities.iter().position(|&e| e == t).unwrap_or(0);
                entities[(i + 1) % entities.len()]
            }
        })
    }
}

pub fn orbit_camera_control_target(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut bodies: Query<&mut ConstantForce>,
    s: Single<(&mut OrbitCamera, &mut Transform), With<Camera3d>>,
    time: Res<Time>,
) {
    if keyboard_input.pressed(KeyCode::KeyW) {
        let (camera, transform) = s.into_inner();
        if let Some(t) = camera.scene_params.target {
            if let Ok(mut v) = bodies.get_mut(t) {
                let force_dir = transform.forward() * time.delta().as_secs_f32() * 10000.0;
                *v = ConstantForce::new(force_dir.x, force_dir.y, force_dir.z);
            }
        }
    } else if keyboard_input.just_released(KeyCode::KeyW) {
        let (camera, _transform) = s.into_inner();
        if let Some(t) = camera.scene_params.target {
            if let Ok(mut v) = bodies.get_mut(t) {
                *v = ConstantForce::new(0.0, 0.0, 0.0);
            }
        }
    }
}
