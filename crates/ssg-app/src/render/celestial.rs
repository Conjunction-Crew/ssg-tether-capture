//! The Earth, its map twin and the lights.

use std::f32::consts::PI;

use avian3d::prelude::*;
use bevy::camera::visibility::RenderLayers;
use bevy::light::{CascadeShadowConfigBuilder, SunDisk};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;

use crate::render::camera::setup_camera;
use crate::render::layers::*;
use crate::sim::SimSet;
use crate::sim::session::UiScreen;

// Component to query Earth.
#[derive(Component)]
pub struct Earth;

#[derive(Resource, Debug, Default)]
pub struct Celestials {
    pub planets: HashMap<String, Entity>,
}

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

fn init_celestials(mut commands: Commands) {
    commands.init_resource::<Celestials>();
}

fn remove_celestials(mut commands: Commands) {
    commands.remove_resource::<Celestials>();
}

pub struct CelestialPlugin;

impl Plugin for CelestialPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            OnEnter(UiScreen::Sim),
            (
                init_celestials,
                setup_lighting,
                setup_celestial,
                setup_camera,
            )
                .chain()
                .after(SimSet::SessionInit),
        )
        .add_systems(OnExit(UiScreen::Sim), remove_celestials);
    }
}
