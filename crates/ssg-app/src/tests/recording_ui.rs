//! The Recording window's state and the app's step cap (P7 S5): the panel logic
//! without rendering, and one run from the panel's clicks to a recording on disk
//! that the replay mode loads.

use std::time::Duration;

use avian3d::prelude::DistanceJoint;
use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;

use crate::render::camera::CameraTarget;
use crate::render::gizmos::{replay_tether_chain, running_tether_chains};
use crate::tests::app_support::{test_app, tick};
use crate::ui::recording::{
    MAX_STEP_CAP, MIN_STEP_CAP, PanelAction, RecordingPanel, adapt_step_cap, apply_panel_action,
    collect_recorder_events, install_recording_sink, next_step_cap, panel_status, projected_bytes,
    recording_name, save_recording_settings,
};
use crate::ui::replay::{
    ReplayBody, load_replay_session, replay_shapes, scan_recordings, spawn_replay_bodies,
};
use ssg_sim::orbit::cache::OrbitalCache;
use ssg_sim::orbit::selection::OrbitalSelectionState;
use ssg_sim::project::WorkingDirectory;
use ssg_sim::project::manifest::{ProjectManifest, read_manifest_file, save_manifest};
use ssg_sim::project::recording::{
    RecordingBodyRole, RecordingEndReason, RecordingSource, read_recording_file,
    save_recording_manifest,
};
use ssg_sim::recording::recorder::{Recorder, RecorderCommand, RecordingSettings};
use ssg_sim::recording::{RecordingBodies, RecordingSink};
use ssg_sim::session::PauseState;
use ssg_sim::test_support::{TempDir, fixed_epoch, q13};
use ssg_sim::tether::{DebrisBody, TetherShape};
use ssg_sim::time::{StepCap, WorldTime};

#[test]
fn step_cap_halves_after_slow_frames_and_doubles_after_fast_ones() {
    // Slow frames at the cap halve it, never below 8.
    let mut cap = 64;
    let mut seen = Vec::new();
    for _ in 0..4 {
        cap = next_step_cap(cap, 8192, cap, 0.060);
        seen.push(cap);
    }
    assert_eq!(seen, [32, 16, 8, MIN_STEP_CAP]);

    // Fast frames while the request exceeds the cap double it, never above 512.
    let mut seen = Vec::new();
    for _ in 0..6 {
        cap = next_step_cap(cap, 8192, cap, 0.010);
        seen.push(cap);
    }
    assert_eq!(seen, [16, 32, 64, 128, 256, MAX_STEP_CAP]);
    assert_eq!(
        next_step_cap(MAX_STEP_CAP, 8192, MAX_STEP_CAP, 0.010),
        MAX_STEP_CAP
    );
}

#[test]
fn step_cap_holds_when_nothing_calls_for_a_change() {
    // A slow frame that did not run at the cap says nothing about the cap.
    assert_eq!(next_step_cap(64, 8192, 10, 0.060), 64);
    // A fast frame with no request above the cap has nothing to raise it for.
    assert_eq!(next_step_cap(64, 64, 64, 0.010), 64);
    assert_eq!(next_step_cap(64, 1, 1, 0.010), 64);
    // Between 25 ms and 50 ms the cap stays.
    assert_eq!(next_step_cap(64, 8192, 64, 0.030), 64);
    assert_eq!(next_step_cap(64, 8192, 64, 0.050), 64);
}

#[test]
fn step_cap_changes_at_most_every_half_second() {
    let mut app = test_app();
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
        60,
    )))
    .insert_resource(WorldTime {
        multiplier: 8192,
        effective_multiplier: 64,
        ..WorldTime::starting_at(fixed_epoch())
    })
    .add_systems(Update, adapt_step_cap);
    app.update();
    let mut caps = Vec::new();
    for _ in 0..24 {
        // The warp stays pinned at the cap, as a machine too slow for it would.
        let cap = app.world().resource::<StepCap>().0;
        app.world_mut()
            .resource_mut::<WorldTime>()
            .effective_multiplier = cap;
        app.update();
        caps.push(app.world().resource::<StepCap>().0);
    }
    // 60 ms frames: the cap may change every ninth frame (0.54 s), no sooner.
    let changes: Vec<usize> = (1..caps.len())
        .filter(|&i| caps[i] != caps[i - 1])
        .collect();
    assert_eq!(caps[0], 64, "no change in the first frame: {caps:?}");
    assert!(
        changes.len() == 2 && changes[1] - changes[0] >= 8 && changes[0] >= 6,
        "two halvings, each after at least half a second: {changes:?} {caps:?}"
    );
    assert_eq!(*caps.last().unwrap(), 16, "{caps:?}");

    // While paused the cap does not move.
    app.world_mut()
        .resource_mut::<NextState<PauseState>>()
        .set(PauseState::Paused);
    app.update();
    let held = app.world().resource::<StepCap>().0;
    for _ in 0..20 {
        app.update();
    }
    assert_eq!(app.world().resource::<StepCap>().0, held);
}

