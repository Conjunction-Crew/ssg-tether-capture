use crate::components::orbit::Orbital;
use crate::components::orbit_camera::OrbitCamera;

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
// const CELESTIAL_LAYER: usize = 0;
// const LOCAL_LAYER: usize = 1;

// Scale factor of celestial layer objects (1:1000 size for rendering)
// pub const CELESTIAL_UNITS_TO_M: f64 = 1000.0;

// Earth constants
const EARTH_RADIUS: f32 = 6_360_000.0;
const EARTH_ATMOSPHERE_RADIUS: f32 = 6_460_000.0;
// pub const EARTH_Y_OFFSET: f32 = EARTH_RADIUS / CELESTIAL_UNITS_TO_M as f32;

// Other constants
const ISS_ORBIT: OrbitalElements = OrbitalElements {
    // Semi-major axis (meters)
    a: 6_799_130.0,
    // Eccentricity (dimensionless)
    e: 0.00112,
    // Inclination (radians)
    i: 0.90114,
    // Right ascension of ascending node (radians)
    raan: 3.54993,
    // raan: 6.28,
    // Argument of periapsis (radians)
    argp: 1.51296,
    // True anomaly (radians)
    nu: 4.77190,
};

pub fn setup_lighting(mut commands: Commands) {
    commands.spawn((
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
            first_cascade_far_bound: 200.0,
            maximum_distance: 20_000.0,
            ..default()
        }
        .build(),
    ));
}

pub fn setup_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut scattering_mediums: ResMut<Assets<ScatteringMedium>>,
    asset_server: Res<AssetServer>,
) {
    // Set up Earth rendering
    let earth_mesh = Sphere::new(EARTH_RADIUS).mesh().uv(128, 64);
    let earth_texture: Handle<Image> = asset_server.load("textures/earth.jpg");

    let earth = commands.spawn((
        Orbital {
            object_id: String::from("Earth"),
            ..default()
        },
        Mesh3d(meshes.add(earth_mesh)),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color_texture: Some(earth_texture),
            perceptual_roughness: 1.0,
            ..default()
        })),
        Transform::from_xyz(0.0, 0.0, 0.0),
    )).id();

    let test_sphere_mesh = Mesh::from(Sphere::new(1.0));

    // Sphere 1
    let sphere_1 = commands.spawn((
        RigidBody::Dynamic,
        Orbital {
            elements: Some(ISS_ORBIT),
            object_id: String::from("Sphere1"),
            parent_entity: Some(earth),
            ..default()
        },
        ConstantForce::new(0.0, 0.0, 0.0),
        Collider::convex_hull_from_mesh(&test_sphere_mesh).unwrap(),
        Mesh3d(meshes.add(test_sphere_mesh.clone())),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::WHITE,
            perceptual_roughness: 1.0,
            ..default()
        })),
        Transform::from_xyz(-10.0, 0.0, 0.0),
    )).id();

    // Sphere 2
    commands.spawn((
        RigidBody::Dynamic,
        Orbital {
            elements: Some(ISS_ORBIT),
            object_id: String::from("Sphere2"),
            parent_entity: Some(earth),
            ..default()
        },
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

    // Set up 3D scene camera
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 0.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y),
        OrbitCamera {
            target: Some(sphere_1),
            ..default()
        },
        Atmosphere {
            world_position: Vec3::new(0.0 as f32, 0.0, 0.0),
            bottom_radius: EARTH_RADIUS,
            top_radius: EARTH_ATMOSPHERE_RADIUS,
            ground_albedo: Vec3::splat(0.3),
            medium: scattering_mediums.add(ScatteringMedium::default()),
        },
        AtmosphereSettings {
            rendering_method: AtmosphereMode::Raymarched,
            ..default()
        },
    ));
}
