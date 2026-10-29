//! Pause, resume and time warp on the sim side (P5 S7): none of them changes the
//! trajectory, only how many fixed steps one `app.update()` runs.

use bevy::prelude::*;
use ssg_sim::session::PauseState;
use ssg_sim::test_support::{q13, test_app, tick, ticks};
use ssg_sim::time::WorldTime;

/// `(step, state, range_m bit pattern)`: equal tuples are the same trajectory
/// point, to the bit.
fn point(app: &App, handles: &q13::Q13Handles) -> (u64, String, u64) {
    let sample = q13::sample(app, handles);
    (sample.step, sample.state, sample.range_m.to_bits())
}

fn set_pause(app: &mut App, state: PauseState) {
    app.world_mut()
        .resource_mut::<NextState<PauseState>>()
        .set(state);
}

#[test]
fn warp_16_reaches_the_same_state_as_16_times_as_many_ticks_at_1x() {
    let mut at_1x = test_app();
    let handles_1x = q13::build(&mut at_1x);
    let mut at_16x = test_app();
    let handles_16x = q13::build(&mut at_16x);
    at_16x.world_mut().resource_mut::<WorldTime>().multiplier = 16;

    ticks(&mut at_1x, 16 * 40);
    ticks(&mut at_16x, 40);

    let (step, state, range_bits) = point(&at_1x, &handles_1x);
    assert_eq!(step, 643, "3 set-up steps + 640 ticks");
    assert_eq!(point(&at_16x, &handles_16x), (step, state, range_bits));
    assert_eq!(
        at_16x.world().resource::<WorldTime>().effective_multiplier,
        16
    );
}

#[test]
fn pausing_freezes_the_clock_and_resuming_continues_the_same_trajectory() {
    let mut uninterrupted = test_app();
    let handles_u = q13::build(&mut uninterrupted);
    ticks(&mut uninterrupted, 100 + 60);

    let mut paused = test_app();
    let handles_p = q13::build(&mut paused);
    ticks(&mut paused, 100);
    let before = point(&paused, &handles_p);

    set_pause(&mut paused, PauseState::Paused);
    ticks(&mut paused, 25);
    assert_eq!(
        point(&paused, &handles_p),
        before,
        "25 updates while paused change nothing"
    );

    set_pause(&mut paused, PauseState::Running);
    ticks(&mut paused, 60);

    assert_eq!(
        point(&paused, &handles_p),
        point(&uninterrupted, &handles_u),
        "after resuming, 60 ticks reach the point 160 ticks reach without a pause"
    );
}

#[test]
fn a_paused_clock_ignores_the_warp_multiplier() {
    let mut app = test_app();
    let handles = q13::build(&mut app);
    app.world_mut().resource_mut::<WorldTime>().multiplier = 16;
    set_pause(&mut app, PauseState::Paused);
    tick(&mut app);
    let before = point(&app, &handles);

    ticks(&mut app, 10);

    assert_eq!(point(&app, &handles), before);
}
