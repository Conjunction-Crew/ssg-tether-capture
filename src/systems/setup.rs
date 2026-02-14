use crate::components::orbit_camera::OrbitCamera;
use crate::components::orbit::Orbital;

use astrora_core::core::elements::OrbitalElements;
use avian3d::prelude::*;
use bevy::camera::Camera3dDepthLoadOp;
use bevy::camera::visibility::RenderLayers;
use bevy::light::{CascadeShadowConfigBuilder, SunDisk};
use bevy::pbr::{Atmosphere, AtmosphereMode, AtmosphereSettings, ScatteringMedium};
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;
use std::f32::consts::PI;

// Camera layers
const CELESTIAL_LAYER: usize = 0;
const LOCAL_LAYER: usize = 1;

// Earth constants
const EARTH_RADIUS: f32 = 6_360_000.0;
const EARTH_ATMOSPHERE_RADIUS: f32 = 6_460_000.0;
pub const EARTH_Y_OFFSET: f32 = EARTH_RADIUS / 1000.0;

// Other constants
const INITIAL_HEIGHT_KM: f32 = 400.0;
const ISS_ORBIT: OrbitalElements = OrbitalElements {
    // Semi-major axis (meters)
    a: 6_799_130.0,
    // Eccentricity (dimensionless)
    e: 0.00112,
    // Inclination (radians)
    i: 0.90114,
    // Right ascension of ascending node (radians)
    raan: 3.54993,
    // Argument of periapsis (radians)
    argp: 1.51296,
    // True anomaly (radians)
    nu: 4.77190
};

// Setup the orbital simulation environment
pub fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut scattering_mediums: ResMut<Assets<ScatteringMedium>>,
    asset_server: Res<AssetServer>,
) {
    // Set up celestial camera to render Sun / Earth
    commands.spawn((
        RenderLayers::layer(CELESTIAL_LAYER),
        Camera3d::default(),
        Orbital {
            elements: Some(ISS_ORBIT),
            ..default()
        },
        Transform::from_xyz(0.0, 0.0, 0.0).looking_at(Vec3::NEG_Y, Vec3::Z),
        Atmosphere {
            bottom_radius: EARTH_RADIUS,
            top_radius: EARTH_ATMOSPHERE_RADIUS,
            ground_albedo: Vec3::splat(0.3),
            medium: scattering_mediums.add(ScatteringMedium::default()),
        },
        AtmosphereSettings {
            rendering_method: AtmosphereMode::Raymarched,
            scene_units_to_m: 1000.0,
            ..default()
        },
    ));

    let earth_mesh = Sphere::new(EARTH_RADIUS / 1000.0).mesh().uv(128, 64);
    let earth_texture: Handle<Image> = asset_server.load("textures/earth.jpg");

    commands.spawn((
        RenderLayers::layer(CELESTIAL_LAYER),
        Mesh3d(meshes.add(earth_mesh)),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color_texture: Some(earth_texture),
            perceptual_roughness: 1.0,
            ..default()
        })),
        Transform::from_xyz(0.0, -EARTH_Y_OFFSET, 0.0),
    ));

    // Set up local camera to render Rigid Body physics objects
    commands.spawn((
        RenderLayers::layer(LOCAL_LAYER),
        Camera3d {
            depth_load_op: Camera3dDepthLoadOp::Clear(0.0),
            ..default()
        },
        Camera {
            order: 1,
            clear_color: ClearColorConfig::None,
            ..default()
        },
        Transform::from_xyz(0.0, 0.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y),
        OrbitCamera::default(),
        Bloom::NATURAL,
    ));

    let test_sphere_mesh = Mesh::from(Sphere::new(1.0));

    commands.spawn((
        RigidBody::Dynamic,
        RenderLayers::layer(LOCAL_LAYER),
        ConstantForce::new(0.0, 0.0, 0.0),
        Collider::convex_hull_from_mesh(&test_sphere_mesh).unwrap(),
        Mesh3d(meshes.add(test_sphere_mesh.clone())),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::WHITE,
            perceptual_roughness: 1.0,
            ..default()
        })),
        Transform::from_xyz(-10.0, 0.0, 0.0),
    ));

    commands.spawn((
        RenderLayers::layer(LOCAL_LAYER),
        RigidBody::Dynamic,
        ConstantForce::new(0.0, 0.0, 0.0),
        Collider::convex_hull_from_mesh(&test_sphere_mesh.clone()).unwrap(),
        Mesh3d(meshes.add(test_sphere_mesh.clone())),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::WHITE,
            perceptual_roughness: 1.0,
            ..default()
        })),
        Transform::from_xyz(10.0, 0.0, 0.0),
    ));

    commands.spawn((
        RenderLayers::layer(LOCAL_LAYER),
        DirectionalLight {
            illuminance: light_consts::lux::AMBIENT_DAYLIGHT,
            shadows_enabled: true,
            ..default()
        },
        SunDisk::EARTH,
        Transform {
            translation: Vec3::new(0.0, 2.0, 0.0),
            rotation: Quat::from_rotation_x(-PI / 4.),
            ..default()
        },
        CascadeShadowConfigBuilder {
            first_cascade_far_bound: 4.0,
            maximum_distance: 1000.0,
            ..default()
        }
        .build(),
    ));

    commands.spawn((
        RenderLayers::layer(CELESTIAL_LAYER).with(LOCAL_LAYER),
        DirectionalLight {
            illuminance: light_consts::lux::AMBIENT_DAYLIGHT,
            shadows_enabled: true,
            ..default()
        },
        SunDisk::EARTH,
        Transform {
            translation: Vec3::new(0.0, 2.0, 0.0),
            rotation: Quat::from_rotation_x(-PI / 4.),
            ..default()
        },
        CascadeShadowConfigBuilder {
            first_cascade_far_bound: 4.0,
            maximum_distance: 1000.0,
            ..default()
        }
        .build(),
    ));
}
