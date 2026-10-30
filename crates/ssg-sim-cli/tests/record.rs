//! `ssg-sim run --record NAME` (P7 S3, R3, R4): a recording is a read-only tap, so
//! the run's state hash does not change; the recording folder holds the layout of
//! the design; the project's `recording` block and `--record-from-phase` choose when
//! it starts and ends. Platform-independent: runs are compared with each other; the
//! macOS reference hash is asserted on macOS/aarch64 only.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{Value, json};
use ssg_sim::project::recording::{
    RecordingEndReason, RecordingSource, RecordingStartTrigger, read_recording_file,
};
use ssg_sim::recording::files::{check_recording, read_jsonl, read_recording};
use ssg_sim::test_support::q13;

/// The Q13 fixture's GP epoch, as in the harness (`fixed_epoch()`).
const EPOCH: &str = "2026-04-03T16:03:47.002464";

/// `ssg-sim run` of the Q13 6,400-step command (macOS/aarch64 reference).
const Q13_6400_HASH: &str = "990794b3425a84be";

/// The files of a recording folder.
const FILES: [&str; 6] = [
    "manifest.json",
    "inputs.jsonl",
    "keyframes.parquet",
    "frames.jsonl",
    "transitions.jsonl",
    "log.jsonl",
];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn temp_project(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("ssg-sim-cli-record-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A project holding the Q13 fixture as `datasets/q13.json` (the flag run).
fn flag_project(name: &str) -> PathBuf {
    let dir = temp_project(name);
    std::fs::create_dir_all(dir.join("datasets")).unwrap();
    std::fs::copy(
        repo_root().join("crates/ssg-sim/src/test_support/fixtures/q13_hubble_pair.json"),
        dir.join("datasets/q13.json"),
    )
    .unwrap();
    dir
}

/// A project that holds the Q13 scenario in `project.json`, with `recording` as
/// its `recording` block.
fn manifest_project(name: &str, recording: Value) -> PathBuf {
    let dir = temp_project(name);
    let record = |id| serde_json::to_value(q13::fixture_record(id)).unwrap();
    let manifest = json!({
        "schema_version": 1,
        "name": "Q13 study",
        "start_epoch_utc": format!("{EPOCH}Z"),
        "active_plan": "example_plan",
        "target": { "label": "HST", "orbit": { "gp": record(q13::HST_NORAD_ID) } },
        "chaser": { "label": "P0 HST CHASER", "orbit": { "gp": record(q13::CHASER_NORAD_ID) } },
        "recording": recording,
    });
    std::fs::write(
        dir.join("project.json"),
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();
    dir
}

/// `ssg-sim run <project> <extra> --out <project>/<out>`.
fn run(project: &Path, out: &str, extra: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ssg-sim"))
        .arg("run")
        .arg(project)
        .args(extra)
        .arg("--assets")
        .arg(repo_root().join("assets"))
        .arg("--out")
        .arg(project.join(out))
        .output()
        .expect("ssg-sim runs")
}

/// The flag run of the Q13 scenario for `steps` steps.
fn q13_flags(steps: &str) -> [&str; 10] {
    [
        "--target",
        "20580",
        "--chaser",
        "99901",
        "--epoch",
        EPOCH,
        "--steps",
        steps,
        "--sample-every",
        "64",
    ]
}

#[track_caller]
fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "exit {:?}: {}",
        output.status.code(),
        stderr(output)
    );
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).replace("\r\n", "\n")
}

fn summary(project: &Path, out: &str) -> Value {
    serde_json::from_str(&std::fs::read_to_string(project.join(out).join("summary.json")).unwrap())
        .unwrap()
}

fn text(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap().replace("\r\n", "\n")
}