#[test]
fn projected_size_follows_the_budget() {
    let defaults = RecordingSettings::default();
    // E2: K = 32 at 201 bodies is 84 MB for the 30-minute cap.
    let bytes = projected_bytes(&defaults, 201);
    assert_eq!(bytes, (1800 * 64 / 32 + 1) * 116 * 201);
    assert!((83.0e6..85.0e6).contains(&(bytes as f64)), "{bytes}");
    // K = 1 hits the 100 MB size cap long before the duration cap.
    let every_step = RecordingSettings {
        keyframe_interval_steps: 1,
        ..defaults.clone()
    };
    assert_eq!(projected_bytes(&every_step, 201), 100_000_000);
    assert_eq!(projected_bytes(&defaults, 0), 0);
}

#[test]
fn an_idle_recorder_shows_nothing_recorded() {
    let status = panel_status(
        &Recorder::default(),
        0,
        &RecordingSettings::default(),
        193,
        false,
    );
    assert!(!status.recording && !status.near_cap);
    assert_eq!((status.frames, status.bytes), (0, 0));
    assert_eq!(
        status.projected_bytes,
        projected_bytes(&RecordingSettings::default(), 193)
    );
}

#[test]
fn recording_names_are_the_plan_and_a_stamp() {
    assert_eq!(
        recording_name("example_plan", "20261030T120000Z"),
        "example_plan-20261030T120000Z"
    );
    // A plan id that is not a file name is sanitised.
    assert_eq!(recording_name("my plan/1", "S"), "my_plan_1-S");
}

#[test]
fn record_and_stop_become_recorder_commands() {
    let mut panel = RecordingPanel::default();
    let mut settings = RecordingSettings::default();
    let dir = TempDir::new("panel-commands");
    let record = apply_panel_action(
        &PanelAction::Record,
        &mut panel,
        &mut settings,
        Some("example_plan"),
        dir.path(),
        "STAMP",
    );
    assert_eq!(
        record,
        Some(RecorderCommand::Start {
            name: "example_plan-STAMP".to_string()
        })
    );
    let stop = apply_panel_action(
        &PanelAction::Stop,
        &mut panel,
        &mut settings,
        None,
        dir.path(),
        "S",
    );
    assert_eq!(stop, Some(RecorderCommand::Stop));
}

#[test]
fn saving_the_settings_needs_a_valid_project_file() {
    let dir = TempDir::new("panel-save");
    let draft = RecordingSettings {
        keyframe_interval_steps: 16,
        auto_start_phase: Some("capture".to_string()),
        ..RecordingSettings::default()
    };
    // No project.json: nothing is written.
    let err = save_recording_settings(dir.path(), &draft).expect_err("no manifest");
    assert!(err.contains("project.json"), "{err}");
    assert!(!dir.path().join("project.json").exists());
}

#[test]
fn recorder_messages_reach_the_window_and_the_last_six_are_kept() {
    let mut app = App::new();
    app.add_message::<ssg_sim::recording::recorder::RecorderEvent>()
        .init_resource::<RecordingPanel>()
        .add_systems(Update, collect_recorder_events);
    for i in 0..9 {
        app.world_mut()
            .write_message(ssg_sim::recording::recorder::RecorderEvent::Warning(
                format!("w{i}"),
            ));
        app.update();
    }
    let notes = &app.world().resource::<RecordingPanel>().notes;
    assert_eq!(notes.len(), 6);
    assert_eq!(notes.first().map(String::as_str), Some("Warning: w3"));
    assert_eq!(notes.last().map(String::as_str), Some("Warning: w8"));
}

/// The Q13 harness with a project folder: `project.json` is the scenario the app
/// writes at Start, and the working directory points at it.
fn q13_in_project(tag: &str) -> (App, TempDir) {
    let dir = TempDir::new(tag);
    let mut app = test_app();
    q13::build(&mut app);
    let manifest = ProjectManifest::from_selection(
        "q13",
        fixed_epoch(),
        Some(q13::PLAN_ID),
        app.world().resource::<OrbitalSelectionState>(),
    )
    .expect("both bodies are selected");
    save_manifest(dir.path(), &manifest).expect("project.json is written");
    let path = dir.path().to_string_lossy().into_owned();
    app.insert_resource(WorkingDirectory {
        path: path.clone(),
        pending_path: path,
    });
    (app, dir)
}

