//! The recording files (P7 S3, R3, R8): the Parquet keyframe table, the JSON Lines
//! files, the file sink and the checks `ssg-sim validate` runs. Text files are
//! compared with their line endings normalised (P6 CR LF lesson).

use std::fs;
use std::path::Path;

use bevy::math::{DQuat, DVec3};
use brahe::Epoch;
use nalgebra::Vector6;
use parquet::basic::{Compression, Encoding};
use parquet::file::reader::{FileReader, SerializedFileReader};

use crate::capture::plan::CapturePlanLibrary;
use crate::orbit::selection::OrbitalSelectionState;
use crate::project::manifest::{ProjectManifest, RecordingBlock};
use crate::project::recording::{
    RECORDING_FRAMES_FILE, RECORDING_INPUTS_FILE, RECORDING_KEYFRAMES_PARQUET, RECORDING_LOG_FILE,
    RECORDING_MANIFEST_FILE, RECORDING_TRANSITIONS_FILE, RecordingBodyRole, RecordingEndReason,
    RecordingSource, RecordingStartTrigger, read_recording_file,
};
use crate::recording::files::{
    FileSink, FrameLine, KeyframeRow, KeyframeWriter, ROW_GROUP_FRAMES, RecordingContext,
    TransitionLine, check_recording, list_recording_dirs, read_jsonl, read_keyframes,
    read_recording,
};
use crate::recording::inputs::{RecordedCommand, RecordedInput};
use crate::recording::recorder::{RecorderCommand, RecordingSettings};
use crate::recording::{
    BodyFrame, EndReason, Frame, FrameSink, RecordedTransition, RecordingEnd, RecordingHeader,
    RecordingSink, StartTrigger,
};
use crate::telemetry::{LogEntry, LogLevel};
use crate::test_support::{
    Lcg, TempDir, current_step, fixed_epoch, load_example_plans, q13, test_app, tick,
};
use crate::time::PHYS_DT;

const SEED: u64 = 0x7E57_F11E_5;

fn random_body(rng: &mut Lcg) -> BodyFrame {
    let mut v = |scale: f64| rng.range(-scale, scale);
    BodyFrame {
        position: DVec3::new(v(1e4), v(1e4), v(1e4)),
        rotation: DQuat::from_xyzw(v(1.0), v(1.0), v(1.0), v(1.0)),
        velocity: DVec3::new(v(10.0), v(10.0), v(10.0)),
        angular_velocity: DVec3::new(v(1.0), v(1.0), v(1.0)),
    }
}

/// A frame at `step` with `bodies` random bodies and awkward floats in the scalars.
fn frame(rng: &mut Lcg, step: u64, bodies: usize) -> Frame {
    Frame {
        step,
        epoch: fixed_epoch() + step as f64 * PHYS_DT,
        phase: (step % 3 != 0).then(|| "approach".to_string()),
        capture_sphere_radius_m: rng.range(0.0, 100.0),
        frame_origin_eci: (step % 5 != 0).then(|| {
            Vector6::new(
                rng.range(-7e6, 7e6),
                rng.range(-7e6, 7e6),
                rng.range(-7e6, 7e6),
                rng.range(-8e3, 8e3),
                rng.range(-8e3, 8e3),
                rng.range(-8e3, 8e3),
            )
        }),
        origin_shift_m: if step % 7 == 0 {
            DVec3::new(rng.range(-50.0, 50.0), -0.0, f64::MIN_POSITIVE / 4.0)
        } else {
            DVec3::ZERO
        },
        origin_shift_mps: DVec3::new(rng.range(-1.0, 1.0), 1e-300, 1e300),
        bodies: (0..bodies).map(|_| random_body(rng)).collect(),
    }
}

fn bits(state: &BodyFrame) -> [u64; 13] {
    KeyframeRow::new(0, 0, state).values.map(f64::to_bits)
}

fn text(path: &Path) -> String {
    fs::read_to_string(path).unwrap().replace("\r\n", "\n")
}

// ---------------------------------------------------------------------------
// the Parquet keyframe table
// ---------------------------------------------------------------------------

fn write_table(path: &Path, frames: &[Frame]) {
    let mut writer = KeyframeWriter::create(path).unwrap();
    for f in frames {
        writer.push_frame(f.step, &f.bodies).unwrap();
    }
    assert_eq!(
        writer.finish().unwrap(),
        frames.iter().map(|f| f.bodies.len() as u64).sum::<u64>()
    );
}

