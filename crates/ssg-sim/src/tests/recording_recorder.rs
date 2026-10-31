//! The recorder state machine (P7 S2, R4): manual start and stop, automatic
//! start and stops, caps, refusals and the body table.

use avian3d::prelude::DistanceJoint;
use bevy::ecs::message::Messages;
use bevy::prelude::*;

use crate::orbit::cache::OrbitalCache;
use crate::project::recording::RecordingBodyRole;
use crate::recording::recorder::{Recorder, RecorderCommand, RecorderEvent, RecordingSettings};
use crate::recording::{EndReason, RecordingBodies, RecordingEnd, StartTrigger};
use crate::session::UiScreen;
use crate::test_support::{
    insert_plan_json, install_memory_sink, load_example_plans, q13, test_app, tick,
};
use crate::tests::recording_end_event::SHORT_PLAN;
use crate::tests::recording_tap::{TapRun, run_plan, run_q13};
use crate::tether::TetherNode;

fn k32() -> RecordingSettings {
    RecordingSettings::default()
}

fn frame_steps(run: &TapRun, segment: usize) -> Vec<u64> {
    run.segments[segment]
        .frames
        .iter()
        .map(|f| f.step)
        .collect()
}

fn end(run: &TapRun, segment: usize) -> Option<(u64, EndReason)> {
    run.segments[segment].end.map(|e| (e.end_step, e.reason))
}

fn warnings(run: &TapRun) -> Vec<String> {
    run.events
        .iter()
        .filter_map(|e| match e {
            RecorderEvent::Warning(w) => Some(w.clone()),
            _ => None,
        })
        .collect()
}

