use bevy::{
    camera::visibility::RenderLayers,
    pbr::{Atmosphere, AtmosphereSettings},
    prelude::*,
};

use crate::render::camera::set_map_view;
use crate::ui::catalog_state::SpaceCatalogUiState;
use crate::ui::settings::Settings;
use ssg_sim::session::PauseState;
use ssg_sim::telemetry::{LogEvent, LogLevel};
use ssg_sim::time::{WarpChange, WorldTime};

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

/// One step on the time-warp ladder ([`WorldTime::change_warp`]): applies the
/// pause state and logs the change. Shared by the keyboard and the UI button.
pub(crate) fn step_time_warp(
    increase: bool,
    world_time: &mut WorldTime,
    pause: &State<PauseState>,
    next_pause: &mut NextState<PauseState>,
    log: &mut MessageWriter<LogEvent>,
) {
    let message = match world_time.change_warp(increase, *pause.get() == PauseState::Paused) {
        WarpChange::Unchanged => return,
        WarpChange::Multiplier(multiplier) => format!("Time warp → {multiplier}x"),
        WarpChange::Paused => {
            next_pause.set(PauseState::Paused);
            "Sim paused".to_string()
        }
        WarpChange::Resumed => {
            next_pause.set(PauseState::Running);
            "Sim resumed".to_string()
        }
    };
    log.write(LogEvent {
        level: LogLevel::Info,
        source: "ui",
        message,
    });
}

pub fn change_time_warp(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    catalog_ui: Res<SpaceCatalogUiState>,
    mut world_time: ResMut<WorldTime>,
    pause: Res<State<PauseState>>,
    mut next_pause: ResMut<NextState<PauseState>>,
    mut log: MessageWriter<LogEvent>,
) {
    if catalog_ui.search_focused {
        return;
    }

    // Period steps up, else Comma steps down.
    let increase = if keyboard_input.just_pressed(KeyCode::Period) {
        true
    } else if keyboard_input.just_pressed(KeyCode::Comma) {
        false
    } else {
        return;
    };
    step_time_warp(increase, &mut world_time, &pause, &mut next_pause, &mut log);
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