#[test]
fn the_keyframe_table_round_trips_bit_for_bit_with_a_short_last_row_group() {
    let dir = TempDir::new("keyframes_round_trip");
    let mut rng = Lcg::new(SEED);
    // (frames, row groups): one short group, exactly full, one frame over, three.
    for (n, groups) in [(1usize, 1usize), (10, 1), (64, 1), (65, 2), (129, 3)] {
        let bodies = 5;
        let mut frames: Vec<Frame> = (0..n)
            .map(|i| frame(&mut rng, 100 + 32 * i as u64, bodies))
            .collect();
        // A NaN payload, -0.0 and a subnormal must survive: PLAIN stores the bits.
        frames[0].bodies[0].position.x = f64::from_bits(0x7ff8_dead_beef_0001);
        frames[0].bodies[0].velocity.y = -0.0;
        frames[0].bodies[0].angular_velocity.z = f64::MIN_POSITIVE / 8.0;
        let path = dir.path().join(format!("k{n}.parquet"));
        write_table(&path, &frames);

        let mut rows = Vec::new();
        let read_groups = read_keyframes(&path, |row| rows.push(row)).unwrap();
        assert_eq!(
            read_groups, groups,
            "{n} frames: row groups (seed {SEED:#x})"
        );
        assert_eq!(rows.len(), n * bodies, "{n} frames: rows");
        for (i, row) in rows.iter().enumerate() {
            let (f, b) = (&frames[i / bodies], i % bodies);
            assert_eq!(row.step, f.step as i64, "{n} frames: row {i} step");
            assert_eq!(row.body, b as i32, "{n} frames: row {i} body");
            assert_eq!(
                row.values.map(f64::to_bits),
                bits(&f.bodies[b]),
                "{n} frames: row {i} columns, bit for bit (seed {SEED:#x})"
            );
            assert_eq!(bits(&row.state()), bits(&f.bodies[b]));
        }
    }
}

#[test]
fn the_row_groups_hold_64_frames_and_the_last_one_the_rest() {
    let dir = TempDir::new("keyframes_groups");
    let mut rng = Lcg::new(SEED);
    let bodies = 3;
    let frames: Vec<Frame> = (0..ROW_GROUP_FRAMES + 1)
        .map(|i| frame(&mut rng, i as u64, bodies))
        .collect();
    let path = dir.path().join("k.parquet");
    write_table(&path, &frames);
    let reader = SerializedFileReader::new(fs::File::open(&path).unwrap()).unwrap();
    let rows: Vec<i64> = (0..reader.num_row_groups())
        .map(|g| reader.metadata().row_group(g).num_rows())
        .collect();
    assert_eq!(rows, [(ROW_GROUP_FRAMES * bodies) as i64, bodies as i64]);
}

#[test]
fn the_table_is_plain_uncompressed_and_116_bytes_a_row() {
    let dir = TempDir::new("keyframes_plain");
    let mut rng = Lcg::new(SEED);
    let (n, bodies) = (64, 193);
    let frames: Vec<Frame> = (0..n).map(|i| frame(&mut rng, i as u64, bodies)).collect();
    let path = dir.path().join("k.parquet");
    write_table(&path, &frames);
    let reader = SerializedFileReader::new(fs::File::open(&path).unwrap()).unwrap();
    let group = reader.metadata().row_group(0);
    assert_eq!(group.num_columns(), 15);
    for column in group.columns() {
        assert_eq!(column.compression(), Compression::UNCOMPRESSED);
        // PLAIN values; RLE is only the (empty) level encoding. No dictionary.
        let encodings: Vec<Encoding> = column.encodings().collect();
        assert!(encodings.contains(&Encoding::PLAIN), "{encodings:?}");
        assert!(
            encodings
                .iter()
                .all(|e| matches!(e, Encoding::PLAIN | Encoding::RLE)),
            "{encodings:?}"
        );
    }
    let raw = (n * bodies * 116) as f64;
    let size = fs::metadata(&path).unwrap().len() as f64;
    assert!(
        size >= raw && size < raw * 1.01,
        "{size} bytes for {raw} bytes of rows"
    );
}

#[test]
fn a_file_that_is_not_a_keyframe_table_is_refused() {
    let dir = TempDir::new("keyframes_refused");
    let garbage = dir.write("garbage.parquet", "this is not parquet");
    let error = read_keyframes(&garbage, |_| {}).unwrap_err();
    assert!(error.starts_with("keyframes.parquet: "), "{error}");
    let missing = dir.path().join("missing.parquet");
    assert!(
        read_keyframes(&missing, |_| {})
            .unwrap_err()
            .starts_with("cannot open ")
    );
    assert!(
        KeyframeWriter::create(&dir.path().join("no/such/folder/k.parquet"))
            .err()
            .unwrap()
            .starts_with("cannot create ")
    );
}

// ---------------------------------------------------------------------------
// the file sink
// ---------------------------------------------------------------------------

fn example_plan() -> crate::capture::plan::CapturePlan {
    let mut app = test_app();
    load_example_plans(&mut app);
    app.world().resource::<CapturePlanLibrary>().plans[q13::PLAN_ID].clone()
}

fn q13_project() -> ProjectManifest {
    let mut selection = OrbitalSelectionState::default();
    selection.target = Some(q13::catalog_pick("HST", q13::HST_NORAD_ID));
    selection.chaser = Some(q13::catalog_pick("P0 HST CHASER", q13::CHASER_NORAD_ID));
    ProjectManifest::from_selection("q13", fixed_epoch(), Some(q13::PLAN_ID), &selection).unwrap()
}

fn context(recordings: &Path) -> RecordingContext {
    RecordingContext {
        recordings_dir: recordings.to_path_buf(),
        default_name: "auto".to_string(),
        source: RecordingSource::Cli,
        plan_id: q13::PLAN_ID.to_string(),
        plan: example_plan(),
        project: Some(q13_project()),
        settings: RecordingSettings::default(),
        target_name: "HST".to_string(),
    }
}