/// Record through the window's clicks, let the duration cap stop it, then open
/// the recording the way the Recordings window does.
#[test]
fn a_recording_made_from_the_window_plays_back() {
    let (mut app, dir) = q13_in_project("panel-to-playback");
    let mut panel = RecordingPanel::default();
    let mut settings = RecordingSettings::default();
    app.init_resource::<RecordingPanel>()
        .add_systems(Update, collect_recorder_events);

    // Save: a frame every 16 steps and a 2 s cap (128 steps) go to project.json.
    panel.draft = Some(RecordingSettings {
        keyframe_interval_steps: 16,
        max_duration_s: 2.0,
        ..RecordingSettings::default()
    });
    apply_panel_action(
        &PanelAction::Save,
        &mut panel,
        &mut settings,
        Some(q13::PLAN_ID),
        dir.path(),
        "S",
    );
    assert_eq!(panel.saved, Some(Ok(())));
    assert_eq!(settings.keyframe_interval_steps, 16);
    let saved = read_manifest_file(dir.path())
        .and_then(|report| report.manifest)
        .and_then(|manifest| manifest.recording)
        .expect("the recording block is in project.json");
    assert_eq!(
        (saved.keyframe_interval_steps, saved.max_duration_s),
        (Some(16), Some(2.0))
    );

    // The sink and the settings come from that file when the bodies are spawned.
    app.world_mut()
        .run_system_once(install_recording_sink)
        .expect("install_recording_sink runs");
    assert_eq!(
        app.world()
            .resource::<RecordingSettings>()
            .keyframe_interval_steps,
        16
    );

    // Record.
    let command = apply_panel_action(
        &PanelAction::Record,
        &mut panel,
        &mut settings,
        Some(q13::PLAN_ID),
        dir.path(),
        "20261030T120000Z",
    )
    .expect("Record sends a command");
    app.world_mut().write_message(command);
    tick(&mut app);
    for _ in 0..110 {
        tick(&mut app);
    }
    let status = {
        let world = app.world();
        panel_status(
            world.resource::<Recorder>(),
            world.resource::<RecordingSink>().0.bytes(),
            world.resource::<RecordingSettings>(),
            world.resource::<RecordingBodies>().roles.len(),
            world.resource::<RecordingPanel>().warned,
        )
    };
    assert!(status.recording, "{status:?}");
    assert!(status.near_cap, "110 of 128 steps is past 80 %: {status:?}");
    assert!(status.frames >= 7, "{status:?}");
    assert_eq!(status.bytes, status.frames * 78 * 116);

    // The 2 s cap stops it.
    for _ in 0..40 {
        tick(&mut app);
    }
    assert!(!app.world().resource::<Recorder>().is_recording());
    let notes = app.world().resource::<RecordingPanel>().notes.clone();
    assert!(notes.iter().any(|n| n.starts_with("Warning:")), "{notes:?}");
    assert!(notes.iter().any(|n| n.contains("MaxDuration")), "{notes:?}");

    // On disk: one finished recording, written by the app.
    let recordings = dir.path().join("recordings");
    let name = "example_plan-20261030T120000Z";
    let recording_dir = recordings.join(name);
    assert!(recording_dir.is_dir(), "{recording_dir:?}");
    assert!(!recordings.join(format!("{name}.partial")).exists());
    let manifest = read_recording_file(&recording_dir)
        .and_then(|report| report.manifest)
        .expect("a valid manifest");
    assert_eq!(manifest.source, Some(RecordingSource::App));
    assert_eq!(manifest.end_reason, Some(RecordingEndReason::MaxDuration));
    assert_eq!(manifest.keyframe_interval_steps, 16);
    assert!(manifest.project.is_some(), "the scenario is embedded");

    // The Recordings window's list.
    std::fs::create_dir_all(recordings.join("half.partial")).unwrap();
    std::fs::create_dir_all(recordings.join("bad")).unwrap();
    std::fs::write(recordings.join("bad").join("manifest.json"), "not json").unwrap();
    let entries = scan_recordings(&recordings);
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["bad", name], "the .partial folder is not listed");
    assert!(entries[0].error.is_some() && entries[0].duration_s == 0.0);
    let entry = &entries[1];
    assert!(entry.error.is_none() && entry.differences.is_empty());
    assert_eq!(
        entry.bodies, 78,
        "F-66 (P7.5 S2): the target and 77 tether bodies"
    );
    assert!(
        (entry.duration_s - 2.0).abs() < 0.01,
        "{}",
        entry.duration_s
    );
    assert!(entry.size_bytes > 78 * 116 * 8);
    // Another platform shows in the list.
    let mut edited = manifest.clone();
    edited.built_with.target = "elsewhere-os".to_string();
    save_recording_manifest(&recording_dir, &edited).unwrap();
    let entries = scan_recordings(&recordings);
    assert!(
        entries[1]
            .differences
            .iter()
            .any(|d| d.contains("elsewhere-os")),
        "{:?}",
        entries[1]
    );
    save_recording_manifest(&recording_dir, &manifest).unwrap();

    // Open: the player starts at the first frame; the visuals are the spawner's.
    let session = load_replay_session(&recording_dir).expect("the recording loads");
    assert_eq!(session.name, name);
    assert_eq!(session.roles.len(), 78);
    assert_eq!(session.player.start_step(), manifest.start_step);
    assert_eq!(session.player.end_step(), manifest.end_step.unwrap());
    let live = app.world().resource::<RecordingBodies>().clone();
    for (i, entity) in live.entities.iter().enumerate() {
        assert_eq!(
            session.shapes[i],
            app.world().get::<TetherShape>(*entity).copied(),
            "body {i} ({:?})",
            live.roles[i]
        );
    }
    assert!(app.world().get::<DebrisBody>(live.entities[0]).is_some());
    assert_eq!(
        replay_shapes(&session.roles),
        session.shapes,
        "the shapes come from the recording's body roles"
    );

    // The replay scene: one visual-only body per recorded body, at the first frame.
    let first = session.player.view();
    let mut scene = App::new();
    scene
        .add_message::<ssg_sim::telemetry::LogEvent>()
        .insert_resource(session);
    scene
        .world_mut()
        .run_system_once(spawn_replay_bodies)
        .expect("spawn_replay_bodies runs");
    let mut bodies = scene.world_mut().query::<(&ReplayBody, &Transform)>();
    let spawned: Vec<(usize, Vec3)> = bodies
        .iter(scene.world())
        .map(|(body, transform)| (body.index, transform.translation))
        .collect();
    assert_eq!(spawned.len(), 78);
    for (index, translation) in spawned {
        assert_eq!(
            translation,
            first.bodies[index].position.as_vec3(),
            "body {index}"
        );
    }
    // The camera follows the tether root, as in the live sim (the anchor).
    let mut followed = scene
        .world_mut()
        .query_filtered::<&ReplayBody, With<CameraTarget>>();
    let followed: Vec<usize> = followed.iter(scene.world()).map(|b| b.index).collect();
    assert_eq!(followed, [1]);
    assert_eq!(live.roles[1], RecordingBodyRole::TetherRoot);
}

