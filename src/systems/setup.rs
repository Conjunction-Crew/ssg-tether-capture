use std::f32::consts::PI;
use std::fs;
use std::ops::RangeInclusive;
use std::path::PathBuf;

use crate::components::dev_components::Origin;
use crate::components::orbit::{Earth, Orbit, TetherNode, TrueParams};
use crate::components::orbit_camera::{CameraTarget, OrbitCamera, OrbitCameraParams};
use crate::constants::*;
use crate::resources::capture_plans::CapturePlanLibrary;
use crate::resources::celestials::Celestials;
use crate::resources::orbital_entities::OrbitalEntities;

use avian3d::prelude::*;
use bevy::camera::visibility::RenderLayers;
use bevy::color::palettes::css::WHITE;
use bevy::core_pipeline::Skybox;
use bevy::light::{CascadeShadowConfigBuilder, SunDisk};
use bevy::math::cubic_splines::LinearSpline;
use bevy::pbr::{Atmosphere, AtmosphereMode, AtmosphereSettings, ScatteringMedium};
use bevy::post_process::auto_exposure::{AutoExposure, AutoExposureCompensationCurve};
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;

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
                Orbit::FromParams(TrueParams {
                    r: [0.0, 0.0, 0.0],
                    v: [0.0, 0.0, 0.0],
                }),
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
    mut orbital_entities: ResMut<OrbitalEntities>,
    asset_server: Res<AssetServer>,
) {
    let test_sphere_mesh = Mesh::from(Sphere::new(1.0));

    // Origin Sphere
    commands.spawn((
        Origin,
        Visibility::Hidden,
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
            scene_params: OrbitCameraParams::default(),
            map_params: OrbitCameraParams {
                distance: EARTH_ATMOSPHERE_RADIUS / MAP_UNITS_TO_M
                    + 2.0 * (EARTH_ATMOSPHERE_RADIUS / MAP_UNITS_TO_M),
                min_distance: EARTH_ATMOSPHERE_RADIUS / MAP_UNITS_TO_M,
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

    orbital_entities.debris.insert(
        "Satellite1".to_string(),
        commands
            .spawn((
                SceneRoot(scene),
                RigidBody::Dynamic,
                Orbit::FromElements(ISS_ORBIT),
                ColliderConstructorHierarchy::new(ColliderConstructor::ConvexHullFromMesh),
                CenterOfMass(Vec3::ZERO),
                Mass::from(2500.0),
                AngularVelocity {
                    0: Vec3::new(0.2, 0.2, 0.0),
                    ..default()
                },
                Transform::from_xyz(0.0, 4.0, 20.0),
            ))
            .id(),
    );
}

pub fn setup_tether(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut orbital_entities: ResMut<OrbitalEntities>,
) {
    let root_tail_radius = 0.50;
    let rope_radius = 0.25;

    let sphere_mesh = meshes.add(Mesh::from(Sphere::new(root_tail_radius)));
    let sphere_collider = Collider::sphere(root_tail_radius);
    let sphere_material = materials.add(StandardMaterial {
        base_color: Color::Srgba(Srgba { red: 1.0, green: 0.0, blue: 0.0, alpha: 1.0 }),
        perceptual_roughness: 1.0,
        ..default()
    });

    let sphere_mesh_small = meshes.add(Mesh::from(Sphere::new(rope_radius)));
    let sphere_collider_small = Collider::sphere(rope_radius);

    // The root tether node
    let tether_root = commands
        .spawn((
            CameraTarget,
            RenderLayers::layer(SCENE_LAYER),
            RigidBody::Dynamic,
            sphere_collider.clone(),
            Mesh3d(sphere_mesh.clone()),
            MeshMaterial3d(sphere_material.clone()),
            Mass::from(2.0),
            Transform::from_xyz(0.0, 0.0, 0.0),
            Orbit::FromElements(ISS_ORBIT),
        ))
        .id();

    orbital_entities
        .tethers
        .insert("Tether1".to_string(), vec![tether_root]);

    let mut prev_sphere = tether_root;
    let mut prev_radius = root_tail_radius;
    let mut prev_y = 0.0;
    let surface_gap = 0.00;

    for i in 1..NUM_TETHER_JOINTS {
        let (mesh, collider, mass, curr_radius) = if i == NUM_TETHER_JOINTS - 1 {
            (sphere_mesh.clone(), sphere_collider.clone(), 1.0, root_tail_radius)
        } else {
            (
                sphere_mesh_small.clone(),
                sphere_collider_small.clone(),
                0.1,
                rope_radius,
            )
        };

        let link_spacing = prev_radius + curr_radius + surface_gap;
        let y = prev_y + link_spacing;

        let sphere = commands
            .spawn((
                RenderLayers::layer(SCENE_LAYER),
                TetherNode { root: tether_root },
                RigidBody::Dynamic,
                collider,
                Mesh3d(mesh),
                MeshMaterial3d(sphere_material.clone()),
                Mass::from(mass),
                Transform::from_xyz(0.0, y, 0.0),
            ))
            .id();

        let anchor = Vec3::new(0.0, prev_y + link_spacing * 0.5, 0.0);

        commands.spawn(SphericalJoint::new(prev_sphere, sphere).with_anchor(anchor));

        prev_sphere = sphere;
        prev_radius = curr_radius;
        prev_y = y;
    }

    // Add tail node to tether entity
    orbital_entities
        .tethers
        .get_mut(&"Tether1".to_string())
        .expect("Error getting tether")
        .push(prev_sphere);
}

pub fn load_capture_plans(mut capture_plan_lib: ResMut<CapturePlanLibrary>) {
    let plans_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/capture_plans");

    for plan_file_result in fs::read_dir(&plans_dir).expect("failed to read capture_plans dir") {
        if let Ok(plan_file) = plan_file_result {
            let path = plan_file.path();
            if path
                .extension()
                .is_some_and(|extension| extension == "json")
            {
                if let Ok(raw_json) = fs::read_to_string(&path) {
                    if let Ok(plan) = serde_json::from_str(&raw_json) {
                        if let Some(plan_id) = path.file_stem() {
                            capture_plan_lib.plans.insert(
                                String::from(plan_id.to_str().expect("failed to get plan name!")),
                                plan,
                            );
                        }
                    } else {
                        println!("Failed to parse plan json");
                    }
                }
            }
        }
    }
}