fn header(name: Option<&str>, start: u64, roles: usize) -> RecordingHeader {
    let mut all = vec![RecordingBodyRole::Target, RecordingBodyRole::TetherRoot];
    all.extend(std::iter::repeat_n(
        RecordingBodyRole::TetherNode,
        roles - 3,
    ));
    all.push(RecordingBodyRole::TetherTail);
    RecordingHeader {
        name: name.map(str::to_string),
        start_step: start,
        keyframe_interval_steps: 16,
        trigger: StartTrigger::Phase,
        start_phase: Some("approach".to_string()),
        roles: all,
    }
}

fn input(step: u64, command: RecordedCommand) -> RecordedInput {
    RecordedInput {
        after_step: step,
        command,
    }
}

fn session_inputs() -> Vec<RecordedInput> {
    vec![
        input(
            1,
            RecordedCommand::SessionStart {
                steps_before_propagators: 1,
            },
        ),
        input(
            2,
            RecordedCommand::StartCapture {
                target: RecordingBodyRole::Target,
                plan_id: q13::PLAN_ID.to_string(),
            },
        ),
    ]
}

fn transition(step: u64, from: &str, to: &str) -> RecordedTransition {
    RecordedTransition {
        step,
        epoch: fixed_epoch() + step as f64 * PHYS_DT,
        from: from.to_string(),
        to: to.to_string(),
        reason: format!("{from} → {to}: the \"range\" fell below 100 m"),
    }
}

fn log_entry(message: &str) -> LogEntry {
    LogEntry {
        timestamp: "16:03:47".to_string(),
        epoch_utc: "2026-04-03T16:03:47.002464Z".to_string(),
        level: LogLevel::Info,
        source: "capture",
        message: message.to_string(),
    }
}

/// Records `frames` (steps 100, 116, …) through `sink` with the given extras.
fn record(sink: &mut FileSink, name: Option<&str>, frames: &[Frame], inputs: &[RecordedInput]) {
    sink.begin(&header(name, frames[0].step, frames[0].bodies.len()));
    for (i, f) in frames.iter().enumerate() {
        sink.frame(f);
        if i == 1 {
            sink.transition(&transition(f.step, "rendezvous", "approach"));
            sink.log(&log_entry("Transition rendezvous → approach"));
        }
    }
    sink.inputs(inputs);
    sink.finish(&RecordingEnd {
        end_step: frames.last().unwrap().step,
        reason: EndReason::EndSuccess,
    });
}

fn sample_frames(n: usize, bodies: usize) -> Vec<Frame> {
    let mut rng = Lcg::new(SEED);
    (0..n)
        .map(|i| frame(&mut rng, 100 + 16 * i as u64, bodies))
        .collect()
}

#[test]
fn the_sink_writes_the_layout_into_a_partial_folder_and_renames_it_when_done() {
    let dir = TempDir::new("sink_layout");
    let recordings = dir.path().join("recordings");
    let mut sink = FileSink::new(context(&recordings));
    let frames = sample_frames(5, 6);

    sink.begin(&header(Some("r1"), 100, 6));
    assert!(recordings.join("r1.partial").is_dir());
    assert!(!recordings.join("r1").exists());
    for f in &frames {
        sink.frame(f);
    }
    assert_eq!(sink.bytes(), 5 * 6 * 116, "the table's bytes so far");
    sink.inputs(&session_inputs());
    sink.finish(&RecordingEnd {
        end_step: 164,
        reason: EndReason::Manual,
    });

    assert!(!recordings.join("r1.partial").exists());
    let folder = recordings.join("r1");
    for file in [
        RECORDING_MANIFEST_FILE,
        RECORDING_INPUTS_FILE,
        RECORDING_KEYFRAMES_PARQUET,
        RECORDING_FRAMES_FILE,
        RECORDING_TRANSITIONS_FILE,
        RECORDING_LOG_FILE,
    ] {
        assert!(folder.join(file).is_file(), "{file}");
    }
    assert_eq!(sink.bytes(), 0, "nothing is recording");
    let results = sink.results();
    assert_eq!(results.len(), 1);
    let written = results[0].as_ref().unwrap();
    assert_eq!((written.frames, written.bodies), (5, 6));
    assert_eq!(written.dir, folder);
    assert!(written.keyframes_bytes >= 5 * 6 * 116);
}

