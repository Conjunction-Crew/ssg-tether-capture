//! Tests for `apply_sim_commands`, the system that turns the UI's
//! `SimCommand::StartCapture` into a `CaptureComponent` (P5 S4).
//!
//! The characterization tests cover the radius reset of a successful start. These
//! cover the rest of the command: what a start sets up, and what each refusal logs
//! and leaves behind.

use bevy::ecs::message::Messages;
use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;

use crate::capture::plan::{CapturePlan, CapturePlanLibrary};
use crate::capture::{
    CaptureComponent, CaptureSphereRadius, INITIAL_CAPTURE_SPHERE_RADIUS_M, SimCommand,
    apply_sim_commands,
};
use crate::telemetry::{DataCollection, LogLevel};
use crate::test_support::{LogTap, assert_close, fixed_epoch, insert_plan_json, test_app};
use crate::time::WorldTime;

const GOOD_PLAN: &str = r#"{"name": "good", "tether": "T", "states": [
  {"id": "first", "parameters": {"max_velocity": 1.0, "max_force": 1.0}},
  {"id": "second", "parameters": {"max_velocity": 1.0, "max_force": 1.0}}
]}"#;

fn commands_app() -> App {
    let mut app = test_app();
    insert_plan_json(&mut app, "good", GOOD_PLAN);
    app
}

fn start(target: Option<Entity>, plan_id: &str) -> SimCommand {
    SimCommand::StartCapture {
        target,
        plan_id: plan_id.to_string(),
    }
}

/// Sends `commands`, runs `apply_sim_commands` once and returns the log events it
/// wrote as `(level, source, message)`.
fn apply(app: &mut App, commands: Vec<SimCommand>) -> Vec<(LogLevel, &'static str, String)> {
    let mut tap = LogTap::new(app.world());
    for command in commands {
        app.world_mut().write_message(command);
    }
    app.world_mut()
        .run_system_once(apply_sim_commands)
        .expect("apply_sim_commands failed to run");
    // A new one-shot system reads the whole buffer; no `app.update()` swaps it.
    app.world_mut()
        .resource_mut::<Messages<SimCommand>>()
        .clear();
    tap.drain(app.world())
}

fn capture_of(app: &App, entity: Entity) -> Option<&CaptureComponent> {
    app.world().get::<CaptureComponent>(entity)
}

fn marked_entities(app: &mut App) -> Vec<Entity> {
    app.world_mut()
        .query_filtered::<Entity, With<CaptureComponent>>()
        .iter(app.world())
        .collect()
}

#[test]
fn start_capture_marks_the_target_and_starts_data_collection() {
    let mut app = commands_app();
    app.world_mut()
        .insert_resource(WorldTime::starting_at(fixed_epoch()));
    app.world_mut().resource_mut::<WorldTime>().epoch = fixed_epoch() + 7.0;
    app.world_mut()
        .insert_resource(CaptureSphereRadius { radius: 3.0 });
    let target = app.world_mut().spawn_empty().id();

    let logs = apply(&mut app, vec![start(Some(target), "good")]);

    assert_eq!(
        logs,
        vec![(
            LogLevel::Info,
            "ui",
            format!("Capture initiated for entity {target:?} with plan 'good'")
        )]
    );
    let component = capture_of(&app, target).expect("target is marked");
    assert_eq!(component.plan_id, "good");
    assert_eq!(
        component.current_state, "first",
        "a capture starts in the plan's first state"
    );
    // F-38: the entry time is sim seconds since the start epoch (7 s here).
    assert_close(
        component.state_enter_time_s,
        7.0,
        1e-9,
        "state_enter_time_s",
    );
    assert_eq!(component.state_elapsed_time_s, 0.0);
    assert_eq!(
        app.world().resource::<CaptureSphereRadius>().radius,
        INITIAL_CAPTURE_SPHERE_RADIUS_M
    );
    let data = app.world().resource::<DataCollection>();
    assert_eq!(data.position.get(&target), Some(&Vec::new()));
    assert_eq!(data.velocity.get(&target), Some(&Vec::new()));
}

#[test]
fn start_capture_without_a_sim_clock_enters_the_first_state_at_zero() {
    // Outside the sim screen there is no `WorldTime` (and no sphere radius).
    let mut app = commands_app();
    app.world_mut().remove_resource::<WorldTime>();
    app.world_mut().remove_resource::<CaptureSphereRadius>();
    let target = app.world_mut().spawn_empty().id();

    let logs = apply(&mut app, vec![start(Some(target), "good")]);

    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].0, LogLevel::Info);
    let component = capture_of(&app, target).expect("target is marked");
    assert_eq!(component.state_enter_time_s, 0.0);
    assert!(
        app.world().get_resource::<CaptureSphereRadius>().is_none(),
        "the command does not create the radius"
    );
}

#[test]
fn start_capture_moves_the_mark_from_the_previous_target() {
    let mut app = commands_app();
    let first = app.world_mut().spawn_empty().id();
    let second = app.world_mut().spawn_empty().id();
    apply(&mut app, vec![start(Some(first), "good")]);
    assert_eq!(marked_entities(&mut app), vec![first]);

    apply(&mut app, vec![start(Some(second), "good")]);

    // At most one entity carries the component.
    assert_eq!(marked_entities(&mut app), vec![second]);
}

