//! `ssg-sim replay` (P7 S4, R7): a recording re-simulates bit for bit on the
//! machine that made it, whatever the spawn-update step count k (C1), across
//! floating-origin resets (C2) and with an input applied in a zero-step update; a
//! changed DOUBLE gives exit 1 (C9), another build exit 3, a missing
//! `SessionStart` k = 1 for `source: cli` and exit 2 otherwise. Recordings are made
//! in this process by the library recorder and the file sink, or by `ssg-sim run
//! --record`, and compared with themselves, so the tests hold on every platform.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use bevy::math::DVec3;
use bevy::prelude::*;
use serde_json::Value;
use ssg_sim::capture::SimCommand;
use ssg_sim::capture::plan::CapturePlanLibrary;
use ssg_sim::orbit::cache::OrbitalCache;
use ssg_sim::orbit::selection::OrbitalSelectionState;
use ssg_sim::project::SelectedProject;
use ssg_sim::project::manifest::ProjectManifest;
use ssg_sim::project::recording::{RecordingBodyRole, RecordingSource};
use ssg_sim::recording::RecordingSink;
use ssg_sim::recording::files::{
    FileSink, KeyframeRow, KeyframeWriter, RecordingContext, read_jsonl, read_keyframes,
    read_recording,
};
use ssg_sim::recording::inputs::{RecordedCommand, RecordedInput};
use ssg_sim::recording::player::Player;
use ssg_sim::recording::recorder::{RecorderCommand, RecordingSettings};
use ssg_sim::session::{PauseState, UiScreen};
use ssg_sim::test_support::{
    current_step, enter_sim, fixed_epoch, load_example_plans, q13, start_running, test_app, tick,
};
use ssg_sim::time::{PHYS_DT, WorldTime};

/// The Q13 fixture's GP epoch, as in the harness (`fixed_epoch()`).
const EPOCH: &str = "2026-04-03T16:03:47.002464";

const SAME_BUILD_DIFFERS: &str =
    "same target and version: a defect, or a different C library or toolchain (F-48)";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn temp_dir(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("ssg-sim-cli-replay-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// `ssg-sim replay <dir> --assets <repo>/assets <extra>`.
fn replay(dir: &Path, extra: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ssg-sim"))
        .arg("replay")
        .arg(dir)
        .arg("--assets")
        .arg(repo_root().join("assets"))
        .args(extra)
        .output()
        .expect("ssg-sim runs")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n")
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).replace("\r\n", "\n")
}

/// Asserts the exit code and returns stdout, printed for the record.
#[track_caller]
fn expect_exit(output: &Output, code: i32) -> String {
    let out = stdout(output);
    print!("{out}");
    assert_eq!(
        output.status.code(),
        Some(code),
        "stdout:\n{out}\nstderr:\n{}",
        stderr(output)
    );
    out
}

/// The `replay` line `frames: N, identical: M` for a recording of `frames` frames.
fn identical(frames: usize) -> String {
    format!("frames: {frames}, identical: {frames}")
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        std::fs::copy(entry.path(), to.join(entry.file_name())).unwrap();
    }
}

fn edit_manifest(dir: &Path, edit: impl FnOnce(&mut Value)) {
    let path = dir.join("manifest.json");
    let mut manifest: Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    edit(&mut manifest);
    std::fs::write(&path, serde_json::to_string_pretty(&manifest).unwrap()).unwrap();
}

fn write_inputs(dir: &Path, inputs: &[RecordedInput]) {
    let lines: String = inputs
        .iter()
        .map(|input| serde_json::to_string(input).unwrap() + "\n")
        .collect();
    std::fs::write(dir.join("inputs.jsonl"), lines).unwrap();
}