#[test]
fn every_file_reads_back_what_was_recorded() {
    let dir = TempDir::new("sink_round_trip");
    let recordings = dir.path().join("recordings");
    let mut sink = FileSink::new(context(&recordings));
    let frames = sample_frames(70, 6);
    let inputs = session_inputs();
    record(&mut sink, Some("r"), &frames, &inputs);
    let folder = recordings.join("r");

    let loaded = read_recording(&folder).unwrap();
    // manifest
    let manifest = &loaded.manifest;
    assert_eq!(manifest.plan_id, q13::PLAN_ID);
    assert_eq!(manifest.start_step, 100);
    assert_eq!(manifest.end_step, Some(100 + 16 * 69));
    assert_eq!(manifest.end_reason, Some(RecordingEndReason::EndSuccess));
    assert_eq!(manifest.start_trigger, Some(RecordingStartTrigger::Phase));
    assert_eq!(manifest.start_phase.as_deref(), Some("approach"));
    assert_eq!(manifest.source, Some(RecordingSource::Cli));
    assert_eq!(manifest.keyframe_interval_steps, 16);
    assert_eq!(manifest.limits.max_size_mb, Some(100.0));
    assert_eq!(manifest.limits.max_tether_bodies, 200);
    assert_eq!(manifest.project, Some(q13_project()));
    let names: Vec<_> = manifest.bodies.iter().map(|b| b.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "HST",
            "Tether root",
            "Tether node 1",
            "Tether node 2",
            "Tether node 3",
            "Tether tail"
        ]
    );
    assert_eq!(
        manifest.start_epoch_utc,
        crate::project::format_utc(frames[0].epoch)
    );
    // frames: the scalars and all 13 columns of every body, bit for bit
    assert_eq!(loaded.frames.len(), 70);
    for (read, written) in loaded.frames.iter().zip(&frames) {
        assert_eq!(read.line, FrameLine::new(written), "step {}", written.step);
        assert_eq!(read.line.step, written.step);
        for (r, w) in read.bodies.iter().zip(&written.bodies) {
            assert_eq!(bits(r), bits(w), "step {} (seed {SEED:#x})", written.step);
        }
    }
    let first = &loaded.frames[0].line;
    assert_eq!(first.phase, frames[0].phase);
    assert_eq!(
        first.origin_shift_m.map(f64::to_bits),
        frames[0].origin_shift_m.to_array().map(f64::to_bits)
    );
    // inputs, transitions, log
    assert_eq!(loaded.inputs, inputs);
    assert_eq!(
        loaded.transitions,
        [TransitionLine::new(&transition(
            116,
            "rendezvous",
            "approach"
        ))]
    );
    let log: Vec<serde_json::Value> = read_jsonl(&folder.join(RECORDING_LOG_FILE)).unwrap();
    assert_eq!(log.len(), 1);
    assert_eq!(log[0]["message"], "Transition rendezvous → approach");
    assert_eq!(log[0]["level"], "Info");
    assert_eq!(log[0]["epoch_utc"], "2026-04-03T16:03:47.002464Z");
    // inputs.jsonl starts with SessionStart in the design's shape
    assert_eq!(
        text(&folder.join(RECORDING_INPUTS_FILE))
            .lines()
            .next()
            .unwrap(),
        r#"{"step":1,"command":{"session_start":{"steps_before_propagators":1}}}"#
    );
    assert_eq!(
        text(&folder.join(RECORDING_INPUTS_FILE))
            .lines()
            .nth(1)
            .unwrap(),
        format!(
            r#"{{"step":2,"command":{{"start_capture":{{"target":"target","plan_id":"{}"}}}}}}"#,
            q13::PLAN_ID
        )
    );
    // the files are valid under the checks
    let check = check_recording(&folder, "recordings/r");
    assert_eq!(check.errors, Vec::<String>::new());
    assert_eq!(check.warnings, Vec::<String>::new());
    assert_eq!(
        check.summary.as_deref(),
        Some("recordings/r: 70 frames, 6 bodies, steps 100..1204")
    );
}

#[test]
fn the_files_read_with_crlf_line_endings_too() {
    let dir = TempDir::new("sink_crlf");
    let recordings = dir.path().join("recordings");
    let mut sink = FileSink::new(context(&recordings));
    record(
        &mut sink,
        Some("r"),
        &sample_frames(4, 5),
        &session_inputs(),
    );
    let folder = recordings.join("r");
    for file in [
        RECORDING_INPUTS_FILE,
        RECORDING_FRAMES_FILE,
        RECORDING_TRANSITIONS_FILE,
        RECORDING_LOG_FILE,
    ] {
        let converted = text(&folder.join(file)).replace('\n', "\r\n");
        fs::write(folder.join(file), converted).unwrap();
    }
    let loaded = read_recording(&folder).unwrap();
    assert_eq!((loaded.frames.len(), loaded.inputs.len()), (4, 2));
    assert_eq!(loaded.transitions.len(), 1);
    assert!(check_recording(&folder, "r").errors.is_empty());
}

#[test]
fn an_automatic_recording_takes_the_default_name_and_a_free_suffix() {
    let dir = TempDir::new("sink_names");
    let recordings = dir.path().join("recordings");
    let mut sink = FileSink::new(context(&recordings));
    let frames = sample_frames(2, 5);
    for _ in 0..3 {
        record(&mut sink, None, &frames, &session_inputs());
    }
    let names: Vec<String> = list_recording_dirs(&recordings)
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["auto", "auto-2", "auto-3"]);
    assert!(sink.results().iter().all(Result::is_ok));
}

#[test]
fn a_named_recording_never_overwrites_another_and_the_error_is_a_result() {
    let dir = TempDir::new("sink_exists");
    let recordings = dir.path().join("recordings");
    let mut sink = FileSink::new(context(&recordings));
    let frames = sample_frames(2, 5);
    record(&mut sink, Some("same"), &frames, &session_inputs());
    let before = text(&recordings.join("same").join(RECORDING_FRAMES_FILE));
    record(&mut sink, Some("same"), &frames, &session_inputs());
    let results = sink.results();
    assert!(results[0].is_ok());
    assert_eq!(
        results[1].as_ref().unwrap_err(),
        "recording `same` already exists"
    );
    assert_eq!(
        text(&recordings.join("same").join(RECORDING_FRAMES_FILE)),
        before
    );
    // The name is a file stem: no path separators.
    record(&mut sink, Some("../escape"), &frames, &session_inputs());
    assert!(recordings.join("___escape").is_dir());
    assert!(!dir.path().join("escape").exists());
}

