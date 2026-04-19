use std::f32::consts::PI;
use std::ops::RangeInclusive;

use crate::components::orbit::Earth;
use crate::components::orbit_camera::{CameraTarget, OrbitCamera, OrbitCameraParams};
use crate::constants::*;
use crate::resources::capture_log::{LogEvent, LogLevel};
use crate::resources::capture_plans::CapturePlanLibrary;
use crate::resources::celestials::Celestials;
use crate::resources::orbital_cache::OrbitalCache;
use crate::resources::space_catalog::OrbitalSelectionState;
use crate::systems::spawners::{spawn_debris, spawn_tether};
use crate::ui::state::{SelectedProject, UiScreen};

use avian3d::prelude::*;
use bevy::camera::visibility::RenderLayers;
use bevy::core_pipeline::Skybox;
use bevy::light::{CascadeShadowConfigBuilder, SunDisk};
use bevy::math::cubic_splines::LinearSpline;
use bevy::pbr::{Atmosphere, AtmosphereMode, AtmosphereSettings, ScatteringMedium};
use bevy::post_process::auto_exposure::{AutoExposure, AutoExposureCompensationCurve};
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;
use nalgebra::Vector6;

pub fn setup_lighting(mut commands: Commands) {
    let sun_rotation = Quat::from_rotation_x(0.0);
    let moon_rotation = sun_rotation * Quat::from_rotation_y(PI);

    // Sun
    commands.spawn((
        DespawnOnExit(UiScreen::Sim),
        RenderLayers::from_layers(&[SCENE_LAYER, MAP_LAYER]),
        DirectionalLight {
            illuminance: light_consts::lux::AMBIENT_DAYLIGHT,
            shadows_enabled: true,
            ..default()
        },
        SunDisk::EARTH,
        Transform {
            rotation: sun_rotation,
            ..default()
        },
        CascadeShadowConfigBuilder::default().build(),
    ));

    // Moon
    commands.spawn((
        DespawnOnExit(UiScreen::Sim),
        RenderLayers::from_layers(&[SCENE_LAYER, MAP_LAYER]),
        DirectionalLight {
            illuminance: light_consts::lux::FULL_MOON_NIGHT,
            shadows_enabled: true,
            ..default()
        },
        SunDisk::EARTH,
        Transform {
            rotation: moon_rotation,
            ..default()
        },
        CascadeShadowConfigBuilder::default().build(),
    ));
}

/// -------------------------------------------------------------- ///
///                         SCENE SETUP
/// -------------------------------------------------------------- ///
pub fn setup_celestial(
    mut commands: Commands,
    mut celestials: ResMut<Celestials>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    asset_server: Res<AssetServer>,
) {
    // Set up Earth rendering
    let earth_mesh = Sphere::new(EARTH_RADIUS).mesh().uv(512, 256);
    let earth_texture: Handle<Image> = asset_server.load("textures/earth_8192x4096_uastc.ktx2");
    let earth_material = materials.add(StandardMaterial {
        base_color_texture: Some(earth_texture),
        perceptual_roughness: 1.0,
        ..default()
    });

    // Add Earth to our CelestialBodies resource (enables global entity access)
    celestials.planets.insert(
        "Earth".to_string(),
        commands
            .spawn((
                DespawnOnExit(UiScreen::Sim),
                Earth,
                RigidBodyDisabled,
                RenderLayers::layer(SCENE_LAYER),
                // Orbit::FromParams(TrueParams {
                //     rv: Vector6::new(0.0, 0.0, 0.0, 0.0, 0.0, 0.0),
                // }),
                Mesh3d(meshes.add(earth_mesh)),
                MeshMaterial3d(earth_material.clone()),
                Transform::from_xyz(0.0, 0.0, 0.0).with_rotation(orbit_frame_rotation()),
            ))
            .id(),
    );

    // Set up Earth map rendering
    let map_earth_mesh = Sphere::new(EARTH_RADIUS / MAP_UNITS_TO_M as f32)
        .mesh()
        .uv(512, 256);

    celestials.planets.insert(
        "Map_Earth".to_string(),
        commands
            .spawn((
                DespawnOnExit(UiScreen::Sim),
                RenderLayers::layer(MAP_LAYER),
                Mesh3d(meshes.add(map_earth_mesh)),
                MeshMaterial3d(earth_material.clone()),
                Transform::from_xyz(0.0, 0.0, 0.0).with_rotation(orbit_frame_rotation()),
            ))
            .id(),
    );
}

pub fn setup_camera(
    mut commands: Commands,
    mut scattering_mediums: ResMut<Assets<ScatteringMedium>>,
    mut compensation_curves: ResMut<Assets<AutoExposureCompensationCurve>>,
    asset_server: Res<AssetServer>,
    mut log_events: MessageWriter<LogEvent>,
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

pub fn setup_orbital_selection(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    selected_project: Res<SelectedProject>,
    capture_plan_lib: Res<CapturePlanLibrary>,
    selection: Res<OrbitalSelectionState>,
    mut orbital_cache: ResMut<OrbitalCache>,
    asset_server: Res<AssetServer>,
) {
    let Some(chaser) = selection.chaser.as_ref() else {
        return;
    };

    let Some(target) = selection.target.as_ref() else {
        return;
    };

    if let Err(e) = spawn_tether(
        &mut commands,
        &mut meshes,
        &mut materials,
        &mut orbital_cache,
        &selected_project,
        &capture_plan_lib,
        chaser.elements.to_vec6(),
    ) {
        error!("Error spawning tether: {}", e);
    };

    if let Err(e) = spawn_debris(
        &mut commands,
        &mut orbital_cache,
        &asset_server,
        target.elements.to_vec6(),
    ) {
        error!("Error spawning debris: {}", e);
    };
}
