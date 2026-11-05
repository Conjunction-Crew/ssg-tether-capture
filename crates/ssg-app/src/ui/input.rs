use bevy::{
    camera::visibility::RenderLayers,
    pbr::{Atmosphere, AtmosphereSettings},
    prelude::*,
};
use bevy_egui::input::EguiWantsInput;
use bevy_egui::{EguiInput, egui};

use crate::render::camera::set_map_view;
use crate::ui::settings::Settings;
use ssg_sim::session::PauseState;
use ssg_sim::telemetry::{LogEvent, LogLevel};
use ssg_sim::time::{StepCap, WarpChange, WorldTime};

/// Run condition: egui is taking keyboard input (a focused text field or an
/// open popup), so the sim hotkeys must not act.
pub fn egui_wants_keyboard(egui_wants_input: Option<Res<EguiWantsInput>>) -> bool {
    egui_wants_input.is_some_and(|wants| wants.wants_any_keyboard_input())
}

/// Removes Tab from `raw`, so egui never moves its focus with it (P0-006).
pub(crate) fn strip_tab(raw: &mut egui::RawInput) {
    raw.events.retain(|event| {
        !matches!(
            event,
            egui::Event::Key {
                key: egui::Key::Tab,
                ..
            }
        )
    });
}

/// On the sim screen Tab cycles the camera target and nothing else (P0-006).
pub fn keep_tab_from_egui(mut inputs: Query<&mut EguiInput>) {
    for mut input in &mut inputs {
        strip_tab(&mut input.0);
    }
}

pub fn toggle_map_view(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    scene_camera: Single<(&mut RenderLayers, &mut Atmosphere, &mut AtmosphereSettings)>,
    mut log: MessageWriter<LogEvent>,
) {
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
/// pause state and logs the change. Shared by the keyboard and the UI button. A
/// step up asks for at most 2 × the cap: enough for the cap to grow on a fast
/// machine, and `,` shows a change within two presses (F-46, OQ2).
pub(crate) fn step_time_warp(
    increase: bool,
    world_time: &mut WorldTime,
    cap: StepCap,
    pause: &State<PauseState>,
    next_pause: &mut NextState<PauseState>,
    log: &mut MessageWriter<LogEvent>,
) {
    let paused = *pause.get() == PauseState::Paused;
    if increase && !paused && world_time.multiplier >= cap.0.saturating_mul(2) {
        return;
    }
    let message = match world_time.change_warp(increase, paused) {
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
    mut world_time: ResMut<WorldTime>,
    cap: Res<StepCap>,
    pause: Res<State<PauseState>>,
    mut next_pause: ResMut<NextState<PauseState>>,
    mut log: MessageWriter<LogEvent>,
) {
    // Period steps up, else Comma steps down.
    let increase = if keyboard_input.just_pressed(KeyCode::Period) {
        true
    } else if keyboard_input.just_pressed(KeyCode::Comma) {
        false
    } else {
        return;
    };
    step_time_warp(
        increase,
        &mut world_time,
        *cap,
        &pause,
        &mut next_pause,
        &mut log,
    );
}

pub fn toggle_origin(keyboard_input: Res<ButtonInput<KeyCode>>, mut settings: ResMut<Settings>) {
    if keyboard_input.just_pressed(KeyCode::KeyO) {
        settings.dev_gizmos = !settings.dev_gizmos;
    }
}

pub fn toggle_capture_gizmos(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut settings: ResMut<Settings>,
) {
    if keyboard_input.just_pressed(KeyCode::KeyC) {
        settings.toggle_capture_gizmos();
    }
}