#[test]
fn recording_does_not_change_the_run() {
    // Steps 468 and 979 are floating-origin resets, 4318 the first transition:
    // 1,100 steps cover the resets with the recording running.
    let project = flag_project("hash");
    assert_success(&run(&project, "plain", &q13_flags("1100")));
    let mut args = q13_flags("1100").to_vec();
    args.extend(["--record", "r"]);
    assert_success(&run(&project, "recorded", &args));
    let (plain, recorded) = (summary(&project, "plain"), summary(&project, "recorded"));
    assert_eq!(
        plain["state_hash"], recorded["state_hash"],
        "a recording is a read-only tap"
    );
    for key in ["transitions", "final", "steps"] {
        assert_eq!(plain[key], recorded[key], "{key}");
    }
}

#[test]
fn a_6400_step_recording_has_the_design_layout_and_keeps_the_reference_hash() {
    let block = json!({ "keyframe_interval_steps": 16 });
    let project = manifest_project("layout", block);
    let recorded = run(&project, "recorded", &["--steps", "6400", "--record", "r"]);
    assert_success(&recorded);
    let recorded_summary = summary(&project, "recorded");
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        assert_eq!(recorded_summary["state_hash"], Q13_6400_HASH);
    }
    let message = stderr(&recorded);
    assert!(
        message.contains("recorded ") && message.contains("401 frames of 193 bodies"),
        "{message}"
    );

    let folder = project.join("recordings/r");
    assert!(!project.join("recordings/r.partial").exists());
    for file in FILES {
        assert!(folder.join(file).is_file(), "{file}");
    }
    // inputs.jsonl starts with SessionStart k = 1 (the CLI always runs one step
    // before the propagators), then the capture command.
    let inputs = text(&folder.join("inputs.jsonl"));
    let mut lines = inputs.lines();
    assert_eq!(
        lines.next().unwrap(),
        r#"{"step":1,"command":{"session_start":{"steps_before_propagators":1}}}"#
    );
    assert!(
        lines
            .next()
            .unwrap()
            .starts_with(r#"{"step":2,"command":{"start_capture":{"target":"target""#)
    );
    // The manifest.
    let manifest = read_recording_file(&folder).unwrap().manifest.unwrap();
    assert_eq!(manifest.source, Some(RecordingSource::Cli));
    assert_eq!(manifest.start_trigger, Some(RecordingStartTrigger::Manual));
    assert_eq!(manifest.end_reason, Some(RecordingEndReason::SessionEnd));
    assert_eq!(
        manifest.end_step,
        recorded_summary["final"]["step"].as_u64()
    );
    assert_eq!(manifest.start_step, 2);
    assert_eq!(manifest.bodies.len(), 193);
    assert_eq!(manifest.keyframe_interval_steps, 16);
    // Its project gives the run's inputs back (the dataset is not needed).
    let inputs = manifest.project.as_ref().unwrap().sim_inputs().unwrap();
    let fixture = q13::fixture_record(q13::HST_NORAD_ID);
    assert_eq!(
        inputs.target.gp.unwrap().mean_motion_rev_per_day.to_bits(),
        fixture.mean_motion.unwrap().as_f64().unwrap().to_bits()
    );
    assert_eq!(
        inputs.start_epoch,
        ssg_sim::project::parse_utc(EPOCH).unwrap()
    );
    // Rows and size: 116 bytes per body per frame.
    let loaded = read_recording(&folder).unwrap();
    assert_eq!(loaded.frames.len(), 401);
    let rows = 193 * loaded.frames.len() as u64;
    let size = std::fs::metadata(folder.join("keyframes.parquet"))
        .unwrap()
        .len();
    println!("6,400-step recording at K = 16:");
    for file in FILES {
        println!(
            "  {file}: {} bytes",
            std::fs::metadata(folder.join(file)).unwrap().len()
        );
    }
    assert!(
        size >= rows * 116 && size < rows * 116 * 101 / 100,
        "{size} bytes for {rows} rows of 116 bytes"
    );
    // The transition of the run is in the recording.
    assert_eq!(loaded.transitions.len(), 1);
    assert_eq!(
        (
            loaded.transitions[0].step,
            loaded.transitions[0].to.as_str()
        ),
        (4318, "approach")
    );
    // The folder passes the checks.
    let check = check_recording(&folder, "recordings/r");
    assert!(
        check.errors.is_empty() && check.warnings.is_empty(),
        "{check:?}"
    );
    println!("{}", check.summary.unwrap());
}

