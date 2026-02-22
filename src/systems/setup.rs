use std::f32::consts::PI;
use std::ops::RangeInclusive;

use crate::components::orbit::{Earth, Orbital, TetherNode};
use crate::components::orbit_camera::{OrbitCamera, OrbitCameraParams};
use crate::components::user_interface::{OrbitLabel, TrackObject};
use crate::constants::*;
use crate::resources::celestials::Celestials;
use crate::resources::devices::Devices;

use avian3d::prelude::*;
use bevy::camera::CameraOutputMode;
use bevy::camera::visibility::RenderLayers;
use bevy::core_pipeline::Skybox;
use bevy::light::{CascadeShadowConfigBuilder, SunDisk};
use bevy::math::cubic_splines::LinearSpline;
use bevy::pbr::{Atmosphere, AtmosphereMode, AtmosphereSettings, ScatteringMedium};
use bevy::post_process::auto_exposure::{AutoExposure, AutoExposureCompensationCurve};
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;
use bevy::render::render_resource::BlendState;

pub fn setup_lighting(mut commands: Commands) {
    let sun_rotation = Quat::from_rotation_x(0.0);
    let moon_rotation = sun_rotation * Quat::from_rotation_y(PI);

    // Sun
    commands.spawn((
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
        RenderLayers::from_layers(&[SCENE_LAYER, MAP_LAYER]),
        DirectionalLight {
            illuminance: light_consts::lux::CIVIL_TWILIGHT,
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
                Earth,
                RenderLayers::layer(SCENE_LAYER),
                Orbital {
                    object_id: String::from("Earth"),
                    ..default()
                },
                Mesh3d(meshes.add(earth_mesh)),
                MeshMaterial3d(earth_material.clone()),
                Transform::from_xyz(0.0, 0.0, 0.0)
                    .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
            ))
            .id(),
    );

    // Set up Earth map rendering
    let map_earth_mesh = Sphere::new(EARTH_RADIUS / MAP_UNITS_TO_M)
        .mesh()
        .uv(512, 256);

    celestials.planets.insert(
        "Map_Earth".to_string(),
        commands
            .spawn((
                RenderLayers::layer(MAP_LAYER),
                Mesh3d(meshes.add(map_earth_mesh)),
                MeshMaterial3d(earth_material.clone()),
                Transform::from_xyz(0.0, 0.0, 0.0)
                    .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
            ))
            .id(),
    );
}

