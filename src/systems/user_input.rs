use crate::{
    components::dev_components::Origin,
    constants::{MAP_LAYER, MAP_UNITS_TO_M, SCENE_LAYER},
    resources::time_warp::TimeWarp,
};

use avian3d::prelude::{Physics, PhysicsTime};
use bevy::{
    camera::visibility::RenderLayers,
    pbr::{Atmosphere, AtmosphereSettings},
    prelude::*,
};

pub fn toggle_map_view(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    scene_camera: Single<(&mut RenderLayers, &mut Atmosphere, &mut AtmosphereSettings)>,
) {
    if keyboard_input.just_pressed(KeyCode::KeyM) {
        let (mut render_layers, mut atmosphere, mut atmosphere_settings) =
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

const MAX_TIME_WARP: f64 = 1000.0;
const MIN_TIME_WARP: f64 = 0.001;

pub fn change_time_warp(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut time_warp: ResMut<TimeWarp>,
    mut physics_time: ResMut<Time<Physics>>,
) {
    if keyboard_input.just_pressed(KeyCode::Period) && time_warp.multiplier * 2.0 <= MAX_TIME_WARP
    {
        time_warp.multiplier *= 2.0;
    } else if keyboard_input.just_pressed(KeyCode::Comma)
        && time_warp.multiplier / 2.0 >= MIN_TIME_WARP
    {
        time_warp.multiplier /= 2.0;
    }

    if time_warp.multiplier > 4.0 {
        physics_time.pause();
    } else {
        physics_time.unpause();
        physics_time.set_relative_speed_f64(time_warp.multiplier as f64);
    }
}

pub fn toggle_origin(
    mut commands: Commands,
    keyboard_input: Res<ButtonInput<KeyCode>>,
    origin: Single<(Entity, &Visibility), With<Origin>>,
) {
    if keyboard_input.just_pressed(KeyCode::KeyO) {
        let (origin_entity, origin_vis) = origin.into_inner();

        match origin_vis {
            Visibility::Visible => {
                commands.entity(origin_entity).insert(Visibility::Hidden);
            }
            Visibility::Hidden => {
                commands.entity(origin_entity).insert(Visibility::Visible);
            }

            Visibility::Inherited => {}
        }
    }
}
