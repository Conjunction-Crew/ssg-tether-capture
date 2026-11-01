//! The recorder is a read-only tap (P7 S2, A-3, R5, C8, C11): a Q13 run with
//! recordings is bit-identical, step by step, to the run without; the frames
//! carry the floating-origin shift of the resets since the previous frame.

use bevy::ecs::message::Messages;
use bevy::prelude::*;

use crate::capture::{CaptureTransition, capture_state_machine_update};
use crate::floating_origin::OriginReset;
use crate::recording::recorder::record_step;
use crate::recording::recorder::{RecorderCommand, RecorderEvent, RecordingSettings};
use crate::recording::{EndReason, MemorySegment, StartTrigger};
use crate::test_support::{
    current_step, insert_plan_json, install_memory_sink, load_example_plans, q13, state_hash,
    test_app, tick,
};
use crate::time::{ManualPhysics, PHYS_DT, WorldTime};

/// The outcome of [`run_q13`].
pub struct TapRun {
    /// `(step, state_hash)` after every tick.
    pub hashes: Vec<(u64, u64)>,
    pub segments: Vec<MemorySegment>,
    pub events: Vec<RecorderEvent>,
    pub resets: Vec<OriginReset>,
    /// `(step, from, to)` of every capture transition the sim wrote.
    pub transitions: Vec<(u64, String, String)>,
}

/// The Q13 golden steps hold on macOS/aarch64 only; other platforms reach them a
/// few steps apart (F-17, F-48; `baseline_hubble`). Elsewhere the tests compare
/// the recording with what the same run's sim wrote.
pub const MACOS_GOLDENS: bool = cfg!(all(target_os = "macos", target_arch = "aarch64"));

/// The Q13 scenario (capture started by command) for `ticks` ticks after
/// [`q13::build_with_command`], with `settings`; each `(step, command)` is sent
/// so that it is applied after that step.
pub fn run_q13(
    ticks: u64,
    settings: RecordingSettings,
    commands: &[(u64, RecorderCommand)],
) -> TapRun {
    run_plan(q13::PLAN_ID, None, ticks, settings, commands)
}

/// [`run_q13`] with the plan `plan_id`, inserted from `json` when given.
pub fn run_plan(
    plan_id: &str,
    json: Option<&str>,
    ticks: u64,
    settings: RecordingSettings,
    commands: &[(u64, RecorderCommand)],
) -> TapRun {
    let mut app = test_app();
    load_example_plans(&mut app);
    if let Some(json) = json {
        insert_plan_json(&mut app, plan_id, json);
    }
    app.insert_resource(settings);
    let sink = install_memory_sink(&mut app);
    let mut events_cursor = app
        .world()
        .resource::<Messages<RecorderEvent>>()
        .get_cursor_current();
    let mut resets_cursor = app
        .world()
        .resource::<Messages<OriginReset>>()
        .get_cursor_current();
    let mut transitions_cursor = app
        .world()
        .resource::<Messages<CaptureTransition>>()
        .get_cursor_current();
    q13::build_with_command(&mut app, plan_id);
    let mut run = TapRun {
        hashes: Vec::with_capacity(ticks as usize),
        segments: Vec::new(),
        events: Vec::new(),
        resets: Vec::new(),
        transitions: Vec::new(),
    };
    for _ in 0..ticks {
        let next = current_step(&app) + 1;
        for (_, command) in commands.iter().filter(|(step, _)| *step == next) {
            app.world_mut().write_message(command.clone());
        }
        tick(&mut app);
        run.hashes.push((current_step(&app), state_hash(&mut app)));
        let world = app.world();
        run.events.extend(
            events_cursor
                .read(world.resource::<Messages<RecorderEvent>>())
                .cloned(),
        );
        run.resets.extend(
            resets_cursor
                .read(world.resource::<Messages<OriginReset>>())
                .copied(),
        );
        let start = world.resource::<WorldTime>().start_epoch;
        run.transitions.extend(
            transitions_cursor
                .read(world.resource::<Messages<CaptureTransition>>())
                .map(|t| {
                    let step = ((t.epoch - start) / PHYS_DT).round() as u64;
                    (step, t.from.clone(), t.to.clone())
                }),
        );
    }
    run.segments = sink.segments();
    run
}

fn summary(segments: &[MemorySegment]) -> Vec<String> {
    segments
        .iter()
        .map(|s| {
            format!(
                "{:?} {}..{:?} frames {} transitions {:?} end {:?}",
                s.header.trigger,
                s.header.start_step,
                s.frames.last().map(|f| f.step),
                s.frames.len(),
                s.transitions
                    .iter()
                    .map(|t| (t.step, t.from.as_str(), t.to.as_str()))
                    .collect::<Vec<_>>(),
                s.end
            )
        })
        .collect()
}

