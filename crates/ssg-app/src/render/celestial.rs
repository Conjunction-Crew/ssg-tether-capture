//! The Earth, its map twin and the lights.

use std::f32::consts::PI;

use avian3d::prelude::*;
use bevy::camera::visibility::RenderLayers;
use bevy::light::{CascadeShadowConfigBuilder, SunDisk};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;

use bevy::pbr::Atmosphere;

use crate::render::camera::setup_camera;
use crate::render::layers::*;
use crate::ui::replay::{ReplaySession, show_replay_frame};
use ssg_sim::SimSet;
use ssg_sim::session::{SimState, UiScreen};

// Component to query Earth.
#[derive(Component)]
pub struct Earth;

#[derive(Resource, Debug, Default)]
pub struct Celestials {
    pub planets: HashMap<String, Entity>,
}

pub fn setup_lighting(mut commands: Commands) {
    // The sun shines along ECI +Y, in the equatorial plane: the direction it had
    // before the scene moved to ECI axes (F-08).
    let sun_rotation = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
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
                Transform::from_xyz(0.0, 0.0, 0.0).with_rotation(earth_mesh_rotation()),
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
                Transform::from_xyz(0.0, 0.0, 0.0).with_rotation(earth_mesh_rotation()),
            ))
            .id(),
    );
}

/// The Earth's translation in the replay scene (R6, C2, C7): minus the frame
/// origin's ECI position, because every replay body sits at its bubble-frame
/// position. Unlike the live scene it needs no body: the player interpolates the
/// frame origin origin-aware, so the Earth moves continuously across a
/// floating-origin reset.
pub fn replay_earth_translation(frame_origin_eci: &[f64; 6]) -> Vec3 {
    -Vec3::new(
        frame_origin_eci[0] as f32,
        frame_origin_eci[1] as f32,
        frame_origin_eci[2] as f32,
    )
}

/// Places the Earth and its atmosphere from the player's frame origin while a
/// recording plays. A recording without a frame origin leaves them where they are.
pub fn place_replay_earth(
    session: Res<ReplaySession>,
    mut earth: Single<&mut Transform, With<Earth>>,
    camera: Single<(&mut Atmosphere, &RenderLayers), With<Camera3d>>,
) {
    let (mut atmosphere, render_layers) = camera.into_inner();
    // The map view keeps the Earth at the map's origin.
    if render_layers.intersects(&RenderLayers::layer(MAP_LAYER)) {
        return;
    }
    let Some(origin) = session.player.view().frame_origin_eci else {
        return;
    };
    let translation = replay_earth_translation(&origin);
    earth.translation = translation;
    atmosphere.world_position = translation;
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
        .add_systems(OnExit(UiScreen::Sim), remove_celestials)
        .add_systems(
            Update,
            place_replay_earth
                .after(show_replay_frame)
                .run_if(in_state(SimState::Replay)),
        );
    }
}