#[test]
fn an_io_error_is_reported_when_the_recording_ends() {
    let dir = TempDir::new("sink_io");
    // `recordings` is a file, so no folder can be made under it.
    let blocked = dir.write("recordings", "in the way");
    let mut sink = FileSink::new(context(&blocked));
    record(
        &mut sink,
        Some("r"),
        &sample_frames(3, 5),
        &session_inputs(),
    );
    let results = sink.results();
    assert_eq!(results.len(), 1);
    assert!(
        results[0]
            .as_ref()
            .unwrap_err()
            .starts_with("cannot create "),
        "{results:?}"
    );
    // Calls with nothing recording are ignored.
    sink.frame(&sample_frames(1, 5)[0]);
    sink.transition(&transition(1, "a", "b"));
    sink.log(&log_entry("x"));
    sink.inputs(&[]);
    sink.finish(&RecordingEnd {
        end_step: 1,
        reason: EndReason::Manual,
    });
    assert_eq!(sink.results().len(), 1);
}

// ---------------------------------------------------------------------------
// the checks of `ssg-sim validate` (R8)
// ---------------------------------------------------------------------------

/// A valid recording named `r` of 5 frames (steps 100 … 164, K = 16) in `dir`.
fn valid_recording(dir: &TempDir, source: RecordingSource) -> std::path::PathBuf {
    let recordings = dir.path().join("recordings");
    let mut ctx = context(&recordings);
    ctx.source = source;
    let mut sink = FileSink::new(ctx);
    record(
        &mut sink,
        Some("r"),
        &sample_frames(5, 5),
        &session_inputs(),
    );
    recordings.join("r")
}

fn errors_of(folder: &Path) -> Vec<String> {
    check_recording(folder, "recordings/r").errors
}

fn rewrite_manifest(folder: &Path, edit: impl FnOnce(&mut serde_json::Value)) {
    let path = folder.join(RECORDING_MANIFEST_FILE);
    let mut value: serde_json::Value = serde_json::from_str(&text(&path)).unwrap();
    edit(&mut value);
    fs::write(path, serde_json::to_string_pretty(&value).unwrap()).unwrap();
}

#[test]
fn a_missing_or_invalid_manifest_is_an_error() {
    let dir = TempDir::new("check_manifest");
    let folder = valid_recording(&dir, RecordingSource::Cli);
    rewrite_manifest(&folder, |v| v["plan_id"] = serde_json::json!(" "));
    assert_eq!(
        errors_of(&folder),
        ["[recordings/r/manifest.json] /plan_id: must not be blank"]
    );
    fs::remove_file(folder.join(RECORDING_MANIFEST_FILE)).unwrap();
    assert_eq!(errors_of(&folder), ["[recordings/r/manifest.json] missing"]);
}

#[test]
fn a_missing_keyframe_table_is_an_error() {
    let dir = TempDir::new("check_no_table");
    let folder = valid_recording(&dir, RecordingSource::Cli);
    fs::remove_file(folder.join(RECORDING_KEYFRAMES_PARQUET)).unwrap();
    assert_eq!(
        errors_of(&folder),
        ["[recordings/r/keyframes.parquet] missing"]
    );
    assert!(read_recording(&folder).is_err());
}

#[test]
fn a_keyframe_table_that_does_not_read_is_an_error() {
    let dir = TempDir::new("check_bad_table");
    let folder = valid_recording(&dir, RecordingSource::Cli);
    fs::write(folder.join(RECORDING_KEYFRAMES_PARQUET), "no footer").unwrap();
    let errors = errors_of(&folder);
    assert!(
        errors[0].starts_with("[recordings/r/keyframes.parquet] keyframes.parquet: "),
        "{errors:?}"
    );
}

#[test]
fn the_row_count_the_steps_and_the_cadence_of_the_table_are_checked() {
    let dir = TempDir::new("check_table");
    let folder = valid_recording(&dir, RecordingSource::Cli);
    let table = folder.join(RECORDING_KEYFRAMES_PARQUET);
    let mut rng = Lcg::new(SEED);
    let at = |step: u64, rng: &mut Lcg| frame(rng, step, 5);
    let cases: Vec<(&str, Vec<Frame>, &str)> = vec![
        (
            "a body missing from the last frame",
            {
                let mut frames: Vec<Frame> = [100, 116, 132].map(|s| at(s, &mut rng)).to_vec();
                frames[2].bodies.pop();
                frames
            },
            "[recordings/r/keyframes.parquet] has 14 rows; 5 bodies × 3 frames would be 15",
        ),
        (
            "steps not increasing",
            [100, 116, 116].map(|s| at(s, &mut rng)).to_vec(),
            "[recordings/r/keyframes.parquet] steps are not strictly increasing (116 then 116)",
        ),
        (
            "a step off the cadence",
            [100, 116, 140, 148].map(|s| at(s, &mut rng)).to_vec(),
            "[recordings/r/keyframes.parquet] step 140 is not start_step 100 plus a multiple of 16",
        ),
        (
            "a first step before start_step",
            [96, 112].map(|s| at(s, &mut rng)).to_vec(),
            "[recordings/r/keyframes.parquet] starts at step 96, not at start_step 100",
        ),
    ];
    for (what, frames, expected) in cases {
        write_table(&table, &frames);
        let errors = errors_of(&folder);
        assert!(
            errors.iter().any(|e| e == expected),
            "{what}: {errors:?} should contain {expected}"
        );
    }
    // The last frame may be off the cadence (a stop), but must be the end_step.
    write_table(&table, &[100, 116, 132, 137].map(|s| frame(&mut rng, s, 5)));
    let errors = errors_of(&folder);
    assert!(
        errors.contains(
            &"[recordings/r/keyframes.parquet] ends at step 137, not at end_step 164".to_string()
        ),
        "{errors:?}"
    );
    assert!(
        errors.iter().all(|e| !e.contains("plus a multiple of")),
        "{errors:?}"
    );
}