#[test]
fn start_capture_with_an_unknown_plan_logs_an_error() {
    let mut app = commands_app();
    let target = app.world_mut().spawn_empty().id();
    let previous = app.world_mut().spawn_empty().id();
    apply(&mut app, vec![start(Some(previous), "good")]);

    let logs = apply(&mut app, vec![start(Some(target), "ghost")]);

    assert_eq!(
        logs,
        vec![(
            LogLevel::Error,
            "ui",
            "Capture plan 'ghost' not found".to_string()
        )]
    );
    assert!(capture_of(&app, target).is_none());
    assert!(
        !app.world()
            .resource::<DataCollection>()
            .position
            .contains_key(&target),
        "no data collection for a refused start"
    );
    // Proposed finding (P5): the previous capture is removed before the plan is
    // looked up, so a start that fails (here: an unknown plan id) also ends the
    // capture in progress. Pinned as today's behaviour.
    assert!(capture_of(&app, previous).is_none());
}

#[test]
fn start_capture_with_a_plan_without_states_logs_an_error() {
    let mut app = commands_app();
    // A plan the loader would have rejected; it is in the library all the same.
    let empty = CapturePlan {
        schema_version: 1,
        name: "empty".to_string(),
        id: String::new(),
        tether: "T".to_string(),
        device: None,
        capture_sphere: None,
        phases: Vec::new(),
        migrated_from_v0: None,
    };
    app.world_mut()
        .resource_mut::<CapturePlanLibrary>()
        .insert_plan("empty".to_string(), empty);
    app.world_mut()
        .insert_resource(CaptureSphereRadius { radius: 3.0 });
    let target = app.world_mut().spawn_empty().id();

    let logs = apply(&mut app, vec![start(Some(target), "empty")]);

    assert_eq!(
        logs,
        vec![(
            LogLevel::Error,
            "ui",
            format!("Failed to initiate capture for entity {target:?} with plan 'empty'")
        )]
    );
    assert!(capture_of(&app, target).is_none());
    assert!(
        !app.world()
            .resource::<DataCollection>()
            .velocity
            .contains_key(&target),
        "no data collection for a refused start"
    );
    assert_eq!(
        app.world().resource::<CaptureSphereRadius>().radius,
        3.0,
        "the radius is reset only for a start that succeeds"
    );
}

#[test]
fn start_capture_on_an_already_marked_entity_is_ignored() {
    let mut app = commands_app();
    let target = app.world_mut().spawn_empty().id();
    apply(&mut app, vec![start(Some(target), "good")]);
    // Progress the capture, shrink the sphere, and give it data.
    {
        let mut component = app
            .world_mut()
            .get_mut::<CaptureComponent>(target)
            .expect("marked");
        component.current_state = "second".to_string();
        component.state_elapsed_time_s = 3.0;
    }
    app.world_mut()
        .insert_resource(CaptureSphereRadius { radius: 7.0 });
    app.world_mut()
        .resource_mut::<DataCollection>()
        .position
        .get_mut(&target)
        .expect("data collection started")
        .push((1.0, 2.0));

    let logs = apply(&mut app, vec![start(Some(target), "good")]);

    assert_eq!(
        logs,
        vec![(
            LogLevel::Warn,
            "ui",
            format!("Entity {target:?} already marked for capture — ignoring")
        )]
    );
    let component = capture_of(&app, target).expect("still marked");
    assert_eq!(component.current_state, "second");
    assert_eq!(component.state_elapsed_time_s, 3.0);
    assert_eq!(app.world().resource::<CaptureSphereRadius>().radius, 7.0);
    assert_eq!(
        app.world().resource::<DataCollection>().position[&target],
        vec![(1.0, 2.0)],
        "the collected data is kept"
    );
}

#[test]
fn start_capture_without_a_target_warns_and_changes_nothing() {
    let mut app = commands_app();
    let marked = app.world_mut().spawn_empty().id();
    apply(&mut app, vec![start(Some(marked), "good")]);

    let logs = apply(&mut app, vec![start(None, "good")]);

    assert_eq!(
        logs,
        vec![(
            LogLevel::Warn,
            "ui",
            "Capture initiated with no target entity for plan 'good'".to_string()
        )]
    );
    assert_eq!(marked_entities(&mut app), vec![marked], "capture goes on");
}

#[test]
fn start_capture_handles_each_queued_command_in_order() {
    let mut app = commands_app();
    let target = app.world_mut().spawn_empty().id();

    // A refused command does not stop the ones queued after it.
    let logs = apply(
        &mut app,
        vec![start(Some(target), "ghost"), start(Some(target), "good")],
    );

    assert_eq!(
        logs,
        vec![
            (
                LogLevel::Error,
                "ui",
                "Capture plan 'ghost' not found".to_string()
            ),
            (
                LogLevel::Info,
                "ui",
                format!("Capture initiated for entity {target:?} with plan 'good'")
            ),
        ]
    );
    assert_eq!(marked_entities(&mut app), vec![target]);
}
