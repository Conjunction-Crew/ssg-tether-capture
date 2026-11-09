//! The capture log holds one sim session (#122, F-44): leaving the sim clears it,
//! lines logged on Home after that are kept into the next session (F-42), and the
//! panel starts collapsed with its filters kept.

use bevy::prelude::*;

use crate::tests::app_support::{enter_sim, test_app};
use crate::ui::log_ui::CaptureLogUiState;
use crate::ui::plugin::{clear_capture_log, reset_terminal_log_ui_state};
use crate::ui::screen::UiScreen;
use ssg_sim::telemetry::{CaptureLog, LogEvent, LogLevel};

/// The app's two capture-log systems on the screen transitions, as the UI
/// plugin registers them.
fn log_app() -> App {
    let mut app = test_app();
    app.init_resource::<CaptureLogUiState>()
        .add_systems(OnEnter(UiScreen::Sim), reset_terminal_log_ui_state)
        .add_systems(OnExit(UiScreen::Sim), clear_capture_log);
    app
}

fn go_home(app: &mut App) {
    app.world_mut()
        .resource_mut::<NextState<UiScreen>>()
        .set(UiScreen::Home);
    app.update();
}

fn log(app: &mut App, message: &str) {
    app.world_mut().write_message(LogEvent {
        level: LogLevel::Info,
        source: "test",
        message: message.to_string(),
    });
    app.update();
}

fn messages(app: &App) -> Vec<String> {
    app.world()
        .resource::<CaptureLog>()
        .entries
        .iter()
        .map(|e| e.message.clone())
        .collect()
}

#[test]
fn reentering_the_sim_shows_only_the_new_session_and_later_home_lines() {
    let mut app = log_app();
    enter_sim(&mut app);
    log(&mut app, "first session");
    assert!(messages(&app).contains(&"first session".to_string()));

    go_home(&mut app);
    assert!(
        messages(&app).is_empty(),
        "cleared on leaving: {:?}",
        messages(&app)
    );
    log(&mut app, "on home");

    enter_sim(&mut app);
    log(&mut app, "second session");

    let messages = messages(&app);
    assert!(
        !messages.contains(&"first session".to_string()),
        "{messages:?}"
    );
    let home = messages.iter().position(|m| m == "on home");
    let second = messages.iter().position(|m| m == "second session");
    assert!(home.is_some() && second.is_some(), "{messages:?}");
    assert!(home < second, "{messages:?}");
    assert!(
        messages.iter().any(|m| m.starts_with("Sim start epoch:")),
        "the new session's start line: {messages:?}"
    );
}

#[test]
fn reentering_the_sim_collapses_the_log_and_keeps_its_filters() {
    let mut app = log_app();
    enter_sim(&mut app);
    {
        let mut ui = app.world_mut().resource_mut::<CaptureLogUiState>();
        ui.is_open = true;
        ui.active_filters.remove(&LogLevel::Debug);
    }

    go_home(&mut app);
    enter_sim(&mut app);

    let ui = app.world().resource::<CaptureLogUiState>();
    assert!(!ui.is_open, "the panel starts collapsed");
    assert!(
        !ui.active_filters.contains(&LogLevel::Debug),
        "filters kept"
    );
    assert_eq!(ui.active_filters.len(), 3);
}