#[track_caller]
fn assert_same_steps(off: &TapRun, on: &TapRun, what: &str) {
    assert_eq!(off.hashes.len(), on.hashes.len(), "{what}: steps compared");
    if let Some(i) = off.hashes.iter().zip(&on.hashes).position(|(a, b)| a != b) {
        panic!(
            "{what}: the state differs at step {} (off {:016x}, on {:016x})",
            off.hashes[i].0, off.hashes[i].1, on.hashes[i].1
        );
    }
    let (first, last) = (on.hashes[0], on.hashes[on.hashes.len() - 1]);
    println!(
        "{what}: {} steps compared ({}..={}), identical; first hash {:016x}, last hash {:016x}",
        on.hashes.len(),
        first.0,
        last.0,
        first.1,
        last.1
    );
}

#[test]
fn a_recording_does_not_change_the_run() {
    const TICKS: u64 = 6_400;
    let k16 = || RecordingSettings {
        keyframe_interval_steps: 16,
        ..RecordingSettings::default()
    };
    // The three runs are independent: run them side by side.
    let (off, single, multi) = std::thread::scope(|scope| {
        let off = scope.spawn(|| run_q13(TICKS, RecordingSettings::default(), &[]));
        let single = scope.spawn(|| {
            run_q13(
                TICKS,
                k16(),
                &[(
                    640,
                    RecorderCommand::Start {
                        name: "single".into(),
                    },
                )],
            )
        });
        let multi = scope.spawn(|| {
            let settings = RecordingSettings {
                auto_start_phase: Some("approach".into()),
                ..k16()
            };
            run_q13(
                TICKS,
                settings,
                &[
                    (640, RecorderCommand::Start { name: "a".into() }),
                    (2_000, RecorderCommand::Stop),
                    (2_500, RecorderCommand::Start { name: "b".into() }),
                    (3_500, RecorderCommand::Stop),
                ],
            )
        });
        (
            off.join().expect("off run"),
            single.join().expect("single run"),
            multi.join().expect("multi run"),
        )
    });

    // The off run really records nothing.
    assert!(off.segments.is_empty(), "the recorder-off run recorded");
    assert!(
        off.events.is_empty(),
        "the recorder-off run has recorder events"
    );

    println!("single: {:#?}", summary(&single.segments));
    println!("multi: {:#?}", summary(&multi.segments));
    assert_same_steps(&off, &single, "single start at 640");
    assert_same_steps(
        &off,
        &multi,
        "start 640, stop 2000, start 2500, stop 3500, auto start",
    );

    // What was recorded.
    let single_steps: Vec<u64> = single.segments[0].frames.iter().map(|f| f.step).collect();
    assert_eq!(single.segments.len(), 1);
    assert_eq!(single_steps.first(), Some(&640));
    assert!(single_steps.windows(2).all(|w| w[1] - w[0] == 16));
    assert_eq!(single.segments[0].end, None, "still recording at the end");

    // The single recording (640 to the end) holds the one transition the sim wrote
    // inside it (4,695 on macOS, F-17), and the frames' phase follows it.
    let approach = single.transitions[0].0;
    assert_eq!(single.transitions.len(), 1);
    if MACOS_GOLDENS {
        assert_eq!(approach, 4_703); // F-66 (P7.5 S3): provisional until S4
    }
    let recorded: Vec<(u64, &str, &str)> = single.segments[0]
        .transitions
        .iter()
        .map(|t| (t.step, t.from.as_str(), t.to.as_str()))
        .collect();
    println!("single: transitions {recorded:?}");
    assert_eq!(recorded, vec![(approach, "rendezvous", "approach")]);
    assert!(
        single.segments[0].transitions[0]
            .reason
            .starts_with("distance")
    );
    for frame in &single.segments[0].frames {
        let expected = if frame.step < approach {
            "rendezvous"
        } else {
            "approach"
        };
        assert_eq!(
            frame.phase.as_deref(),
            Some(expected),
            "frame {}",
            frame.step
        );
    }

    // The automatic start is the step the sim entered `approach` (4,695 on macOS).
    let approach = multi
        .transitions
        .iter()
        .find(|(_, _, to)| to == "approach")
        .expect("the run enters approach")
        .0;
    println!("approach entered at step {approach}");
    if MACOS_GOLDENS {
        assert_eq!(approach, 4_703); // F-66 (P7.5 S3): provisional until S4
    }
    let starts: Vec<(u64, StartTrigger)> = multi
        .segments
        .iter()
        .map(|s| (s.header.start_step, s.header.trigger))
        .collect();
    assert_eq!(
        starts,
        vec![
            (640, StartTrigger::Manual),
            (2_500, StartTrigger::Manual),
            (approach, StartTrigger::Phase),
        ]
    );
    let ends: Vec<Option<(u64, EndReason)>> = multi
        .segments
        .iter()
        .map(|s| s.end.map(|e| (e.end_step, e.reason)))
        .collect();
    assert_eq!(
        ends,
        vec![
            Some((2_000, EndReason::Manual)),
            Some((3_500, EndReason::Manual)),
            None,
        ]
    );
    for segment in &multi.segments {
        let last = segment.frames.last().expect("frames").step;
        assert_eq!(
            segment.end.map_or(last, |e| e.end_step),
            last,
            "the last frame is the end step"
        );
    }
}

