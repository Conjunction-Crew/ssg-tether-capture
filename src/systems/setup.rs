use std::f32::consts::PI;
use std::ops::RangeInclusive;

use crate::components::orbit::{Earth, Orbit, TetherNode, TrueParams};
use crate::components::orbit_camera::{CameraTarget, OrbitCamera, OrbitCameraParams};
use crate::constants::*;
use crate::resources::celestials::Celestials;
use crate::resources::orbital_entities::OrbitalEntities;
use crate::ui::state::UiScreen;

use avian3d::prelude::*;
use bevy::camera::visibility::RenderLayers;
use bevy::core_pipeline::Skybox;
use bevy::light::{CascadeShadowConfigBuilder, SunDisk};
use bevy::math::DVec3;
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
                RenderLayers::layer(SCENE_LAYER),
                Orbit::FromParams(TrueParams {
                    rv: Vector6::new(0.0, 0.0, 0.0, 0.0, 0.0, 0.0),
                }),
                Mesh3d(meshes.add(earth_mesh)),
                MeshMaterial3d(earth_material.clone()),
                Transform::from_xyz(0.0, 0.0, 0.0)
                    .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
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

    // Skybox
    let skybox_handle: Handle<Image> = asset_server.load("textures/hdr-cubemap-2048x2048.ktx2");

    // Set up 3D scene camera
    commands.spawn((
        DespawnOnExit(UiScreen::Sim),
        RenderLayers::layer(SCENE_LAYER),
        Camera3d::default(),
        Bloom {
            intensity: 0.01,
            ..default()
        },
        AutoExposure {
            filter: RangeInclusive::new(0.10, 0.90),
            speed_brighten: 3.0,
            speed_darken: 1.0,
            compensation_curve: compensation_curves.add(
                AutoExposureCompensationCurve::from_curve(LinearSpline::new([
                    vec2(-4.0, -1.0),
                    vec2(0.0, 0.75),
                    vec2(2.0, 1.0),
                    vec2(4.0, 1.5),
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
                distance: EARTH_ATMOSPHERE_RADIUS / MAP_UNITS_TO_M as f32
                    + 2.0 * (EARTH_ATMOSPHERE_RADIUS / MAP_UNITS_TO_M as f32),
                min_distance: EARTH_ATMOSPHERE_RADIUS / MAP_UNITS_TO_M as f32,
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
                DespawnOnExit(UiScreen::Sim),
                SceneRoot(scene),
                RigidBody::Dynamic,
                Orbit::FromElements(ISS_ORBIT),
                ColliderConstructorHierarchy::new(ColliderConstructor::ConvexHullFromMesh),
                CenterOfMass(Vec3::ZERO),
                Mass::from(2500.0),
                AngularVelocity {
                    0: DVec3::new(0.01, 0.01, 0.01),
                    ..default()
                },
                Transform::from_xyz(150.0, 0.0, 300.0),
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
    let root_tail_radius: f64 = 0.50;
    let rope_radius: f64 = 0.25;
    let tether_node_length = DIST_BETWEEN_JOINTS;
    let tether_node_half_length = tether_node_length * 0.5;

    let sphere_mesh = meshes.add(Mesh::from(Sphere::new(root_tail_radius as f32)));
    let sphere_collider = Collider::sphere(root_tail_radius);
    let sphere_material = materials.add(StandardMaterial {
        base_color: Color::Srgba(Srgba {
            red: 1.0,
            green: 0.0,
            blue: 0.0,
            alpha: 1.0,
        }),
        perceptual_roughness: 1.0,
        ..default()
    });

    let tether_node_mesh = Mesh::from(Cylinder::new((rope_radius / 8.0) as f32, tether_node_length as f32));
    let tether_node_collider = Collider::convex_hull_from_mesh(&tether_node_mesh).unwrap();
    let tether_node_mesh = meshes.add(tether_node_mesh);

    // The root tether node
    let tether_root = commands
        .spawn((
            DespawnOnExit(UiScreen::Sim),
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
    let mut prev_half_extent = root_tail_radius;
    let mut prev_y = 0.0;
    let interior_node_count =
        ((TETHER_LENGTH - 2.0 * root_tail_radius) / tether_node_length).max(0.0) as u32;
    let interval_count = interior_node_count + 1;
    let surface_gap = if interval_count > 0 {
        (TETHER_LENGTH - 2.0 * root_tail_radius - interior_node_count as f64 * tether_node_length)
            / interval_count as f64
    } else {
        0.0
    };
    let tail_index = interior_node_count + 1;

    for i in 1..=tail_index {
        let (mesh, collider, mass, curr_half_extent) = if i == tail_index {
            (
                sphere_mesh.clone(),
                sphere_collider.clone(),
                2.0,
                root_tail_radius,
            )
        } else {
            (
                tether_node_mesh.clone(),
                tether_node_collider.clone(),
                0.1,
                tether_node_half_length,
            )
        };

        let link_spacing = prev_half_extent + curr_half_extent + surface_gap;
        let y = prev_y + link_spacing;

        let sphere = commands
            .spawn((
                DespawnOnExit(UiScreen::Sim),
                RenderLayers::layer(SCENE_LAYER),
                TetherNode { root: tether_root },
                RigidBody::Dynamic,
                collider,
                Mesh3d(mesh),
                MeshMaterial3d(sphere_material.clone()),
                Mass::from(mass),
                Transform::from_xyz(0.0, y as f32, 0.0),
            ))
            .id();

        let anchor = DVec3::new(0.0, prev_y + prev_half_extent + surface_gap * 0.5, 0.0);

        commands.spawn((
            DespawnOnExit(UiScreen::Sim),
            // SphericalJoint::new(prev_sphere, sphere).with_anchor(anchor),
            DistanceJoint::new(prev_sphere, sphere).with_anchor(anchor),
            JointDamping {
                linear: 1.0,  // Linear damping
                angular: 1.0, // Angular damping
            },
            JointCollisionDisabled,
        ));

        prev_sphere = sphere;
        prev_half_extent = curr_half_extent;
        prev_y = y;
    }

    // Add tail node to tether entity
    orbital_entities
        .tethers
        .get_mut(&"Tether1".to_string())
        .expect("Error getting tether")
        .push(prev_sphere);
}