pub fn setup_entities(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut scattering_mediums: ResMut<Assets<ScatteringMedium>>,
    mut compensation_curves: ResMut<Assets<AutoExposureCompensationCurve>>,
    celestials: Res<Celestials>,
    devices: Res<Devices>,
    asset_server: Res<AssetServer>,
) {
    let test_sphere_mesh = Mesh::from(Sphere::new(1.0));

    // Origin Sphere
    commands.spawn((
        RenderLayers::layer(SCENE_LAYER),
        Mesh3d(meshes.add(test_sphere_mesh.clone())),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.2, 1.0, 0.2),
            perceptual_roughness: 1.0,
            ..default()
        })),
        Transform::from_xyz(0.0, 0.0, 0.0),
    ));

    // Skybox
    let skybox_handle: Handle<Image> = asset_server.load("textures/hdr-cubemap-2048x2048.ktx2");

    // Set up 3D scene camera
    commands.spawn((
        RenderLayers::layer(SCENE_LAYER),
        Camera3d::default(),
        Bloom {
            intensity: 0.01,
            ..default()
        },
        AutoExposure {
            filter: RangeInclusive::new(0.005, 0.995),
            speed_brighten: 5.0,
            speed_darken: 5.0,
            compensation_curve: compensation_curves.add(
                AutoExposureCompensationCurve::from_curve(LinearSpline::new([
                    vec2(-4.0, -4.0),
                    vec2(0.0, 0.0),
                    vec2(2.0, 1.0),
                ]))
                .unwrap(),
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
            scene_params: OrbitCameraParams {
                target: Some(
                    *devices
                        .tethers
                        .get("Tether1")
                        .expect("Tether1 has not been spawned but was expected."),
                ),
                ..default()
            },
            map_params: OrbitCameraParams {
                distance: EARTH_ATMOSPHERE_RADIUS / MAP_UNITS_TO_M
                    + 2.0 * (EARTH_ATMOSPHERE_RADIUS / MAP_UNITS_TO_M),
                min_distance: EARTH_ATMOSPHERE_RADIUS / MAP_UNITS_TO_M,
                target: Some(
                    *celestials
                        .planets
                        .get("Map_Earth")
                        .expect("The Earth has not been spawned but was expected."),
                ),
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
            ..default()
        },
    ));

    let scene: Handle<Scene> =
        asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/broken_satellite.glb"));

    commands.spawn((
        SceneRoot(scene),
        RigidBody::Dynamic,
        Orbital {
            elements: Some(ISS_ORBIT),
            object_id: String::from("Satellite1"),
            ..default()
        },
        ColliderConstructorHierarchy::new(ColliderConstructor::ConvexHullFromMesh),
        Transform::from_xyz(0.0, 4.0, 40.0),
    ));
}

pub fn setup_tether(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut devices: ResMut<Devices>,
) {
    let sphere_mesh = meshes.add(Mesh::from(Sphere::new(0.5)));
    let sphere_collider = Collider::sphere(0.5);
    let sphere_material = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        perceptual_roughness: 1.0,
        ..default()
    });

    // The root tether node
    let tether_root = commands
        .spawn((
            RenderLayers::layer(SCENE_LAYER),
            RigidBody::Dynamic,
            ConstantForce::new(0.0, 0.0, 0.0),
            sphere_collider.clone(),
            Mesh3d(sphere_mesh.clone()),
            MeshMaterial3d(sphere_material.clone()),
            Transform::from_xyz(0.0, 0.0, 0.0),
            Orbital {
                elements: Some(ISS_ORBIT),
                object_id: String::from("Tether1"),
                ..default()
            },
        ))
        .id();

    devices.tethers.insert("Tether1".to_string(), tether_root);

    let mut prev_sphere = tether_root;

    for i in 1..NUM_TETHER_JOINTS {
        let sphere = commands
            .spawn((
                RenderLayers::layer(SCENE_LAYER),
                TetherNode { root: tether_root },
                RigidBody::Dynamic,
                ConstantForce::new(0.0, 0.0, 0.0),
                sphere_collider.clone(),
                Mesh3d(sphere_mesh.clone()),
                MeshMaterial3d(sphere_material.clone()),
                Transform::from_xyz(0.0, i as f32 * DIST_BETWEEN_JOINTS, 0.0),
            ))
            .id();

        let anchor = Vec3::new(
            0.0,
            i as f32 * DIST_BETWEEN_JOINTS - DIST_BETWEEN_JOINTS / 2.0,
            0.0,
        );

        commands.spawn(SphericalJoint::new(prev_sphere, sphere).with_anchor(anchor));

        prev_sphere = sphere;
        commands.entity(tether_root).add_child(sphere);
    }
}

pub fn setup_user_interface(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    devices: ResMut<Devices>,
) {
    // UI camera
    commands.spawn((
        Camera2d::default(),
        RenderLayers::layer(UI_LAYER),
        Camera {
            order: 1,
            clear_color: ClearColorConfig::None,
            output_mode: CameraOutputMode::Write {
                blend_state: Some(BlendState::ALPHA_BLENDING),
                clear_color: ClearColorConfig::None,
            },
            ..default()
        },
    ));

    // Font
    let font = asset_server.load("fonts/FiraMono-Medium.ttf");

    commands
        .spawn((
            RenderLayers::layer(UI_LAYER),
            Node {
                width: percent(100),
                height: percent(100),
                flex_direction: FlexDirection::Column,

                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.0)),
        ))
        .with_children(|parent| {
            parent.spawn((
                TrackObject {
                    entity: Some(
                        *devices
                            .tethers
                            .get("Tether1")
                            .expect("Tether1 not instantiated!"),
                    ),
                },
                Text::new("TEST 1"),
                Node {
                    margin: UiRect::bottom(px(10)),
                    ..default()
                },
            ));

            // Orbit labels
            parent.spawn((
                OrbitLabel {
                    entity: Some(
                        *devices
                            .tethers
                            .get("Tether1")
                            .expect("Tether1 not instantiated!"),
                    ),
                },
                Text::new("─ Tether1"),
                TextFont {
                    font: font,
                    ..default()
                },
                Node {
                    position_type: PositionType::Absolute,
                    ..default()
                },
            ));
        });
}
