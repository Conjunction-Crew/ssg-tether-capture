use crate::{
    components::{
        orbit::{Earth, Orbital},
        orbit_camera::{CameraTarget, OrbitCamera, OrbitControlButton},
    },
    constants::SCENE_LAYER,
    resources::space_catalog::SpaceCatalogUiState,
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
    s: Single<(&mut OrbitCamera, &RenderLayers), With<Camera3d>>,
    ui_interactions: Query<&Interaction, With<Node>>,
) {
    let (mut orbit_cameras, render_layers) = s.into_inner();

    let delta = mouse_motion.delta;

    let scroll_y = match scroll.unit {
        MouseScrollUnit::Line => scroll.delta.y,
        MouseScrollUnit::Pixel => scroll.delta.y * 0.01,
    };

    // Skip scroll zoom if pointer is hovering over a UI element
    let pointer_over_ui = ui_interactions
        .iter()
        .any(|interaction| *interaction != Interaction::None);

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

    if scroll_y != 0.0 && !pointer_over_ui {
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

    if render_layers.intersects(&RenderLayers::layer(SCENE_LAYER)) {
        if let Ok(target) = targets.single() {
            camera.focus = target.translation;
            camera.up =
                -(earth_transform.translation - target.translation).normalize_or(Vec3::NEG_Y);
        }
    }

    let up = camera.up.normalize_or(Vec3::Y);
    let earth_axis = (earth_transform.rotation * Vec3::Y).normalize_or(Vec3::Y);
    let base_forward = (earth_axis - up * earth_axis.dot(up))
        .normalize_or((Vec3::NEG_Z - up * Vec3::NEG_Z.dot(up)).normalize_or(Vec3::X));
    let right = base_forward.cross(up).normalize_or(Vec3::X);
    let forward = up.cross(right).normalize_or(Vec3::NEG_Z);
    let up_frame = Quat::from_mat3(&Mat3::from_cols(right, up, -forward));

    // Adjust the actual transform of the camera
    let new_rot = Quat::from_euler(EulerRot::YXZ, camera.yaw, camera.pitch, 0.0);
    transform.rotation = (up_frame * new_rot).normalize();

    let new_pos = camera.focus - transform.forward() * camera.distance;

    // let delta_pos = new_pos - transform.translation;

    transform.translation = new_pos;
}

pub fn orbit_camera_switch_target(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    catalog_ui: Res<SpaceCatalogUiState>,
    mut commands: Commands,
    bodies: Query<(Entity, Has<CameraTarget>), (With<RigidBody>, With<Orbital>)>,
) {
    if catalog_ui.search_focused {
        return;
    }

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

/// Handles the on-screen orbit controls widget buttons.
///
/// Each button maps to an `OrbitControlButton` variant.  While the button is
/// held (`Interaction::Pressed`) a constant yaw / pitch / distance delta is
/// applied to `OrbitCamera.scene_params` every frame, giving smooth, continuous
/// motion.  A single press is also enough to move the camera (useful for
/// quick taps on a touchpad).
pub fn orbit_camera_ui_controls(
    orbit_btn_q: Query<(&Interaction, &OrbitControlButton), (Changed<Interaction>, With<Button>)>,
    cam_q: Single<(&mut OrbitCamera, &RenderLayers), With<Camera3d>>,
) {
    let (mut orbit_camera, render_layers) = cam_q.into_inner();
    let camera = if render_layers.intersects(&RenderLayers::layer(SCENE_LAYER)) {
        &mut orbit_camera.scene_params
    } else {
        &mut orbit_camera.map_params
    };

    // Delta values — tuned to feel roughly equivalent to a short mouse drag.
    const YAW_STEP: f32 = 0.05;
    const PITCH_STEP: f32 = 0.05;
    const ZOOM_STEP: f32 = 1.5;

    for (interaction, btn_kind) in orbit_btn_q.iter() {
        if *interaction != Interaction::Pressed {
            continue;
        }

        match btn_kind {
            OrbitControlButton::OrbitLeft => {
                camera.yaw += YAW_STEP;
            }
            OrbitControlButton::OrbitRight => {
                camera.yaw -= YAW_STEP;
            }
            OrbitControlButton::OrbitUp => {
                camera.pitch -= PITCH_STEP;
                camera.pitch = camera.pitch.clamp(-camera.max_pitch, camera.max_pitch);
            }
            OrbitControlButton::OrbitDown => {
                camera.pitch += PITCH_STEP;
                camera.pitch = camera.pitch.clamp(-camera.max_pitch, camera.max_pitch);
            }
            OrbitControlButton::ZoomIn => {
                camera.distance -= ZOOM_STEP;
                camera.distance = camera
                    .distance
                    .clamp(camera.min_distance, camera.max_distance);
            }
            OrbitControlButton::ZoomOut => {
                camera.distance += ZOOM_STEP;
                camera.distance = camera
                    .distance
                    .clamp(camera.min_distance, camera.max_distance);
            }
            OrbitControlButton::ResetView => {
                camera.yaw = 0.0;
                camera.pitch = 0.4;
                camera.distance = 30.0;
            }
        }
    }
}
