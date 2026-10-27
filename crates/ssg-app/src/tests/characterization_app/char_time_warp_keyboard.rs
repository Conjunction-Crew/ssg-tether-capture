//! Characterization of the keyboard time-warp ladder (F-12). Moved from
//! `characterization/char_time_warp.rs` in P3: `change_time_warp` is a UI system.

use crate::sim::telemetry::LogLevel;
use crate::sim::time::WorldTime;
use crate::tests::app_support::{LogTap, enter_sim, start_running, test_app};
use crate::ui::input::change_time_warp;
use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;

/// A harness app in `SimState::Running`.
fn running_app() -> App {
    let mut app = test_app();
    enter_sim(&mut app);
    start_running(&mut app);
    app
}

/// Press `key` and run `change_time_warp` once; returns the multiplier afterwards
/// and the Info messages it logged.
fn press(app: &mut App, key: KeyCode) -> (u32, Vec<String>) {
    let mut tap = LogTap::new(app.world());
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(key);
    app.world_mut()
        .run_system_once(change_time_warp)
        .expect("change_time_warp failed to run");
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .reset_all();
    let messages = tap
        .drain(app.world())
        .into_iter()
        .filter(|(level, source, _)| *level == LogLevel::Info && *source == "ui")
        .map(|(_, _, message)| message)
        .collect();
    (app.world().resource::<WorldTime>().multiplier, messages)
}

#[test]
fn char_warp_keyboard_ladder() {
    let mut app = running_app();
    assert_eq!(app.world().resource::<WorldTime>().multiplier, 1);

    // Period doubles from 1 up to 8192, and then stays: 8192 * 2 > 10000
    // (`user_input.rs:56`, `:70`).
    let mut up = Vec::new();
    for _ in 0..13 {
        let (multiplier, messages) = press(&mut app, KeyCode::Period);
        assert_eq!(messages, vec![format!("Time warp → {multiplier}x")]);
        up.push(multiplier);
    }
    // F-12: current behaviour, expected to change in P4
    assert_eq!(
        up,
        [2, 4, 8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096, 8192]
    );
    let (multiplier, messages) = press(&mut app, KeyCode::Period);
    // F-12: current behaviour, expected to change in P4
    assert_eq!(multiplier, 8192);
    assert!(messages.is_empty());

    // Comma halves down to 1 and then 0.
    let mut down = Vec::new();
    for _ in 0..14 {
        let (multiplier, messages) = press(&mut app, KeyCode::Comma);
        assert_eq!(messages, vec![format!("Time warp → {multiplier}x")]);
        down.push(multiplier);
    }
    // F-12: current behaviour, expected to change in P4
    assert_eq!(
        down,
        [4096, 2048, 1024, 512, 256, 128, 64, 32, 16, 8, 4, 2, 1, 0]
    );

    // Period from 0 goes to 1.
    let (multiplier, messages) = press(&mut app, KeyCode::Period);
    // F-12: current behaviour, expected to change in P4
    assert_eq!(multiplier, 1);
    assert_eq!(messages, vec!["Time warp → 1x".to_string()]);
}
