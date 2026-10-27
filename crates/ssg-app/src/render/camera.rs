//! The scene/map camera and how it follows the camera target.

use std::ops::RangeInclusive;

use avian3d::prelude::*;
use bevy::core_pipeline::Skybox;
use bevy::math::cubic_splines::LinearSpline;
use bevy::pbr::{Atmosphere, AtmosphereMode, AtmosphereSettings, ScatteringMedium};
use bevy::post_process::auto_exposure::{AutoExposure, AutoExposureCompensationCurve};
use bevy::post_process::bloom::Bloom;
use bevy::transform::TransformSystems;
use bevy::{
    camera::visibility::RenderLayers,
    input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit},
    prelude::*,
};
use bevy_egui::input::EguiWantsInput;

use crate::render::celestial::Earth;
use crate::render::layers::{
    EARTH_ATMOSPHERE_RADIUS, EARTH_RADIUS, EARTH_TEXTURE_NORTH_AXIS, MAP_LAYER, MAP_UNITS_TO_M,
    SCENE_LAYER, eci_position_to_map,
};
use crate::sim::anchor::CameraTarget;
use crate::sim::orbit::Orbital;
use crate::sim::orbit::cache::OrbitalCache;
use crate::sim::session::UiScreen;
use crate::sim::telemetry::{LogEvent, LogLevel};
use crate::ui::catalog_state::SpaceCatalogUiState;

// A camera that "orbits" around a target. Hold right click to pan.
#[derive(Component, Debug, Clone)]
pub struct OrbitCamera {
    pub scene_params: OrbitCameraParams,
    pub map_params: OrbitCameraParams,
}

#[derive(Debug, Clone)]
pub struct OrbitCameraParams {
    pub focus: Vec3,
    pub distance: f32,
    pub yaw: f32,
    pub pitch: f32,
    pub min_distance: f32,
    pub max_distance: f32,
    pub sensitivity: f32,
    pub max_pitch: f32,
    pub up: Vec3,
}

impl Default for OrbitCameraParams {
    fn default() -> Self {
        Self {
            focus: Vec3::ZERO,
            distance: 30.0,
            yaw: 0.0,
            pitch: 0.0,
            min_distance: 0.5,
            max_distance: 10000.0,
            #[cfg(windows)] // Decreased default sensitivity on windows due to quirks.
            sensitivity: 0.002,
            #[cfg(not(windows))]
            sensitivity: 0.005,
            max_pitch: 1.55,
            up: Vec3::Y,
        }
    }
}

pub struct OrbitCameraPlugin;

impl Plugin for OrbitCameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (orbit_camera_input, orbit_camera_switch_target).run_if(in_state(UiScreen::Sim)),
        )
        .add_systems(
            PostUpdate,
            orbit_camera_track
                .before(TransformSystems::Propagate)
                .run_if(in_state(UiScreen::Sim)),
        );
    }
}

