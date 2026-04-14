use crate::{
    components::{
        orbit::{Earth, Orbital},
        orbit_camera::{
            CameraTarget, OrbitCamera, OrbitControlButton, OrbitControlsDragRegion, OrbitDragState,
            OrbitHoldState,
        },
    },
    constants::{
        ORBIT_WIDGET_ACCEL_MULTIPLIER, ORBIT_WIDGET_BASE_ORBIT_SPEED, ORBIT_WIDGET_BASE_ZOOM_SPEED,
        ORBIT_WIDGET_HOLD_THRESHOLD_SECS, SCENE_LAYER,
    },
    resources::space_catalog::SpaceCatalogUiState,
};
use avian3d::prelude::*;
use bevy::{
    camera::visibility::RenderLayers,
    input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit},
    prelude::*,
};

/// Handles right-click drag and Ctrl+left-click drag for orbiting the camera.
pub fn orbit_camera_input(
    buttons: Res<ButtonInput<MouseButton>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse_motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    drag_state: Res<OrbitDragState>,
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

    // Ctrl + left-click drag is an alternative to right-click drag.
    // Suppressed while a widget bounding-box drag is active to avoid double-movement.
    let ctrl_held =
        keyboard.pressed(KeyCode::ControlLeft) || keyboard.pressed(KeyCode::ControlRight);
    let ctrl_drag = ctrl_held && buttons.pressed(MouseButton::Left) && !drag_state.active;

    if (buttons.pressed(MouseButton::Right) || ctrl_drag) && delta != Vec2::ZERO {
        camera.yaw -= delta.x * camera.sensitivity;
        camera.pitch -= delta.y * camera.sensitivity;
        camera.pitch = camera.pitch.clamp(-camera.max_pitch, camera.max_pitch);
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
            camera.pitch += pitch_step;
            camera.pitch = camera.pitch.clamp(-camera.max_pitch, camera.max_pitch);
        }
        OrbitControlButton::OrbitDown => {
            camera.pitch -= pitch_step;
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

/// Handles left-click drag inside the orbit controls bounding box.
///
/// When the user presses the left mouse button over the widget container (but
/// NOT over one of the directional/zoom buttons), dragging moves the camera
/// exactly like right-click drag in the main 3D view.
pub fn orbit_controls_drag(
    drag_region: Query<&Interaction, With<OrbitControlsDragRegion>>,
    orbit_btns: Query<&Interaction, With<OrbitControlButton>>,
    mouse_motion: Res<AccumulatedMouseMotion>,
    buttons: Res<ButtonInput<MouseButton>>,
    mut drag_state: ResMut<OrbitDragState>,
    cam_q: Single<(&mut OrbitCamera, &RenderLayers), With<Camera3d>>,
) {
    if buttons.just_released(MouseButton::Left) {
        drag_state.active = false;
        return;
    }

    // Begin a drag only when the bounding box itself is pressed and no
    // individual orbit button is pressed (buttons sit on top of the container).
    if buttons.just_pressed(MouseButton::Left) {
        let any_btn_pressed = orbit_btns.iter().any(|i| *i == Interaction::Pressed);
        let region_pressed = drag_region.iter().any(|i| *i == Interaction::Pressed);
        if region_pressed && !any_btn_pressed {
            drag_state.active = true;
        }
    }

    if !drag_state.active || !buttons.pressed(MouseButton::Left) {
        return;
    }

    let delta = mouse_motion.delta;
    if delta == Vec2::ZERO {
        return;
    }

    let (mut orbit_camera, render_layers) = cam_q.into_inner();
    let camera = if render_layers.intersects(&RenderLayers::layer(SCENE_LAYER)) {
        &mut orbit_camera.scene_params
    } else {
        &mut orbit_camera.map_params
    };

    camera.yaw -= delta.x * camera.sensitivity;
    camera.pitch -= delta.y * camera.sensitivity;
    camera.pitch = camera.pitch.clamp(-camera.max_pitch, camera.max_pitch);
}