#[test]
fn a_phase_and_the_projects_block_set_the_start_the_cadence_and_the_cap() {
    // The flag overrides the block's phase; the block gives K and a 1 s cap.
    let block = json!({
        "auto_start_phase": "capture", "keyframe_interval_steps": 64, "max_duration_s": 1.0
    });
    let project = manifest_project("phase", block.clone());
    let args = [
        "--steps",
        "4400",
        "--record",
        "p",
        "--record-from-phase",
        "approach",
    ];
    assert_success(&run(&project, "out", &args));

    let folder = project.join("recordings/p");
    let loaded = read_recording(&folder).unwrap();
    let manifest = &loaded.manifest;
    assert_eq!(
        manifest.start_step, 4318,
        "the step of rendezvous → approach"
    );
    assert_eq!(manifest.start_trigger, Some(RecordingStartTrigger::Phase));
    assert_eq!(manifest.start_phase.as_deref(), Some("approach"));
    assert_eq!(manifest.keyframe_interval_steps, 64);
    assert_eq!(manifest.end_reason, Some(RecordingEndReason::MaxDuration));
    assert_eq!(manifest.limits.max_duration_s, 1.0);
    // 1 s = 64 steps: a frame at the start and one at the cap.
    assert_eq!(
        loaded
            .frames
            .iter()
            .map(|f| f.line.step)
            .collect::<Vec<_>>(),
        [4318, 4382]
    );
    assert_eq!(
        loaded
            .transitions
            .iter()
            .map(|t| t.step)
            .collect::<Vec<_>>(),
        [4318]
    );
    assert_eq!(
        loaded.inputs.first().map(|i| i.after_step),
        Some(1),
        "the input log is kept from step 0, not from the recording's start"
    );
    // The project the recording carries has the block, so a re-run starts alike.
    let project_copy = serde_json::to_value(manifest.project.as_ref().unwrap()).unwrap();
    assert_eq!(project_copy["recording"], block);
    // The segment's log: the transition line of the start step, then the recorder's.
    let log: Vec<Value> = read_jsonl(&folder.join("log.jsonl")).unwrap();
    let messages: Vec<&str> = log.iter().map(|l| l["message"].as_str().unwrap()).collect();
    assert!(
        messages[0].starts_with("State: rendezvous → approach"),
        "{messages:?}"
    );
    assert!(
        messages
            .iter()
            .any(|m| m.starts_with("Recording started at step 4318")),
        "{messages:?}"
    );
}

#[test]
fn the_blocks_phase_alone_starts_the_recording() {
    let block = json!({ "auto_start_phase": "rendezvous", "keyframe_interval_steps": 64 });
    let project = manifest_project("block", block);
    assert_success(&run(&project, "out", &["--steps", "200", "--record", "b"]));
    let loaded = read_recording(&project.join("recordings/b")).unwrap();
    assert_eq!(loaded.manifest.start_phase.as_deref(), Some("rendezvous"));
    assert_eq!(
        loaded.manifest.start_step, 2,
        "the first phase starts at the capture command"
    );
    assert_eq!(
        loaded
            .frames
            .iter()
            .map(|f| f.line.step)
            .collect::<Vec<_>>(),
        [2, 66, 130, 194, 202]
    );
}

