use crate::{
    components::{
        orbit::Earth,
        orbit_camera::{CameraTarget, OrbitCamera},
    },
    constants::{EARTH_TEXTURE_NORTH_AXIS, SCENE_LAYER},
    resources::{orbital_cache::OrbitalCache, space_catalog::SpaceCatalogUiState},
};
use bevy::{
    camera::visibility::RenderLayers,
    input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit},
    prelude::*,
};
use bevy_egui::input::EguiWantsInput;

/// Builds the restricted camera-cycle candidate list: each tether's root and
/// tail nodes, plus the debris/target entities. Shared by the UI button's
/// `CycleCameraTarget` handler and the raw Tab-key cycling system below, so
/// both visit the same set instead of every intermediate tether node.
pub fn camera_cycle_candidates(orbital_cache: &OrbitalCache) -> Vec<Entity> {
    let mut candidates: Vec<Entity> = Vec::new();
    for nodes in orbital_cache.tethers.values() {
        if let (Some(&first), Some(&last)) = (nodes.first(), nodes.last()) {
            candidates.push(first);
            if last != first {
                candidates.push(last);
            }
        }
    }
    candidates.extend(orbital_cache.debris.values().copied());
    candidates.sort_by_key(|e| e.index());
    candidates
}

pub fn orbit_camera_input(
    buttons: Res<ButtonInput<MouseButton>>,
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

    if buttons.pressed(MouseButton::Right) && delta != Vec2::ZERO && !pointer_over_ui {
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
    let (mut orbit_camera, mut cam_transform, render_layers) = cam_q.into_inner();
    let camera = if render_layers.intersects(&RenderLayers::layer(SCENE_LAYER)) {
        &mut orbit_camera.scene_params
    } else {
        &mut orbit_camera.map_params
    };
    let earth_transform = earth.into_inner();

    if render_layers.intersects(&RenderLayers::layer(SCENE_LAYER)) {
        if let Ok(target_transform) = targets.single() {
            camera.focus = target_transform.translation;
            camera.up = -(earth_transform.translation - target_transform.translation)
                .normalize_or(Vec3::NEG_Y);
        }
    } else {
        // Map view is always Earth-centered — it doesn't follow whichever
        // entity is currently tagged `CameraTarget` for detail view.
        camera.focus = Vec3::ZERO;
    }

    let up = camera.up.normalize_or(Vec3::Y);
    let earth_axis = (earth_transform.rotation * EARTH_TEXTURE_NORTH_AXIS).normalize_or(Vec3::Y);
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
    orbital_cache: Res<OrbitalCache>,
    camera_targets: Query<(), With<CameraTarget>>,
) {
    if catalog_ui.search_focused {
        return;
    }

    if !keyboard_input.just_pressed(KeyCode::Tab) {
        return;
    }

    let candidates = camera_cycle_candidates(&orbital_cache);
    if candidates.is_empty() {
        return;
    }

    let current_index = candidates
        .iter()
        .position(|&e| camera_targets.contains(e))
        .unwrap_or(0);
    let next_target = candidates[(current_index + 1) % candidates.len()];

    for &entity in &candidates {
        if camera_targets.contains(entity) {
            commands.entity(entity).remove::<CameraTarget>();
        }
    }
    commands.entity(next_target).insert(CameraTarget);
}