/// A harness app recording into `recordings` (the Q13 scenario, a frame every
/// `interval` steps), with the bodies selected; returns the app and the sink.
fn harness(recordings: &Path, interval: u32) -> (App, FileSink) {
    let mut app = test_app();
    load_example_plans(&mut app);
    let plan = app.world().resource::<CapturePlanLibrary>().plans[q13::PLAN_ID].clone();
    let selection = OrbitalSelectionState {
        target: Some(q13::catalog_pick("HST", q13::HST_NORAD_ID)),
        chaser: Some(q13::catalog_pick("P0 HST CHASER", q13::CHASER_NORAD_ID)),
    };
    let project =
        ProjectManifest::from_selection("q13", fixed_epoch(), Some(q13::PLAN_ID), &selection)
            .unwrap();
    let settings = RecordingSettings {
        keyframe_interval_steps: interval,
        ..RecordingSettings::default()
    };
    let sink = FileSink::new(RecordingContext {
        recordings_dir: recordings.to_path_buf(),
        default_name: "auto".to_string(),
        source: RecordingSource::Cli,
        plan_id: q13::PLAN_ID.to_string(),
        plan,
        project: Some(project),
        settings: settings.clone(),
        target_name: "HST".to_string(),
    });
    app.insert_resource(settings);
    app.insert_resource(RecordingSink(Box::new(sink.clone())));
    app.world_mut().resource_mut::<SelectedProject>().project_id = Some(q13::PLAN_ID.to_string());
    *app.world_mut().resource_mut::<OrbitalSelectionState>() = selection;
    (app, sink)
}

fn set_paused(app: &mut App, paused: bool) {
    app.world_mut()
        .resource_mut::<NextState<PauseState>>()
        .set(if paused {
            PauseState::Paused
        } else {
            PauseState::Running
        });
}

/// Queues the capture start and the recording start for the next update.
fn start_capture_and_recording(app: &mut App, name: &str) {
    let debris = app.world().resource::<OrbitalCache>().debris["Satellite1"];
    app.world_mut().write_message(SimCommand::StartCapture {
        target: Some(debris),
        plan_id: q13::PLAN_ID.to_string(),
    });
    app.world_mut().write_message(RecorderCommand::Start {
        name: name.to_string(),
    });
}

/// Leaves the sim screen (ending the recording) and returns the recording folder.
fn finish(mut app: App, sink: &FileSink) -> PathBuf {
    app.world_mut()
        .resource_mut::<NextState<UiScreen>>()
        .set(UiScreen::Home);
    app.update();
    let results = sink.results();
    assert_eq!(results.len(), 1, "{results:?}");
    results[0].as_ref().unwrap().dir.clone()
}

/// A recording whose spawn update ran `k` steps (as the app's Start click may,
/// `review-spawn_k_probe.rs`): `WorldTime.multiplier = k`, or paused for k = 0.
/// The capture and the recording start together, in an update that runs no step
/// when `zero_step_capture`, else in the next one; then the session runs at warp
/// `warp` (64 for k = 64, the warp raised before Start) until 48 steps later.
fn record_with_k(recordings: &Path, k: u32, zero_step_capture: bool) -> (PathBuf, u64) {
    let (mut app, sink) = harness(recordings, 16);
    enter_sim(&mut app);
    if k == 0 {
        set_paused(&mut app, true);
    } else {
        app.world_mut().resource_mut::<WorldTime>().multiplier = k;
    }
    start_running(&mut app);
    assert_eq!(
        current_step(&app),
        u64::from(k),
        "the spawn update ran k steps"
    );
    let warp = if k == 64 { 64 } else { 1 };
    app.world_mut().resource_mut::<WorldTime>().multiplier = warp;
    start_capture_and_recording(&mut app, &format!("k{k}"));
    if zero_step_capture {
        set_paused(&mut app, true);
        app.update();
        set_paused(&mut app, false);
    } else {
        set_paused(&mut app, false);
        app.update();
    }
    let capture_step = current_step(&app);
    while current_step(&app) < capture_step + 48 {
        tick(&mut app);
    }
    (finish(app, &sink), capture_step)
}