/// P7.5 S2: the segment gizmo joins the running chain (root along the joints)
/// and the replay's bodies (body-table order) into the same 76 segments.
#[test]
fn the_tether_segments_join_the_running_chain_and_the_replay_bodies_alike() {
    let mut app = test_app();
    q13::build(&mut app);
    let chains = app
        .world_mut()
        .run_system_once(
            |cache: Res<OrbitalCache>,
             joints: Query<&DistanceJoint>,
             transforms: Query<&Transform>| {
                running_tether_chains(&cache, &joints, &transforms)
            },
        )
        .expect("running_tether_chains runs");
    assert_eq!(chains.len(), 1, "one tether");
    assert_eq!(chains[0].len() - 1, 76, "76 segments between 77 bodies");

    // The replay scene: one body per recorded body at the live position, spawned
    // in reverse so the chain must come from the body-table index.
    let live = app.world().resource::<RecordingBodies>().clone();
    let shapes = replay_shapes(&live.roles);
    let mut scene = App::new();
    for (index, (entity, shape)) in live.entities.iter().zip(shapes).enumerate().rev() {
        let at = app.world().get::<Transform>(*entity).unwrap().translation;
        let mut body = scene
            .world_mut()
            .spawn((ReplayBody { index }, Transform::from_translation(at)));
        if let Some(shape) = shape {
            body.insert(shape);
        }
    }
    let replay = scene
        .world_mut()
        .run_system_once(
            |bodies: Query<(&ReplayBody, &Transform), With<TetherShape>>| {
                replay_tether_chain(&bodies)
            },
        )
        .expect("replay_tether_chain runs");
    assert_eq!(replay, chains[0], "the same 77 points in the same order");
}