#[test]
fn a_frame_carries_the_origin_shift_since_the_previous_frame() {
    let settings = RecordingSettings {
        keyframe_interval_steps: 16,
        ..RecordingSettings::default()
    };
    // 1,400 ticks after build: steps 4 ..= 1,403.
    let run = run_q13(
        1_400, // F-66 (P7.5 S1): provisional until S4
        settings,
        &[(400, RecorderCommand::Start { name: "o".into() })],
    );
    let first = run
        .resets
        .iter()
        .find(|r| r.step >= 400)
        .expect("a reset after step 400");
    let second = run
        .resets
        .iter()
        .find(|r| r.step > first.step)
        .expect("a second reset");
    println!(
        "resets: {:?}",
        run.resets.iter().map(|r| r.step).collect::<Vec<_>>()
    );
    if MACOS_GOLDENS {
        assert_eq!((first.step, second.step), (703, 1_314), "the Q13 resets"); // F-66 (P7.5 S3): provisional until S4
    }
    let frames = &run.segments[0].frames;
    let shifted = frames
        .iter()
        .find(|f| f.step > first.step)
        .expect("a frame after the reset");
    println!(
        "step-{} reset com_r {:?} com_v {:?}; frame {} origin_shift_m {:?} origin_shift_mps {:?}",
        first.step,
        first.com_r,
        first.com_v,
        shifted.step,
        shifted.origin_shift_m,
        shifted.origin_shift_mps
    );
    // The first frame after the reset (frames at 400 + 16 n).
    assert_eq!(shifted.step, 400 + (first.step - 400) / 16 * 16 + 16);
    assert_eq!(shifted.origin_shift_m.to_array(), first.com_r.to_array());
    assert_eq!(shifted.origin_shift_mps.to_array(), first.com_v.to_array());
    for frame in frames
        .iter()
        .filter(|f| f.step != shifted.step && f.step < second.step)
    {
        assert_eq!(
            (frame.origin_shift_m, frame.origin_shift_mps),
            (Vec3::ZERO.as_dvec3(), Vec3::ZERO.as_dvec3()),
            "frame {} has no reset since the previous frame",
            frame.step
        );
    }
    // Every frame carries the frame origin's ECI state.
    assert!(frames.iter().all(|f| f.frame_origin_eci.is_some()));
}

#[test]
fn the_tap_runs_after_the_controller_and_its_finite_state_check() {
    let mut app = test_app();
    load_example_plans(&mut app);
    q13::build_with_command(&mut app, q13::PLAN_ID);
    let schedules = app.world().resource::<Schedules>();
    let schedule = schedules.get(ManualPhysics).expect("ManualPhysics");
    // Executable order (topological); systems matched by type (names need
    // Bevy's `debug` feature).
    let order: Vec<std::any::TypeId> = schedule
        .systems()
        .expect("ManualPhysics has run")
        .map(|(_, system)| system.type_id())
        .collect();
    fn type_of<M>(system: impl IntoSystem<(), (), M>) -> std::any::TypeId {
        IntoSystem::into_system(system).type_id()
    }
    let at = |id: std::any::TypeId, name: &str| {
        order
            .iter()
            .position(|t| *t == id)
            .unwrap_or_else(|| panic!("{name} is not in ManualPhysics"))
    };
    let controller = at(
        type_of(capture_state_machine_update),
        "capture_state_machine_update",
    );
    let tap = at(type_of(record_step), "record_step");
    assert!(controller < tap, "the tap runs after the controller");
    #[cfg(debug_assertions)]
    {
        let finite = at(
            type_of(crate::capture::assert_finite_state),
            "assert_finite_state",
        );
        assert!(
            controller < finite && finite < tap,
            "assert_finite_state runs after the controller and before the tap"
        );
    }
}
