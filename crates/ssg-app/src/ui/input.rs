use bevy::{
    camera::visibility::RenderLayers,
    pbr::{Atmosphere, AtmosphereSettings},
    prelude::*,
};

use crate::render::camera::set_map_view;
use crate::sim::telemetry::{LogEvent, LogLevel};
use crate::sim::time::WorldTime;
use crate::ui::catalog_state::SpaceCatalogUiState;
use crate::ui::settings::Settings;

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
        let (render_layers, atmosphere, atmosphere_settings) = scene_camera.into_inner();

        if let Some(message) = set_map_view(render_layers, atmosphere, atmosphere_settings) {
            log.write(LogEvent {
                level: LogLevel::Debug,
                source: "ui",
                message: message.to_string(),
            });
        }
    }
}

pub fn change_time_warp(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    catalog_ui: Res<SpaceCatalogUiState>,
    mut world_time: ResMut<WorldTime>,
    mut log: MessageWriter<LogEvent>,
) {
    if catalog_ui.search_focused {
        return;
    }

    // Period steps up (unless at the top of the ladder), else Comma steps down.
    let changed = (keyboard_input.just_pressed(KeyCode::Period) && world_time.change_warp(true))
        || (keyboard_input.just_pressed(KeyCode::Comma) && world_time.change_warp(false));
    if changed {
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
        settings.toggle_capture_gizmos();
    }
}