#[test]
fn frames_jsonl_must_have_the_tables_steps() {
    let dir = TempDir::new("check_frames");
    let folder = valid_recording(&dir, RecordingSource::Cli);
    let path = folder.join(RECORDING_FRAMES_FILE);
    let mut lines: Vec<String> = text(&path).lines().map(str::to_string).collect();
    lines.pop();
    fs::write(&path, lines.join("\n")).unwrap();
    assert_eq!(
        errors_of(&folder),
        [
            "[recordings/r/frames.jsonl] has 4 frames; the keyframe table has 5, or their steps differ"
        ]
    );
    fs::write(&path, "{not json").unwrap();
    let errors = errors_of(&folder);
    assert!(
        errors[0].starts_with("[recordings/r/frames.jsonl] frames.jsonl: line 1: "),
        "{errors:?}"
    );
    fs::remove_file(&path).unwrap();
    assert!(
        errors_of(&folder)[0].starts_with("[recordings/r/frames.jsonl] frames.jsonl: cannot open")
    );
}

#[test]
fn a_missing_session_start_is_a_warning_for_the_cli_and_an_error_for_the_app() {
    for (source, as_error) in [(RecordingSource::Cli, false), (RecordingSource::App, true)] {
        let dir = TempDir::new("check_session_start");
        let folder = valid_recording(&dir, source);
        let inputs = folder.join(RECORDING_INPUTS_FILE);
        let kept: Vec<String> = text(&inputs).lines().skip(1).map(str::to_string).collect();
        fs::write(&inputs, kept.join("\n")).unwrap();
        let check = check_recording(&folder, "recordings/r");
        let message = "[recordings/r/inputs.jsonl] does not start with a `session_start` entry (the spawn-update step count)";
        if as_error {
            assert_eq!(check.errors, [message], "{source:?}");
            assert!(check.warnings.is_empty());
            assert_eq!(
                check.summary, None,
                "a recording with errors has no summary"
            );
        } else {
            assert!(check.errors.is_empty(), "{:?}", check.errors);
            assert_eq!(check.warnings, [format!("{message}; k = 1 is assumed")]);
            assert!(check.summary.is_some());
        }
        // No inputs file at all is the same as an empty one.
        fs::remove_file(&inputs).unwrap();
        let check = check_recording(&folder, "recordings/r");
        assert_eq!(check.errors.len(), usize::from(as_error), "{source:?}");
    }
}

#[test]
fn unordered_inputs_and_transitions_are_errors() {
    let dir = TempDir::new("check_order");
    let folder = valid_recording(&dir, RecordingSource::Cli);
    let inputs = folder.join(RECORDING_INPUTS_FILE);
    let reversed: Vec<String> = {
        let mut lines: Vec<String> = text(&inputs).lines().map(str::to_string).collect();
        lines.push(lines[0].replace("\"step\":1", "\"step\":0"));
        lines
    };
    fs::write(&inputs, reversed.join("\n")).unwrap();
    assert_eq!(
        errors_of(&folder),
        ["[recordings/r/inputs.jsonl] is not ordered by step"]
    );

    let second = TempDir::new("check_order2");
    let folder = valid_recording(&second, RecordingSource::Cli);
    let path = folder.join(RECORDING_TRANSITIONS_FILE);
    let one = text(&path).lines().next().unwrap().to_string();
    let earlier = one.replace("\"step\":116", "\"step\":108");
    fs::write(&path, format!("{one}\n{earlier}\n")).unwrap();
    assert_eq!(
        errors_of(&folder),
        ["[recordings/r/transitions.jsonl] is not ordered by step"]
    );
}

#[test]
fn transitions_outside_the_recording_are_errors_and_a_missing_file_a_warning() {
    let dir = TempDir::new("check_transitions");
    let folder = valid_recording(&dir, RecordingSource::Cli);
    let path = folder.join(RECORDING_TRANSITIONS_FILE);
    let one = text(&path).lines().next().unwrap().to_string();
    for (step, expected) in [(99, "99"), (165, "165")] {
        fs::write(
            &path,
            one.replace("\"step\":116", &format!("\"step\":{step}")),
        )
        .unwrap();
        assert_eq!(
            errors_of(&folder),
            [format!(
                "[recordings/r/transitions.jsonl] step {expected} is outside the recording's steps 100..164"
            )]
        );
    }
    fs::remove_file(&path).unwrap();
    let check = check_recording(&folder, "recordings/r");
    assert!(check.errors.is_empty());
    assert_eq!(check.warnings.len(), 1);
    assert!(check.warnings[0].contains("transitions.jsonl] missing"));
    fs::write(&path, "[1]").unwrap();
    assert!(
        errors_of(&folder)[0]
            .starts_with("[recordings/r/transitions.jsonl] transitions.jsonl: line 1")
    );
}