#[test]
fn a_phase_that_is_never_entered_makes_no_recording_and_says_so() {
    let project = flag_project("never");
    let mut args = q13_flags("300").to_vec();
    args.extend(["--record", "n", "--record-from-phase", "capture"]);
    let output = run(&project, "out", &args);
    assert_success(&output);
    assert!(!project.join("recordings").exists());
    let message = stderr(&output);
    assert!(
        !message.contains("no phase `capture`"),
        "the example plan has a `capture` phase: {message}"
    );
    assert!(
        message
            .contains("warning: no recording was made: the capture never entered phase `capture`"),
        "{message}"
    );

    // A phase the plan does not have is warned about before the run.
    let mut args = q13_flags("300").to_vec();
    args.extend(["--record", "n", "--record-from-phase", "nope"]);
    let output = run(&project, "out2", &args);
    assert_success(&output);
    let message = stderr(&output);
    assert!(
        message.contains("--record-from-phase: no phase `nope` in plan `example_plan`"),
        "{message}"
    );

    // So is a block that names one.
    let project = manifest_project("never_block", json!({ "auto_start_phase": "nope" }));
    let output = run(&project, "out", &["--steps", "300", "--record", "n"]);
    assert_success(&output);
    assert!(
        stderr(&output).contains(
            "warning: [project.json] /recording/auto_start_phase: no phase `nope` in plan `example_plan`"
        ),
        "{}",
        stderr(&output)
    );
}

#[test]
fn an_existing_recording_is_not_overwritten() {
    let project = flag_project("exists");
    let mut args = q13_flags("200").to_vec();
    args.extend(["--record", "r"]);
    assert_success(&run(&project, "out", &args));
    let before = text(&project.join("recordings/r/frames.jsonl"));
    let output = run(&project, "out2", &args);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr(&output).contains("--record: recording `r` already exists"),
        "{}",
        stderr(&output)
    );
    assert!(
        !project.join("out2").exists(),
        "the run did not start: {}",
        stderr(&output)
    );
    assert_eq!(text(&project.join("recordings/r/frames.jsonl")), before);
    // An unfinished one of that name is in the way too.
    std::fs::create_dir_all(project.join("recordings/p.partial")).unwrap();
    let mut args = q13_flags("200").to_vec();
    args.extend(["--record", "p"]);
    assert_eq!(run(&project, "out3", &args).status.code(), Some(1));
}

#[test]
fn bad_record_flags_are_usage_errors() {
    let project = flag_project("usage");
    for (extra, message) in [
        (
            vec!["--record-from-phase", "approach"],
            "--record-from-phase needs --record",
        ),
        (
            vec!["--record", "a/b"],
            "--record: `a/b` is not a name (use letters, digits, `-` and `_`)",
        ),
        (vec!["--record", ""], "is not a name"),
    ] {
        let mut args = q13_flags("200").to_vec();
        args.extend(extra.iter().copied());
        let output = run(&project, "out", &args);
        assert_eq!(output.status.code(), Some(2), "{extra:?}");
        assert!(
            stderr(&output).contains(message),
            "{extra:?}: {}",
            stderr(&output)
        );
    }
}

#[test]
fn a_recording_that_cannot_be_written_fails_the_run_after_its_outputs() {
    let project = flag_project("unwritable");
    // `recordings` is a file, so no folder can be made under it.
    std::fs::write(project.join("recordings"), "in the way").unwrap();
    let mut args = q13_flags("200").to_vec();
    args.extend(["--record", "r"]);
    let output = run(&project, "out", &args);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr(&output).contains("ssg-sim: recording failed: cannot create "),
        "{}",
        stderr(&output)
    );
    assert!(
        project.join("out/summary.json").is_file(),
        "the run's own outputs are written"
    );
}

#[test]
fn a_now_run_is_recorded_from_a_microsecond_epoch() {
    let project = flag_project("now");
    let output = run(
        &project,
        "out",
        &[
            "--target", "20580", "--chaser", "99901", "--epoch", "now", "--steps", "100",
            "--record", "n",
        ],
    );
    assert_success(&output);
    let manifest = read_recording_file(&project.join("recordings/n"))
        .unwrap()
        .manifest
        .unwrap();
    let start = manifest.project.unwrap().start_epoch_utc;
    assert_eq!(
        ssg_sim::project::format_utc(ssg_sim::project::parse_utc(&start).unwrap()),
        start,
        "the project's start epoch is what a replay reads back exactly"
    );
}
