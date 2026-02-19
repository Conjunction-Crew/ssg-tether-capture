use crate::components::orbit::Earth;
use crate::components::orbit_camera::OrbitCamera;
use crate::constants::{MAP_LAYER, MAP_UNITS_TO_M, SCENE_LAYER};

use bevy::{
    camera::visibility::RenderLayers,
    pbr::{Atmosphere, AtmosphereSettings},
    prelude::*,
};

pub fn toggle_map_view(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    scene_camera: Single<(
        &mut OrbitCamera,
        &mut RenderLayers,
        &mut Atmosphere,
        &mut AtmosphereSettings,
    )>,
    earth: Single<&mut Transform, With<Earth>>,
) {
    if keyboard_input.just_pressed(KeyCode::KeyM) {
        let (mut orbit_camera, mut render_layers, mut atmosphere, mut atmosphere_settings) =
            scene_camera.into_inner();

        if render_layers.intersects(&RenderLayers::layer(SCENE_LAYER)) {
            println!("Switching to map view");
            *render_layers = RenderLayers::layer(MAP_LAYER);

            // Adjust atmosphere
            atmosphere.world_position = Vec3::ZERO;
            atmosphere_settings.scene_units_to_m = MAP_UNITS_TO_M;
        } else if render_layers.intersects(&RenderLayers::layer(MAP_LAYER)) {
            println!("Switching to scene view");
            *render_layers = RenderLayers::layer(SCENE_LAYER);

            // Adjust atmosphere
            atmosphere_settings.scene_units_to_m = 1.0;
        }
    }
}