#[test]
fn a_log_line_that_is_not_an_object_is_an_error() {
    let dir = TempDir::new("check_log");
    let folder = valid_recording(&dir, RecordingSource::Cli);
    fs::write(folder.join(RECORDING_LOG_FILE), "{\"a\":1}\n[2]\n").unwrap();
    assert!(errors_of(&folder)[0].starts_with("[recordings/r/log.jsonl] log.jsonl: line 2"));
    fs::remove_file(folder.join(RECORDING_LOG_FILE)).unwrap();
    assert!(
        errors_of(&folder).is_empty(),
        "a missing log is not an error"
    );
}

#[test]
fn read_recording_refuses_what_does_not_agree() {
    let dir = TempDir::new("read_refused");
    let folder = valid_recording(&dir, RecordingSource::Cli);
    // A table with other steps than frames.jsonl.
    let mut rng = Lcg::new(SEED);
    write_table(
        &folder.join(RECORDING_KEYFRAMES_PARQUET),
        &[100, 116, 132, 148, 165].map(|s| frame(&mut rng, s, 5)),
    );
    let errors = read_recording(&folder).unwrap_err();
    assert!(errors[0].contains("does not belong to frame"), "{errors:?}");
    // A table with too few rows.
    write_table(
        &folder.join(RECORDING_KEYFRAMES_PARQUET),
        &[100, 116].map(|s| frame(&mut rng, s, 5)),
    );
    let errors = read_recording(&folder).unwrap_err();
    assert!(errors[0].contains("has 10 rows"), "{errors:?}");
    // No manifest.
    let empty = TempDir::new("read_empty");
    assert!(
        read_recording(empty.path()).unwrap_err()[0].starts_with("manifest.json: no such file")
    );
    // Optional files may be missing.
    let optional = TempDir::new("read_optional");
    let folder = valid_recording(&optional, RecordingSource::Cli);
    fs::remove_file(folder.join(RECORDING_INPUTS_FILE)).unwrap();
    fs::remove_file(folder.join(RECORDING_TRANSITIONS_FILE)).unwrap();
    let loaded = read_recording(&folder).unwrap();
    assert!(loaded.inputs.is_empty() && loaded.transitions.is_empty());
}

#[test]
fn recording_folders_are_listed_by_name_and_a_missing_folder_is_empty() {
    let dir = TempDir::new("list_dirs");
    assert!(list_recording_dirs(&dir.path().join("recordings")).is_empty());
    let recordings = dir.path().join("recordings");
    for name in ["b", "a.partial", "c"] {
        fs::create_dir_all(recordings.join(name)).unwrap();
    }
    fs::write(recordings.join("file.txt"), "not a folder").unwrap();
    let names: Vec<String> = list_recording_dirs(&recordings)
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["a.partial", "b", "c"]);
}

// ---------------------------------------------------------------------------
// the settings of the project's `recording` block
// ---------------------------------------------------------------------------

#[test]
fn the_settings_take_the_blocks_keys_and_the_defaults_for_the_rest() {
    assert_eq!(
        RecordingSettings::from_block(None),
        RecordingSettings::default()
    );
    assert_eq!(
        RecordingSettings::from_block(Some(&RecordingBlock::default())),
        RecordingSettings::default()
    );
    let block = RecordingBlock {
        auto_start_phase: Some("capture".to_string()),
        auto_stop_on_end: Some(false),
        stop_after_end_s: Some(5.0),
        keyframe_interval_steps: Some(8),
        max_duration_s: Some(60.0),
        max_size_mb: Some(2.5),
        max_tether_bodies: Some(120),
    };
    assert_eq!(
        RecordingSettings::from_block(Some(&block)),
        RecordingSettings {
            auto_start_phase: Some("capture".to_string()),
            auto_stop_on_end: false,
            stop_after_end_s: 5.0,
            keyframe_interval_steps: 8,
            max_duration_s: 60.0,
            max_size_mb: 2.5,
            max_tether_bodies: 120,
        }
    );
    let partial = RecordingBlock {
        keyframe_interval_steps: Some(4),
        ..RecordingBlock::default()
    };
    let settings = RecordingSettings::from_block(Some(&partial));
    assert_eq!(settings.keyframe_interval_steps, 4);
    assert_eq!(settings.max_duration_s, 1800.0);
    assert_eq!(settings.auto_start_phase, None);
}

// ---------------------------------------------------------------------------
// the recorder writing files in a Q13 run
// ---------------------------------------------------------------------------

/// The Q13 scenario (capture by command) for `ticks` ticks with `sink` installed;
/// `commands` as in `recording_tap::run_q13`.
fn run_q13_with(
    sink: Box<dyn FrameSink>,
    settings: RecordingSettings,
    ticks: u64,
    commands: &[(u64, RecorderCommand)],
) {
    let mut app = test_app();
    load_example_plans(&mut app);
    app.insert_resource(settings);
    app.insert_resource(RecordingSink(sink));
    q13::build_with_command(&mut app, q13::PLAN_ID);
    for _ in 0..ticks {
        let next = current_step(&app) + 1;
        for (_, command) in commands.iter().filter(|(step, _)| *step == next) {
            app.world_mut().write_message(command.clone());
        }
        tick(&mut app);
    }
    // Leave the sim screen, as a session ends.
    app.world_mut()
        .resource_mut::<bevy::state::state::NextState<crate::session::UiScreen>>()
        .set(crate::session::UiScreen::Home);
    app.update();
}

