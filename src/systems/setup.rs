use std::f32::consts::PI;

use crate::components::orbit::*;
use crate::components::orbit_camera::OrbitCamera;
use avian3d::prelude::*;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::light::CascadeShadowConfigBuilder;
use bevy::light::SunDisk;
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;

// Setup the orbital simulation environment
pub fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    asset_server: Res<AssetServer>,
) {
    commands.spawn((
        Camera3d::default(),
        Tonemapping::TonyMcMapface,
        Bloom::NATURAL,
        Transform::from_xyz(-200.0, 200.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y),
        OrbitCamera::new(Vec3::new(0.0, 0.0, 0.0), 10.0),
    ));

    let test_sphere_mesh = Mesh::from(Sphere::new(100.0));

    commands.spawn((
        RigidBody::Dynamic,
        ConstantForce::new(0.0, 0.0, 0.0),
        Collider::convex_hull_from_mesh(&test_sphere_mesh).unwrap(),
        Mesh3d(meshes.add(test_sphere_mesh)),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::WHITE,
            perceptual_roughness: 1.0,
            ..default()
        })),
        Transform::from_xyz(0.0, 0.0, 0.0),
    ));

    commands.spawn(
        (OrbitalInfo {
            orbit: Some(OrbitElements {
                inclination_deg: 5.14,
                raan_deg: todo!(),
                eccentricity: todo!(),
                arg_perigee_deg: todo!(),
                mean_anomaly_deg: todo!(),
                mean_motion_rev_per_day: todo!(),
                epoch_utc: todo!(),
            }),
            object_id: todo!(),
            primary_id: todo!(),
            tle: todo!(),
            attitude: todo!(),
            state: todo!(),
            approach: todo!(),
        }),
    );

    commands.spawn((
        DirectionalLight {
            illuminance: light_consts::lux::OVERCAST_DAY,
            shadows_enabled: true,
            ..default()
        },
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
