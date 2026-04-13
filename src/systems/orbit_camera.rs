use crate::{
    components::{
        orbit::{Earth, Orbital},
        orbit_camera::{CameraTarget, OrbitCamera, OrbitControlButton, OrbitHoldState},
    },
    constants::{
        MAP_UNITS_TO_M, ORBIT_WIDGET_ACCEL_MULTIPLIER, ORBIT_WIDGET_BASE_ORBIT_SPEED,
        ORBIT_WIDGET_BASE_ZOOM_SPEED, ORBIT_WIDGET_HOLD_THRESHOLD_SECS, SCENE_LAYER,
    },
    resources::{orbital_cache::OrbitalCache, space_catalog::SpaceCatalogUiState},
};
use avian3d::prelude::*;
use bevy::{
    camera::visibility::RenderLayers,
    input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit},
    prelude::*,
};
use bevy_egui::input::EguiWantsInput;

pub fn orbit_camera_input(
    buttons: Res<ButtonInput<MouseButton>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse_motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    s: Single<(&mut OrbitCamera, &RenderLayers), With<Camera3d>>,
    ui_interactions: Query<&Interaction, With<Node>>,
    egui_wants_input: Option<Res<EguiWantsInput>>,
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
        .any(|interaction| *interaction != Interaction::None)
        || egui_wants_input
            .as_ref()
            .is_some_and(|egui| egui.is_pointer_over_area());

    let camera = if render_layers.intersects(&RenderLayers::layer(SCENE_LAYER)) {
        &mut orbit_cameras.scene_params
    } else {
        &mut orbit_cameras.map_params
    };

    // Ctrl + left-click drag is an alternative to right-click drag.
    let ctrl_held =
        keyboard.pressed(KeyCode::ControlLeft) || keyboard.pressed(KeyCode::ControlRight);
    let ctrl_drag = ctrl_held && buttons.pressed(MouseButton::Left);

    if (buttons.pressed(MouseButton::Right) || ctrl_drag) && delta != Vec2::ZERO {
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
    targets: Query<(&Transform, Entity), (With<CameraTarget>, Without<Camera3d>, Without<Earth>)>,
    cam_q: Single<
        (&mut OrbitCamera, &mut Transform, &RenderLayers),
        (With<Camera3d>, Without<Earth>),
    >,
    earth: Single<&Transform, With<Earth>>,
    orbital_cache: Res<OrbitalCache>,
) {
    let (mut orbit_camera, mut cam_transform, render_layers) = cam_q.into_inner();
    let camera = if render_layers.intersects(&RenderLayers::layer(SCENE_LAYER)) {
        &mut orbit_camera.scene_params
    } else {
        &mut orbit_camera.map_params
    };
    let earth_transform = earth.into_inner();

    let Ok((target_transform, entity)) = targets.single() else {
        return;
    };

    if render_layers.intersects(&RenderLayers::layer(SCENE_LAYER)) {
        camera.focus = target_transform.translation;
        camera.up =
            -(earth_transform.translation - target_transform.translation).normalize_or(Vec3::NEG_Y);
    } else {
        let Some(target_rv) = orbital_cache.eci_states.get(&entity) else {
            return;
        };
        camera.focus = Vec3::new(
            (target_rv[0] / MAP_UNITS_TO_M) as f32,
            (target_rv[1] / MAP_UNITS_TO_M) as f32,
            (target_rv[2] / MAP_UNITS_TO_M) as f32,
        );
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
    cam_transform.rotation = (up_frame * new_rot).normalize();

    let new_pos = camera.focus - cam_transform.forward() * camera.distance;

    // let delta_pos = new_pos - transform.translation;

    cam_transform.translation = new_pos;
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
/// Movement is applied every frame while a button is held, framerate-independent.
/// After 5 s of continuous hold the movement speed increases to 2.5×.
/// Releasing the button resets the hold timer.
/// `ResetView` fires only on the initial press frame.
pub fn orbit_camera_ui_controls(
    orbit_btn_q: Query<(&Interaction, &OrbitControlButton), With<Button>>,
    cam_q: Single<(&mut OrbitCamera, &RenderLayers), With<Camera3d>>,
    time: Res<Time>,
    mut hold_state: ResMut<OrbitHoldState>,
) {
    // Find the first pressed button this frame (if any).
    let pressed_btn = orbit_btn_q
        .iter()
        .find(|(i, _)| **i == Interaction::Pressed)
        .map(|(_, b)| b.clone());

    // Detect the very first frame of a new distinct press.
    let is_new_press = match (&pressed_btn, &hold_state.held) {
        (Some(btn), Some(held)) => btn != held,
        (Some(_), None) => true,
        _ => false,
    };

    // Update hold tracking.
    match &pressed_btn {
        Some(btn) if hold_state.held.as_ref() == Some(btn) => {
            hold_state.hold_secs += time.delta_secs();
        }
        Some(btn) => {
            hold_state.held = Some(btn.clone());
            hold_state.hold_secs = 0.0;
        }
        None => {
            hold_state.held = None;
            hold_state.hold_secs = 0.0;
            return;
        }
    }

    let btn_kind = match &pressed_btn {
        Some(b) => b,
        None => return,
    };

    let (mut orbit_camera, render_layers) = cam_q.into_inner();
    let camera = if render_layers.intersects(&RenderLayers::layer(SCENE_LAYER)) {
        &mut orbit_camera.scene_params
    } else {
        &mut orbit_camera.map_params
    };

    // ResetView fires once per press, not continuously.
    if *btn_kind == OrbitControlButton::ResetView {
        if is_new_press {
            camera.yaw = 0.0;
            camera.pitch = 0.4;
            camera.distance = 30.0;
        }
        return;
    }

    // Base speed is 1/4 of the original; ramps to full original speed after the hold threshold.
    let speed = if hold_state.hold_secs >= ORBIT_WIDGET_HOLD_THRESHOLD_SECS {
        ORBIT_WIDGET_ACCEL_MULTIPLIER
    } else {
        1.0
    };

    let yaw_step = ORBIT_WIDGET_BASE_ORBIT_SPEED * time.delta_secs() * speed;
    let pitch_step = ORBIT_WIDGET_BASE_ORBIT_SPEED * time.delta_secs() * speed;
    let zoom_step = ORBIT_WIDGET_BASE_ZOOM_SPEED * time.delta_secs() * speed;

    match btn_kind {
        OrbitControlButton::OrbitLeft => {
            camera.yaw += yaw_step;
        }
        OrbitControlButton::OrbitRight => {
            camera.yaw -= yaw_step;
        }
        OrbitControlButton::OrbitUp => {
            camera.pitch -= pitch_step;
            camera.pitch = camera.pitch.clamp(-camera.max_pitch, camera.max_pitch);
        }
        OrbitControlButton::OrbitDown => {
            camera.pitch += pitch_step;
            camera.pitch = camera.pitch.clamp(-camera.max_pitch, camera.max_pitch);
        }
        OrbitControlButton::ZoomIn => {
            camera.distance -= zoom_step;
            camera.distance = camera
                .distance
                .clamp(camera.min_distance, camera.max_distance);
        }
        OrbitControlButton::ZoomOut => {
            camera.distance += zoom_step;
            camera.distance = camera
                .distance
                .clamp(camera.min_distance, camera.max_distance);
        }
        OrbitControlButton::ResetView => { /* handled above */ }
    }
}