#[test]
fn recordings_with_each_spawn_update_step_count_replay_exactly() {
    let dir = temp_dir("k");
    let recordings = dir.join("recordings");
    // (k, capture in a zero-step update, the step the capture starts after)
    let cases = [
        (0, true, 0),
        (1, true, 1),
        (2, false, 3),
        (3, false, 4),
        (64, false, 128),
    ];
    for (k, zero_step, capture_after) in cases {
        let (folder, capture_step) = record_with_k(&recordings, k, zero_step);
        assert_eq!(capture_step, capture_after, "k = {k}");
        let inputs: Vec<RecordedInput> = read_jsonl(&folder.join("inputs.jsonl")).unwrap();
        assert_eq!(
            inputs[0].command,
            RecordedCommand::SessionStart {
                steps_before_propagators: k
            }
        );
        assert_eq!(inputs[1].after_step, capture_after);
        let frames = read_recording(&folder).unwrap().frames.len();
        println!("k = {k} (capture after step {capture_after}, zero-step: {zero_step}):");
        let out = expect_exit(&replay(&folder, &[]), 0);
        assert!(
            out.contains(&format!("with k = {k}: {}", identical(frames))),
            "{out}"
        );
    }

    // The driver really uses k: the k = 2 recording replayed as k = 1 differs.
    let folder = recordings.join("k2");
    let mut inputs: Vec<RecordedInput> = read_jsonl(&folder.join("inputs.jsonl")).unwrap();
    inputs[0] = RecordedInput {
        after_step: 1,
        command: RecordedCommand::SessionStart {
            steps_before_propagators: 1,
        },
    };
    write_inputs(&folder, &inputs);
    println!("k = 2 recording with its SessionStart edited to 1:");
    let out = expect_exit(&replay(&folder, &[]), 1);
    assert!(
        out.contains("with k = 1: frames: 4, identical: ") && !out.contains(&identical(4)),
        "{out}"
    );
    assert!(out.contains(SAME_BUILD_DIFFERS), "{out}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_recording_across_floating_origin_resets_replays_and_plays_continuously() {
    // Q13 resets the origin at steps 701 and 1,328 (macOS/aarch64).
    let last_step: u64 = 1400; // F-66 (P7.5 S1): provisional until S4
    let dir = temp_dir("resets");
    let (mut app, sink) = harness(&dir.join("recordings"), 16);
    enter_sim(&mut app);
    start_running(&mut app);
    let debris = app.world().resource::<OrbitalCache>().debris["Satellite1"];
    app.world_mut().write_message(SimCommand::StartCapture {
        target: Some(debris),
        plan_id: q13::PLAN_ID.to_string(),
    });
    while current_step(&app) < last_step {
        if current_step(&app) + 1 == 400 {
            app.world_mut().write_message(RecorderCommand::Start {
                name: "resets".to_string(),
            });
        }
        tick(&mut app);
    }
    let folder = finish(app, &sink);
    let recording = read_recording(&folder).unwrap();
    let shifted: Vec<u64> = recording
        .frames
        .iter()
        .filter(|f| f.line.origin_shift_m != [0.0; 3])
        .map(|f| f.line.step)
        .collect();
    assert!(!shifted.is_empty(), "no reset in steps 400–{last_step}");
    println!("frames carrying an origin shift: {shifted:?}");
    let out = expect_exit(&replay(&folder, &[]), 0);
    assert!(out.contains(&identical(recording.frames.len())), "{out}");

    // Playback one step at a time. Between two frames the player shows both in the
    // later frame's bubble frame (R6), so the bubble coordinates of every body and
    // of the Earth change frame together once per reset interval; what a viewer
    // sees, a body against the Earth, is its ECI position, the frame origin plus
    // the bubble position. No body's ECI position moves more than 1 m beyond its
    // own velocity × one step between cursors one step apart.
    let player = Player::from_recording(&recording, true).unwrap();
    let eci = |view: &ssg_sim::recording::player::FrameView, body: usize| {
        let origin = view.frame_origin_eci.expect("the anchor has a propagator");
        let b = &view.bodies[body];
        (
            DVec3::new(origin[0], origin[1], origin[2]) + b.position,
            DVec3::new(origin[3], origin[4], origin[5]) + b.velocity,
        )
    };
    let (mut largest, mut at) = (0.0_f64, 0);
    let mut largest_bubble = 0.0_f64;
    let mut previous = player.state_at(400.0);
    for step in 401..=last_step {
        let view = player.state_at(step as f64);
        for body in 0..view.bodies.len() {
            let ((p0, v0), (p1, _)) = (eci(&previous, body), eci(&view, body));
            let off = (p1 - p0 - v0 * PHYS_DT).length();
            if off > largest {
                (largest, at) = (off, step);
            }
            largest_bubble = largest_bubble
                .max((view.bodies[body].position - previous.bodies[body].position).length());
        }
        previous = view;
    }
    println!(
        "playback at cursors one step apart, steps 400–1100: largest ECI move beyond velocity × \
         step {largest:.6} m (at step {at}); largest bubble-frame move {largest_bubble:.3} m \
         (the frame change of a reset interval)"
    );
    assert!(largest <= 1.0, "{largest} m at step {at}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_run_record_recording_replays_and_each_edit_gives_its_exit_code() {
    let project = temp_dir("cli");
    std::fs::create_dir_all(project.join("datasets")).unwrap();
    std::fs::copy(
        repo_root().join("crates/ssg-sim/src/test_support/fixtures/q13_hubble_pair.json"),
        project.join("datasets/q13.json"),
    )
    .unwrap();
    let run = Command::new(env!("CARGO_BIN_EXE_ssg-sim"))
        .arg("run")
        .arg(&project)
        .args([
            "--target", "20580", "--chaser", "99901", "--epoch", EPOCH, "--steps", "128",
            "--record", "r",
        ])
        .arg("--assets")
        .arg(repo_root().join("assets"))
        .arg("--out")
        .arg(project.join("out"))
        .output()
        .unwrap();
    assert!(run.status.success(), "{}", stderr(&run));
    let recordings = project.join("recordings");
    let original = recordings.join("r");
    // Frames at steps 2, 34, 66, 98 and 130 (K = 32).
    println!("ssg-sim run --record r, then replay:");
    let out = expect_exit(&replay(&original, &[]), 0);
    assert!(out.contains("78 bodies, 5 frames, steps 2–130"), "{out}");
    assert!(out.contains("built with: this build"), "{out}");
    assert!(
        out.contains(&format!("with k = 1: {}", identical(5))),
        "{out}"
    );

    println!("--frames-only:");
    let out = expect_exit(&replay(&original, &["--frames-only"]), 0);
    assert!(
        out.contains("not re-simulated") && !out.contains("identical"),
        "{out}"
    );

    // Without `SessionStart`: k = 1 for a CLI recording, refused for an app one.
    let no_start = recordings.join("no-start");
    copy_dir(&original, &no_start);
    let inputs: Vec<RecordedInput> = read_jsonl(&no_start.join("inputs.jsonl")).unwrap();
    write_inputs(&no_start, &inputs[1..]);
    println!("SessionStart removed, source cli:");
    let out = expect_exit(&replay(&no_start, &[]), 0);
    assert!(
        out.contains(&format!("with k = 1: {}", identical(5))),
        "{out}"
    );
    edit_manifest(&no_start, |m| m["source"] = "app".into());
    println!("SessionStart removed, source app:");
    let output = replay(&no_start, &[]);
    expect_exit(&output, 2);
    println!("{}", stderr(&output));
    assert!(
        stderr(&output).contains("session_start"),
        "{}",
        stderr(&output)
    );

    // Not re-simulatable: no scenario in the manifest; a capture of a tether body.
    let broken = recordings.join("broken");
    copy_dir(&original, &broken);
    edit_manifest(&broken, |m| {
        m.as_object_mut().unwrap().remove("project");
    });
    let output = replay(&broken, &[]);
    expect_exit(&output, 2);
    assert!(
        stderr(&output).contains("cannot re-simulate: the manifest has no `project`"),
        "{}",
        stderr(&output)
    );
    assert_eq!(
        expect_exit(&replay(&broken, &["--frames-only"]), 0)
            .lines()
            .count(),
        3,
        "the files still check"
    );
    copy_dir(&original, &broken);
    let mut inputs: Vec<RecordedInput> = read_jsonl(&broken.join("inputs.jsonl")).unwrap();
    inputs[1].command = RecordedCommand::StartCapture {
        target: RecordingBodyRole::TetherTail,
        plan_id: q13::PLAN_ID.to_string(),
    };
    write_inputs(&broken, &inputs);
    let output = replay(&broken, &[]);
    expect_exit(&output, 2);
    assert!(
        stderr(&output).contains("starts a capture of a tether_tail"),
        "{}",
        stderr(&output)
    );

    // One DOUBLE changed in its lowest bit: frame step 34, body 5, velocity x.
    let flipped = recordings.join("flipped");
    copy_dir(&original, &flipped);
    let table = flipped.join("keyframes.parquet");
    let mut rows: Vec<KeyframeRow> = Vec::new();
    read_keyframes(&table, |row| rows.push(row)).unwrap();
    let row = rows
        .iter_mut()
        .find(|r| r.step == 34 && r.body == 5)
        .unwrap();
    row.values[7] = f64::from_bits(row.values[7].to_bits() ^ 1);
    let mut writer = KeyframeWriter::create(&table).unwrap();
    for frame in rows.chunks(78) {
        let bodies: Vec<_> = frame.iter().map(KeyframeRow::state).collect();
        writer.push_frame(frame[0].step as u64, &bodies).unwrap();
    }
    writer.finish().unwrap();
    println!("one DOUBLE changed in its lowest bit:");
    let out = expect_exit(&replay(&flipped, &[]), 1);
    assert!(out.contains("frames: 5, identical: 4"), "{out}");
    assert!(
        out.contains("first difference: frame step 34, body 5 (tether_node), |Δposition| 0e0 m"),
        "{out}"
    );
    assert!(out.contains(SAME_BUILD_DIFFERS), "{out}");

    // The same difference in a recording another platform made: exit 3.
    edit_manifest(&flipped, |m| {
        m["built_with"]["target"] = "riscv64-plan9".into()
    });
    println!("the same, with built_with.target edited:");
    let out = expect_exit(&replay(&flipped, &[]), 3);
    assert!(
        out.contains("built with: platform riscv64-plan9 (this build is"),
        "{out}"
    );
    assert!(
        out.contains("(F-48)") && !out.contains(SAME_BUILD_DIFFERS),
        "{out}"
    );
    let _ = std::fs::remove_dir_all(&project);
}

#[test]
fn bad_replay_arguments_and_folders_exit_2() {
    let dir = temp_dir("usage");
    for (args, message) in [
        (vec!["replay"], "missing <recording-dir>"),
        (vec!["replay", "a", "b"], "unexpected argument `b`"),
        (vec!["replay", "a", "--nope"], "unknown option `--nope`"),
        (vec!["replay", "a", "--assets"], "--assets needs a value"),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_ssg-sim"))
            .args(&args)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(
            stderr(&output).contains(message),
            "{args:?}: {}",
            stderr(&output)
        );
    }
    let output = replay(&dir.join("missing"), &[]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output).contains("manifest.json"),
        "{}",
        stderr(&output)
    );
    let _ = std::fs::remove_dir_all(&dir);
}