fn refusals(run: &TapRun) -> Vec<String> {
    run.events
        .iter()
        .filter_map(|e| match e {
            RecorderEvent::Refused(w) => Some(w.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn manual_start_and_stop_give_frames_at_start_every_k_and_stop() {
    let run = run_q13(
        250,
        k32(),
        &[
            (100, RecorderCommand::Start { name: "m".into() }),
            (200, RecorderCommand::Stop),
        ],
    );
    assert_eq!(frame_steps(&run, 0), vec![100, 132, 164, 196, 200]);
    assert_eq!(end(&run, 0), Some((200, EndReason::Manual)));
    let header = &run.segments[0].header;
    assert_eq!(
        (
            header.name.as_deref(),
            header.trigger,
            header.keyframe_interval_steps
        ),
        (Some("m"), StartTrigger::Manual, 32)
    );
    assert_eq!(
        run.events.first(),
        Some(&RecorderEvent::Started {
            step: 100,
            trigger: StartTrigger::Manual
        })
    );
    assert_eq!(
        run.events.last(),
        Some(&RecorderEvent::Stopped {
            end: RecordingEnd {
                end_step: 200,
                reason: EndReason::Manual
            }
        })
    );
}

#[test]
fn a_second_start_while_recording_is_refused() {
    let run = run_q13(
        60,
        k32(),
        &[
            (10, RecorderCommand::Start { name: "a".into() }),
            (20, RecorderCommand::Start { name: "b".into() }),
        ],
    );
    assert_eq!(run.segments.len(), 1);
    assert_eq!(
        refusals(&run),
        vec!["a recording is already running".to_string()]
    );
}

#[test]
fn the_recording_stops_after_the_end_event_and_its_delay() {
    // Automatic start at the plan's first phase (the capture start, after step 2);
    // the end phase is entered at step 3; 1 s = 64 steps later the recording stops.
    let settings = RecordingSettings {
        auto_start_phase: Some("go".into()),
        stop_after_end_s: 1.0,
        ..k32()
    };
    let run = run_plan("short", Some(SHORT_PLAN), 100, settings, &[]);
    println!(
        "short plan: frames {:?} end {:?}",
        frame_steps(&run, 0),
        end(&run, 0)
    );
    assert_eq!(run.segments.len(), 1);
    assert_eq!(
        (
            run.segments[0].header.start_step,
            run.segments[0].header.trigger
        ),
        (2, StartTrigger::Phase)
    );
    assert_eq!(frame_steps(&run, 0), vec![2, 34, 66, 67]);
    assert_eq!(end(&run, 0), Some((67, EndReason::EndAbort)));
    let transitions: Vec<(u64, &str)> = run.segments[0]
        .transitions
        .iter()
        .map(|t| (t.step, t.to.as_str()))
        .collect();
    assert_eq!(transitions, vec![(3, "stop")]);

    // Without `auto_stop_on_end` it keeps recording.
    let settings = RecordingSettings {
        auto_start_phase: Some("go".into()),
        stop_after_end_s: 1.0,
        auto_stop_on_end: false,
        ..k32()
    };
    let run = run_plan("short", Some(SHORT_PLAN), 100, settings, &[]);
    assert_eq!(end(&run, 0), None);
}

#[test]
fn the_duration_cap_stops_the_recording_and_warns_at_80_percent() {
    let settings = RecordingSettings {
        max_duration_s: 1.0,
        ..k32()
    };
    let run = run_q13(
        200,
        settings,
        &[(100, RecorderCommand::Start { name: "d".into() })],
    );
    assert_eq!(frame_steps(&run, 0), vec![100, 132, 164]);
    assert_eq!(end(&run, 0), Some((164, EndReason::MaxDuration)));
    // 80 % of 64 steps: 51.2, so the warning comes at step 152.
    assert_eq!(
        warnings(&run),
        vec!["Recording at 80 % of its duration cap (step 152)".to_string()]
    );
}

#[test]
fn the_size_cap_stops_before_the_frame_that_would_exceed_it() {
    // 193 bodies × 116 B = 22,388 B per frame: four frames fit in 0.1 MB.
    let settings = RecordingSettings {
        max_size_mb: 0.1,
        ..k32()
    };
    let run = run_q13(
        300,
        settings,
        &[(100, RecorderCommand::Start { name: "s".into() })],
    );
    assert_eq!(frame_steps(&run, 0), vec![100, 132, 164, 196]);
    assert_eq!(end(&run, 0), Some((196, EndReason::MaxSize)));
    assert_eq!(
        warnings(&run),
        vec!["Recording at 80 % of its size cap (89552 bytes)".to_string()]
    );
}

#[test]
fn more_tether_bodies_than_the_cap_are_refused() {
    let dense = r#"{"schema_version": 1, "name": "dense", "tether": "Tether1",
      "device": {"type": "tether", "tether_length_m": 20.0, "segment_spacing_m": 0.05},
      "phases": [{"id": "go", "max_velocity_mps": 1.0, "max_force_n": 1.0}]}"#;
    let mut app = test_app();
    load_example_plans(&mut app);
    insert_plan_json(&mut app, "dense", dense);
    let sink = install_memory_sink(&mut app);
    q13::build_with_command(&mut app, "dense");
    let bodies = app.world().resource::<RecordingBodies>().entities.len();
    let tether = app.world().resource::<RecordingBodies>().tether_bodies();
    let mut cursor = app
        .world()
        .resource::<Messages<RecorderEvent>>()
        .get_cursor_current();
    app.world_mut()
        .write_message(RecorderCommand::Start { name: "x".into() });
    tick(&mut app);
    let events: Vec<RecorderEvent> = cursor
        .read(app.world().resource::<Messages<RecorderEvent>>())
        .cloned()
        .collect();
    println!("dense plan: {bodies} bodies, {tether} tether bodies; {events:?}");
    // 383 bodies: the target and 382 tether bodies.
    assert_eq!((bodies, tether), (383, 382));
    assert_eq!(
        events,
        vec![RecorderEvent::Refused(
            "382 tether bodies; at most 200 may be recorded".to_string()
        )]
    );
    assert!(sink.segments().is_empty());
    assert!(!app.world().resource::<Recorder>().is_recording());
}

#[test]
fn the_body_table_is_target_root_nodes_tail() {
    let mut app = test_app();
    load_example_plans(&mut app);
    let handles = q13::build_with_command(&mut app, q13::PLAN_ID);
    let bodies = app.world().resource::<RecordingBodies>().clone();
    let tether = app.world().resource::<OrbitalCache>().tethers["Tether1"].clone();
    assert_eq!(bodies.entities.len(), 193);
    assert_eq!(bodies.entities[0], handles.debris);
    assert_eq!(bodies.entities[1], handles.root);
    // `OrbitalCache.tethers` holds the root and the tail only.
    assert_eq!(tether, vec![bodies.entities[1], bodies.entities[192]]);
    // Consecutive bodies are joined by the spawn's distance joints, in order.
    let joints: Vec<(Entity, Entity)> = app
        .world_mut()
        .query::<&DistanceJoint>()
        .iter(app.world())
        .map(|j| (j.body1, j.body2))
        .collect();
    for pair in bodies.entities[1..].windows(2) {
        assert!(joints.contains(&(pair[0], pair[1])), "{pair:?} not joined");
    }
    for &node in &bodies.entities[2..] {
        assert!(app.world().get::<TetherNode>(node).is_some());
    }
    assert_eq!(bodies.roles[0], RecordingBodyRole::Target);
    assert_eq!(bodies.roles[1], RecordingBodyRole::TetherRoot);
    assert!(
        bodies.roles[2..192]
            .iter()
            .all(|r| *r == RecordingBodyRole::TetherNode)
    );
    assert_eq!(bodies.roles[192], RecordingBodyRole::TetherTail);
}

#[test]
fn leaving_the_sim_ends_the_recording() {
    let mut app = test_app();
    load_example_plans(&mut app);
    let sink = install_memory_sink(&mut app);
    q13::build_with_command(&mut app, q13::PLAN_ID);
    app.world_mut()
        .write_message(RecorderCommand::Start { name: "e".into() });
    for _ in 0..40 {
        tick(&mut app);
    }
    app.world_mut()
        .resource_mut::<NextState<UiScreen>>()
        .set(UiScreen::Home);
    app.update();
    let segments = sink.segments();
    let last = segments[0].frames.last().expect("frames").step;
    println!(
        "session end: frames {:?} end {:?}",
        segments[0]
            .frames
            .iter()
            .map(|f| f.step)
            .collect::<Vec<_>>(),
        segments[0].end
    );
    assert_eq!(segments[0].header.start_step, 4);
    assert_eq!(
        segments[0].end,
        Some(RecordingEnd {
            end_step: last,
            reason: EndReason::SessionEnd
        })
    );
    assert!(!app.world().resource::<Recorder>().is_recording());
    assert!(app.world().get_resource::<RecordingBodies>().is_none());
}
