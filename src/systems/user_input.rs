use crate::{
    constants::{MAP_LAYER, MAP_UNITS_TO_M, SCENE_LAYER},
    resources::{
        capture_log::{LogEvent, LogLevel},
        settings::Settings,
        space_catalog::SpaceCatalogUiState,
        world_time::WorldTime,
    },
};

use bevy::{
    camera::visibility::RenderLayers,
    pbr::{Atmosphere, AtmosphereSettings},
    prelude::*,
};

pub fn toggle_map_view(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    catalog_ui: Res<SpaceCatalogUiState>,
    scene_camera: Single<(&mut RenderLayers, &mut Atmosphere, &mut AtmosphereSettings)>,
    mut log: MessageWriter<LogEvent>,
) {
    if catalog_ui.search_focused {
        return;
    }

    if keyboard_input.just_pressed(KeyCode::KeyM) {
        let (mut render_layers, mut atmosphere, mut atmosphere_settings) =
            scene_camera.into_inner();

        if render_layers.intersects(&RenderLayers::layer(SCENE_LAYER)) {
            *render_layers = RenderLayers::layer(MAP_LAYER);

            // Adjust atmosphere
            atmosphere.world_position = Vec3::ZERO;
            atmosphere_settings.scene_units_to_m = MAP_UNITS_TO_M as f32;
            log.write(LogEvent {
                level: LogLevel::Debug,
                source: "ui",
                message: "Switched to map view".to_string(),
            });
        } else if render_layers.intersects(&RenderLayers::layer(MAP_LAYER)) {
            *render_layers = RenderLayers::layer(SCENE_LAYER);

            // Adjust atmosphere
            atmosphere_settings.scene_units_to_m = 1.0;
            log.write(LogEvent {
                level: LogLevel::Debug,
                source: "ui",
                message: "Switched to scene view".to_string(),
            });
        }
    }
}

const MAX_TIME_WARP: u32 = 10000;
const MIN_TIME_WARP: u32 = 1;

pub fn change_time_warp(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    catalog_ui: Res<SpaceCatalogUiState>,
    mut world_time: ResMut<WorldTime>,
    mut log: MessageWriter<LogEvent>,
) {
    if catalog_ui.search_focused {
        return;
    }

    let prev = world_time.multiplier;
    if keyboard_input.just_pressed(KeyCode::Period) && world_time.multiplier * 2 <= MAX_TIME_WARP {
        world_time.multiplier *= 2;
    } else if keyboard_input.just_pressed(KeyCode::Comma)
        && world_time.multiplier / 2 >= MIN_TIME_WARP
    {
        world_time.multiplier /= 2;
    }
    if world_time.multiplier != prev {
        log.write(LogEvent {
            level: LogLevel::Info,
            source: "ui",
            message: format!("Time warp → {}x", world_time.multiplier),
        });
    }
}

pub fn toggle_origin(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    catalog_ui: Res<SpaceCatalogUiState>,
    mut settings: ResMut<Settings>,
) {
    if catalog_ui.search_focused {
        return;
    }

    if keyboard_input.just_pressed(KeyCode::KeyO) {
        settings.dev_gizmos = !settings.dev_gizmos;
    }
}

pub fn toggle_capture_gizmos(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    catalog_ui: Res<SpaceCatalogUiState>,
    mut settings: ResMut<Settings>,
) {
    if catalog_ui.search_focused {
        return;
    }

    if keyboard_input.just_pressed(KeyCode::KeyC) {
        settings.capture_gizmos = !settings.capture_gizmos;
    }
}
