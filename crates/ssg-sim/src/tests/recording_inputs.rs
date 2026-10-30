//! The session input log (P7 S2, R5, C1): `SessionStart` with the spawn-update
//! step count k first, then each applied capture start with its step.

use bevy::prelude::*;

use crate::capture::SimCommand;
use crate::orbit::selection::OrbitalSelectionState;
use crate::project::SelectedProject;
use crate::project::recording::RecordingBodyRole;
use crate::recording::inputs::{RecordedCommand, RecordedInput, SimInputLog};
use crate::session::{PauseState, SimState};
use crate::test_support::{current_step, enter_sim, load_example_plans, q13, test_app, tick};
use crate::time::WorldTime;

fn log(app: &App) -> Vec<RecordedInput> {
    app.world().resource::<SimInputLog>().entries.clone()
}

fn session_start(k: u32) -> RecordedInput {
    RecordedInput {
        after_step: u64::from(k),
        command: RecordedCommand::SessionStart {
            steps_before_propagators: k,
        },
    }
}

#[test]
fn harness_session_start_is_k_1_then_the_capture_start() {
    let mut app = test_app();
    load_example_plans(&mut app);
    let handles = q13::build_with_command(&mut app, q13::PLAN_ID);

    assert_eq!(
        log(&app),
        vec![
            session_start(1),
            RecordedInput {
                after_step: 2,
                command: RecordedCommand::StartCapture {
                    target: RecordingBodyRole::Target,
                    plan_id: q13::PLAN_ID.to_string(),
                },
            },
        ]
    );

    // A second start on the target already being captured is refused: not an input.
    app.world_mut().write_message(SimCommand::StartCapture {
        target: Some(handles.debris),
        plan_id: q13::PLAN_ID.to_string(),
    });
    tick(&mut app);
    assert_eq!(log(&app).len(), 2, "a refused command is not an input");
}

/// The spawn update runs k fixed steps (k = 0: paused), as
/// `plans/stage-7/results/review-spawn_k_probe.rs` does.
fn session_start_with_spawn_steps(k: u32) -> Vec<RecordedInput> {
    let mut app = test_app();
    load_example_plans(&mut app);
    app.world_mut().resource_mut::<SelectedProject>().project_id = Some(q13::PLAN_ID.to_string());
    {
        let mut selection = app.world_mut().resource_mut::<OrbitalSelectionState>();
        selection.target = Some(q13::catalog_pick("HST", q13::HST_NORAD_ID));
        selection.chaser = Some(q13::catalog_pick("P0 HST CHASER", q13::CHASER_NORAD_ID));
    }
    enter_sim(&mut app);
    if k == 0 {
        app.world_mut()
            .resource_mut::<NextState<PauseState>>()
            .set(PauseState::Paused);
    } else {
        app.world_mut().resource_mut::<WorldTime>().multiplier = k;
    }
    app.world_mut()
        .resource_mut::<NextState<SimState>>()
        .set(SimState::Running);
    app.update();
    assert_eq!(
        current_step(&app),
        u64::from(k),
        "the spawn update ran k steps"
    );
    assert!(log(&app).is_empty(), "no propagator yet: no SessionStart");
    if k == 0 {
        app.world_mut()
            .resource_mut::<NextState<PauseState>>()
            .set(PauseState::Running);
    } else {
        app.world_mut().resource_mut::<WorldTime>().multiplier = 1;
    }
    tick(&mut app);
    tick(&mut app);
    log(&app)
}

#[test]
fn session_start_records_the_spawn_update_step_count() {
    for k in [3, 0] {
        assert_eq!(
            session_start_with_spawn_steps(k),
            vec![session_start(k)],
            "spawn update with k = {k}"
        );
    }
}

#[test]
fn entering_the_sim_clears_the_log() {
    let mut app = test_app();
    app.world_mut()
        .resource_mut::<SimInputLog>()
        .push_session_start(5);
    enter_sim(&mut app);
    assert!(log(&app).is_empty());
}

#[test]
fn only_the_first_session_start_is_kept_and_other_targets_are_not_inputs() {
    let mut input_log = SimInputLog::default();
    input_log.push_session_start(1);
    input_log.push_session_start(4);
    // No capture target registered: a start cannot name a role.
    input_log.push_start_capture(9, Entity::PLACEHOLDER, "p");
    assert_eq!(input_log.entries, vec![session_start(1)]);
}