#[test]
fn a_q13_run_through_the_file_sink_matches_the_memory_sink_bit_for_bit() {
    let commands = [(400, RecorderCommand::Start { name: "q".into() })];
    let settings = RecordingSettings {
        keyframe_interval_steps: 16,
        ..RecordingSettings::default()
    };
    let memory = crate::recording::MemorySink::default();
    run_q13_with(Box::new(memory.clone()), settings.clone(), 1100, &commands);
    let segments = memory.segments();
    assert_eq!(segments.len(), 1);
    let expected = &segments[0];

    let dir = TempDir::new("q13_file_sink");
    let recordings = dir.path().join("recordings");
    let file_sink = FileSink::new(context(&recordings));
    run_q13_with(Box::new(file_sink.clone()), settings, 1100, &commands);
    let results = file_sink.results();
    assert_eq!(results.len(), 1);
    let written = results[0].as_ref().unwrap();
    assert_eq!(written.dir, recordings.join("q"));

    let loaded = read_recording(&written.dir).unwrap();
    assert_eq!(loaded.frames.len(), expected.frames.len());
    assert_eq!(
        loaded.manifest.bodies.len(),
        193,
        "the target and 192 tether bodies"
    );
    assert_eq!(
        loaded.manifest.end_reason,
        Some(RecordingEndReason::SessionEnd),
        "leaving the sim screen ends the recording"
    );
    for (read, memory) in loaded.frames.iter().zip(&expected.frames) {
        assert_eq!(read.line, FrameLine::new(memory), "step {}", memory.step);
        for (r, m) in read.bodies.iter().zip(&memory.bodies) {
            assert_eq!(bits(r), bits(m), "step {}", memory.step);
        }
    }
    // Resets inside the segment (Q13 steps 468 and 979) are in the frames' shifts.
    let shifted: Vec<u64> = loaded
        .frames
        .iter()
        .filter(|f| f.line.origin_shift_m != [0.0; 3])
        .map(|f| f.line.step)
        .collect();
    assert_eq!(shifted.len(), 2, "{shifted:?}");
    // inputs: SessionStart (k = 1) first, then the capture start.
    assert_eq!(loaded.inputs, expected.inputs);
    assert_eq!(
        loaded.inputs[0].command,
        RecordedCommand::SessionStart {
            steps_before_propagators: 1
        }
    );
    assert!(matches!(
        loaded.inputs[1].command,
        RecordedCommand::StartCapture { .. }
    ));
    // The log: the recorder's own start line is in it, in the order written.
    let log: Vec<serde_json::Value> = read_jsonl(&written.dir.join(RECORDING_LOG_FILE)).unwrap();
    let messages: Vec<&str> = log.iter().map(|l| l["message"].as_str().unwrap()).collect();
    assert_eq!(
        messages,
        expected.logs.iter().map(String::as_str).collect::<Vec<_>>()
    );
    assert!(
        messages
            .iter()
            .any(|m| m.starts_with("Recording started at step 400")),
        "{messages:?}"
    );
    let check = check_recording(&written.dir, "recordings/q");
    assert!(
        check.errors.is_empty() && check.warnings.is_empty(),
        "{check:?}"
    );
}

#[test]
fn the_size_cap_ends_a_file_recording_and_the_manifest_says_so() {
    let dir = TempDir::new("q13_size_cap");
    let recordings = dir.path().join("recordings");
    let settings = RecordingSettings {
        keyframe_interval_steps: 8,
        // Four frames of 193 bodies (22,388 bytes each) fit; the fifth does not.
        max_size_mb: 0.1,
        ..RecordingSettings::default()
    };
    let mut ctx = context(&recordings);
    ctx.settings = settings.clone();
    let file_sink = FileSink::new(ctx);
    run_q13_with(
        Box::new(file_sink.clone()),
        settings,
        200,
        &[(100, RecorderCommand::Start { name: "cap".into() })],
    );
    let loaded = read_recording(&recordings.join("cap")).unwrap();
    assert_eq!(loaded.frames.len(), 4);
    assert_eq!(
        loaded.manifest.end_reason,
        Some(RecordingEndReason::MaxSize)
    );
    assert_eq!(loaded.manifest.limits.max_size_mb, Some(0.1));
    assert_eq!(loaded.manifest.end_step, Some(124));
    let report = read_recording_file(&recordings.join("cap")).unwrap();
    assert!(report.errors.is_empty(), "{:?}", report.errors);
}

#[test]
fn the_epoch_of_a_frame_line_parses_back_to_the_frames_epoch() {
    let line = FrameLine::new(&sample_frames(2, 4)[1]);
    let parsed: Epoch = crate::project::parse_utc(&line.epoch_utc).unwrap();
    let expected = fixed_epoch() + 116.0 * PHYS_DT;
    assert!(((parsed - expected).abs()) < 1e-9, "{}", line.epoch_utc);
}