pub fn setup_camera(
    mut commands: Commands,
    mut scattering_mediums: ResMut<Assets<ScatteringMedium>>,
    mut compensation_curves: ResMut<Assets<AutoExposureCompensationCurve>>,
    asset_server: Res<AssetServer>,
    _log_events: MessageWriter<LogEvent>,
) {
    // Skybox
    let skybox_handle: Handle<Image> = asset_server.load("textures/hdr-cubemap-2048x2048.ktx2");

    // Set up 3D scene camera
    commands.spawn((
        DespawnOnExit(UiScreen::Sim),
        RenderLayers::layer(MAP_LAYER),
        Camera3d::default(),
        Bloom {
            intensity: 0.01,
            ..default()
        },
        AutoExposure {
            filter: RangeInclusive::new(0.75, 0.99),
            speed_brighten: 3.0,
            speed_darken: 1.0,
            compensation_curve: compensation_curves.add(
                AutoExposureCompensationCurve::from_curve(LinearSpline::new([
                    vec2(-4.0, -2.5),
                    vec2(0.0, 0.25),
                    vec2(2.0, 0.5),
                    vec2(4.0, 1.0),
                ]))
                .unwrap_or_else(|error| {
                    warn!("Invalid auto exposure compensation curve: {error:?}");
                    AutoExposureCompensationCurve::default()
                }),
            ),
            ..default()
        },
        Camera {
            order: 0,
            is_active: true,
            ..default()
        },
        Skybox {
            image: skybox_handle.clone(),
            brightness: 1000.0,
            ..default()
        },
        Transform::from_xyz(0.0, 0.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y),
        OrbitCamera {
            scene_params: OrbitCameraParams::default(),
            map_params: OrbitCameraParams {
                distance: EARTH_ATMOSPHERE_RADIUS / MAP_UNITS_TO_M as f32
                    + 2.0 * (EARTH_ATMOSPHERE_RADIUS / MAP_UNITS_TO_M as f32),
                min_distance: 0.2,
                ..default()
            },
        },
        AmbientLight {
            brightness: 1.0,
            ..default()
        },
        Atmosphere {
            world_position: Vec3::new(0.0, 0.0, 0.0),
            bottom_radius: EARTH_RADIUS,
            top_radius: EARTH_ATMOSPHERE_RADIUS,
            ground_albedo: Vec3::splat(0.3),
            medium: scattering_mediums.add(ScatteringMedium::default()),
        },
        AtmosphereSettings {
            sky_view_lut_size: UVec2::new(512, 256),
            rendering_method: AtmosphereMode::Raymarched,
            scene_units_to_m: MAP_UNITS_TO_M as f32,
            ..default()
        },
    ));
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

/// The camera target's transform (not the camera's, not the Earth's).
type TargetTransforms<'w, 's> = Query<
    'w,
    's,
    (&'static Transform, Entity),
    (With<CameraTarget>, Without<Camera3d>, Without<Earth>),
>;

/// The scene camera's orbit state, transform and render layers.
type TrackedCamera<'w, 's> = Single<
    'w,
    's,
    (
        &'static mut OrbitCamera,
        &'static mut Transform,
        &'static RenderLayers,
    ),
    (With<Camera3d>, Without<Earth>),
>;

/// The bodies the camera can switch to, and whether each is the camera target.
type CameraBodies<'w, 's> =
    Query<'w, 's, (Entity, Has<CameraTarget>), (With<RigidBody>, With<Orbital>)>;

pub fn orbit_camera_track(
    targets: TargetTransforms,
    cam_q: TrackedCamera,
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
    let target = targets.single().ok();

    if render_layers.intersects(&RenderLayers::layer(SCENE_LAYER)) {
        if let Some((target_transform, _)) = target {
            camera.focus = target_transform.translation;
            camera.up = -(earth_transform.translation - target_transform.translation)
                .normalize_or(Vec3::NEG_Y);
        }
    } else if let Some((_, entity)) = target
        && let Some(target_rv) = orbital_cache.eci_states.get(&entity)
    {
        camera.focus = eci_position_to_map(Vec3::new(
            target_rv[0] as f32,
            target_rv[1] as f32,
            target_rv[2] as f32,
        ));
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
    bodies: CameraBodies,
    mut log: MessageWriter<LogEvent>,
) {
    if catalog_ui.search_focused {
        return;
    }

    if !keyboard_input.just_pressed(KeyCode::Tab) {
        return;
    }

    if let Some(next_target) = cycle_camera_target(&mut commands, bodies.iter()) {
        log.write(LogEvent {
            level: LogLevel::Info,
            source: "ui",
            message: format!("Camera target → entity {next_target:?}"),
        });
    }
}

/// Moves `CameraTarget` to the next of `bodies` by entity index, wrapping, and
/// returns the new target (`None` if there are no bodies). The Tab key and the
/// UI button both call it (F-29).
pub fn cycle_camera_target(
    commands: &mut Commands,
    bodies: impl Iterator<Item = (Entity, bool)>,
) -> Option<Entity> {
    let mut entities: Vec<(Entity, bool)> = bodies.collect();
    if entities.is_empty() {
        return None;
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
    Some(next_target)
}

/// Switches the scene camera between the scene and the map layer and adjusts the
/// atmosphere. Returns the log line, or `None` if the camera is on neither layer.
/// The M key and the UI button both call it (F-29).
pub fn set_map_view(
    mut render_layers: Mut<RenderLayers>,
    mut atmosphere: Mut<Atmosphere>,
    mut atmosphere_settings: Mut<AtmosphereSettings>,
) -> Option<&'static str> {
    if render_layers.intersects(&RenderLayers::layer(SCENE_LAYER)) {
        *render_layers = RenderLayers::layer(MAP_LAYER);

        // Adjust atmosphere
        atmosphere.world_position = Vec3::ZERO;
        atmosphere_settings.scene_units_to_m = MAP_UNITS_TO_M as f32;
        Some("Switched to map view")
    } else if render_layers.intersects(&RenderLayers::layer(MAP_LAYER)) {
        *render_layers = RenderLayers::layer(SCENE_LAYER);

        // Adjust atmosphere
        atmosphere_settings.scene_units_to_m = 1.0;
        Some("Switched to scene view")
    } else {
        None
    }
}
