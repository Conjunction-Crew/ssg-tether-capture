//! Tests for the export writers (`ssg_sim::project::export`, design D7) and the
//! capture's ephemeris samples (P6 S5).

use avian3d::prelude::{LinearVelocity, Position, RigidBodyDisabled};
use brahe::Epoch;
use brahe::ccsds::common::{CCSDSRefFrame, CCSDSTimeSystem};
use brahe::ccsds::oem::OEM;
use nalgebra::Vector6;

use crate::capture::EPHEMERIS_EVERY_STEPS;
use crate::orbit::cache::OrbitalCache;
use crate::physics_bubble::body_eci_state;
use crate::project::export::{
    OEM_ORIGINATOR, OemObject, TrajectoryRow, oem_text, sanitize_file_stem, write_capture_csv,
    write_log_jsonl, write_oem, write_trajectory_csv,
};
use crate::project::parse_utc;
use crate::telemetry::{DataCollection, EphemerisSample, LogEntry, LogLevel};
use crate::test_support::{Lcg, TempDir, fixed_epoch, q13, repo_root, test_app, tick};
use crate::time::{PHYS_DT, WorldTime};

fn start() -> Epoch {
    parse_utc("2026-04-03T16:03:47.002464Z").unwrap()
}

/// Two bodies, three states each, one second apart: the golden file's input.
fn golden_objects() -> Vec<OemObject<'static>> {
    let state = |k: f64, x: f64| {
        Vector6::new(
            x + 1000.0 * k,
            -5_191_853.025_4,
            15_406.009_1 + 0.5 * k,
            7_500.000_001 + k * 1e-3,
            -4_371.702_384,
            0.000_5,
        )
    };
    let states = |x: f64| -> Vec<(Epoch, Vector6<f64>)> {
        (0..3)
            .map(|k| (start() + k as f64, state(k as f64, x)))
            .collect()
    };
    vec![
        OemObject {
            name: "HST",
            id: Some("1990-037B"),
            states: states(-4_470_879.099_2),
        },
        OemObject {
            name: "P0 HST CHASER",
            id: None,
            states: states(-4_470_880.123_456_7),
        },
    ]
}

#[test]
fn oem_matches_the_golden_file() {
    let text = oem_text(start(), &golden_objects()).unwrap();
    let path = repo_root().join("crates/ssg-sim/tests/fixtures/oem/two_segments.oem");
    if std::env::var_os("SSG_UPDATE_GOLDEN").is_some() {
        std::fs::write(&path, &text).unwrap();
    }
    // A Windows checkout may turn the file's line feeds into CR LF.
    let golden = std::fs::read_to_string(&path)
        .unwrap_or_default()
        .replace("\r\n", "\n");
    assert!(
        golden == text,
        "two_segments.oem differs (regenerate with `SSG_UPDATE_GOLDEN=1 cargo test -p ssg-sim \
         oem_matches_the_golden_file` and review the diff)\n--- golden\n{golden}\n--- written\n{text}"
    );
}

#[test]
fn oem_header_and_metadata_follow_the_design() {
    let text = oem_text(start(), &golden_objects()).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], "CCSDS_OEM_VERS = 2.0");
    assert_eq!(
        lines[1], "CREATION_DATE = 2026-04-03T16:03:47.002464",
        "the run's start epoch, not the wall clock"
    );
    assert_eq!(lines[2], format!("ORIGINATOR = {OEM_ORIGINATOR}"));
    for expected in [
        "OBJECT_NAME = HST",
        "OBJECT_ID = 1990-037B",
        "OBJECT_NAME = P0 HST CHASER",
        "OBJECT_ID = P0 HST CHASER",
    ] {
        assert!(lines.contains(&expected), "{expected:?} missing:\n{text}");
    }
    for key in [
        "CENTER_NAME = EARTH",
        "REF_FRAME = GCRF",
        "TIME_SYSTEM = UTC",
        "START_TIME = 2026-04-03T16:03:47.002464",
        "STOP_TIME = 2026-04-03T16:03:49.002464",
    ] {
        assert_eq!(
            lines.iter().filter(|l| **l == key).count(),
            2,
            "{key:?} once per segment"
        );
    }
}

#[test]
fn oem_reads_back_within_a_millimetre_and_a_micrometre_per_second() {
    const SEED: u64 = 0x0E55_2026;
    let mut rng = Lcg::new(SEED);
    let mut random_states = |n: usize| -> Vec<(Epoch, Vector6<f64>)> {
        (0..n)
            .map(|k| {
                let mut v = Vector6::zeros();
                for i in 0..3 {
                    v[i] = rng.range(-7.2e6, 7.2e6);
                    v[i + 3] = rng.range(-8.0e3, 8.0e3);
                }
                (start() + k as f64, v)
            })
            .collect()
    };
    let objects = vec![
        OemObject {
            name: "TARGET",
            id: None,
            states: random_states(150),
        },
        OemObject {
            name: "CHASER",
            id: None,
            states: random_states(150),
        },
    ];
    let parsed = OEM::from_str(&oem_text(start(), &objects).unwrap()).unwrap();

    assert_eq!(parsed.header.format_version, 2.0);
    assert_eq!(parsed.segments.len(), 2);
    let (mut max_dr, mut max_dv, mut max_dt) = (0.0f64, 0.0f64, 0.0f64);
    for (object, segment) in objects.iter().zip(&parsed.segments) {
        assert_eq!(segment.metadata.object_name, object.name);
        assert_eq!(segment.metadata.ref_frame, CCSDSRefFrame::GCRF);
        assert_eq!(segment.metadata.time_system, CCSDSTimeSystem::UTC);
        assert_eq!(segment.states.len(), object.states.len());
        for (index, ((epoch, state), read)) in object.states.iter().zip(&segment.states).enumerate()
        {
            let dr = (0..3)
                .map(|i| (read.position[i] - state[i]).abs())
                .fold(0.0, f64::max);
            let dv = (0..3)
                .map(|i| (read.velocity[i] - state[i + 3]).abs())
                .fold(0.0, f64::max);
            let dt = (read.epoch - *epoch).abs();
            assert!(
                dr <= 1e-3 && dv <= 1e-6 && dt < 1e-6,
                "seed {SEED:#x}, {} state {index}: |dr| {dr:e} m, |dv| {dv:e} m/s, |dt| {dt:e} s",
                object.name
            );
            (max_dr, max_dv, max_dt) = (max_dr.max(dr), max_dv.max(dv), max_dt.max(dt));
        }
    }
    println!(
        "seed {SEED:#x}, 2 x 150 states: max |dr| {max_dr:e} m, max |dv| {max_dv:e} m/s, \
         max |dt| {max_dt:e} s"
    );
}

#[test]
fn oem_without_states_is_an_error() {
    let objects = [OemObject {
        name: "EMPTY",
        id: None,
        states: Vec::new(),
    }];
    assert_eq!(
        oem_text(start(), &objects).unwrap_err(),
        "no ephemeris samples for EMPTY"
    );
}

#[test]
fn write_oem_writes_the_text_and_reports_an_unwritable_path() {
    let dir = TempDir::new("export-oem");
    let path = dir.path().join("e.oem");
    write_oem(&path, start(), &golden_objects()).unwrap();
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        oem_text(start(), &golden_objects()).unwrap()
    );
    let missing = dir.path().join("no-such-dir/e.oem");
    let error = write_oem(&missing, start(), &golden_objects()).unwrap_err();
    assert!(
        error.starts_with(&format!("cannot write {}: ", missing.display())),
        "{error}"
    );
}

#[test]
fn trajectory_csv_has_the_d7_header_and_one_row_per_sample() {
    let dir = TempDir::new("export-trajectory");
    let path = dir.path().join("trajectory.csv");
    let rows = [
        TrajectoryRow {
            step: 66,
            epoch_utc: "2026-04-03T16:03:48.033714Z",
            phase: "rendezvous",
            range_m: 3535.3821333514456,
            rel_speed_mps: 3.8248808191455512,
        },
        TrajectoryRow {
            step: 130,
            epoch_utc: "2026-04-03T16:03:49.033714Z",
            phase: "with,comma",
            range_m: 1.0,
            rel_speed_mps: 0.0,
        },
    ];
    write_trajectory_csv(&path, rows).unwrap();
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "step,epoch_utc,phase,range_m,rel_speed_mps\n\
         66,2026-04-03T16:03:48.033714Z,rendezvous,3535.3821333514456,3.8248808191455512\n\
         130,2026-04-03T16:03:49.033714Z,\"with,comma\",1,0\n"
    );
    let missing = dir.path().join("no-such-dir/t.csv");
    assert!(write_trajectory_csv(&missing, []).is_err());
}

#[test]
fn capture_csv_has_the_d7_header_and_pairs_range_with_speed() {
    let dir = TempDir::new("export-capture");
    let path = dir.path().join("capture.csv");
    let range = [(0.015625, 3535.25), (0.03125, 3534.5), (0.046875, 3534.0)];
    let speed = [(0.015625, 3.75), (0.03125, 4.0)];
    write_capture_csv(&path, &range, &speed).unwrap();
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "seconds_since_start_s,range_m,rel_speed_mps\n\
         0.015625,3535.25,3.75\n\
         0.03125,3534.5,4.0\n",
        "as many rows as the shorter series"
    );
    let missing = dir.path().join("no-such-dir/c.csv");
    assert!(write_capture_csv(&missing, &range, &speed).is_err());
}

#[test]
fn file_stems_keep_letters_digits_dashes_and_underscores() {
    assert_eq!(sanitize_file_stem("example_plan"), "example_plan");
    assert_eq!(sanitize_file_stem("my plan/v2:é"), "my_plan_v2_é");
    assert_eq!(sanitize_file_stem("..\\x"), "___x");
    assert_eq!(sanitize_file_stem(""), "unnamed");
}

fn entry(message: &str) -> LogEntry {
    LogEntry {
        timestamp: "16:03:47".to_string(),
        epoch_utc: "2026-04-03T16:03:47.002464Z".to_string(),
        level: LogLevel::Info,
        source: "test",
        message: message.to_string(),
    }
}

#[test]
fn the_log_file_is_named_after_the_plan_id_and_has_epoch_utc_per_line() {
    let dir = TempDir::new("export-log");
    let entries = [entry("one"), entry("two")];
    let name = write_log_jsonl(
        dir.path(),
        "example plan",
        "2026-04-03 16:03:47.002 UTC",
        &entries,
    )
    .unwrap();
    assert_eq!(name, "example_plan_2026-04-03-16-03-47-002-UTC.jsonl");
    let text = std::fs::read_to_string(dir.path().join("logs").join(&name)).unwrap();
    let lines: Vec<serde_json::Value> = text
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[1]["message"], "two");
    assert_eq!(lines[0]["timestamp"], "16:03:47");
    assert_eq!(lines[0]["epoch_utc"], "2026-04-03T16:03:47.002464Z");
    // Without an active plan the name is `unnamed_…`.
    let name = write_log_jsonl(dir.path(), "", "t", &entries).unwrap();
    assert_eq!(name, "unnamed_t.jsonl");
}

#[test]
fn an_unwritable_log_directory_is_reported() {
    let dir = TempDir::new("export-log-blocked");
    // A file where `logs/` should be.
    dir.write("logs", "");
    let error = write_log_jsonl(dir.path(), "p", "t", &[entry("x")]).unwrap_err();
    assert!(
        error.starts_with("could not create logs directory: "),
        "{error}"
    );
}

/// Ticks the Q13 scenario until `steps` steps have run after `q13::build` (and
/// the step count is a multiple of [`EPHEMERIS_EVERY_STEPS`]), with or without
/// the ephemeris collection `apply_sim_commands` starts.
fn q13_with_ephemeris(collect: bool, min_ticks: u64) -> (bevy::app::App, q13::Q13Handles) {
    let mut app = test_app();
    let handles = q13::build(&mut app);
    if collect {
        app.world_mut()
            .resource_mut::<DataCollection>()
            .ephemeris
            .insert(handles.debris, Vec::new());
    }
    let step = |app: &bevy::app::App| {
        let t = app.world().resource::<WorldTime>();
        ((t.epoch - t.start_epoch) / PHYS_DT).round() as u64
    };
    let mut ticks = 0;
    while ticks < min_ticks || !step(&app).is_multiple_of(EPHEMERIS_EVERY_STEPS) {
        tick(&mut app);
        ticks += 1;
    }
    (app, handles)
}

fn eci_now(app: &bevy::app::App, entity: bevy::prelude::Entity) -> Vector6<f64> {
    let world = app.world();
    body_eci_state(
        world.resource::<OrbitalCache>(),
        entity,
        world.get::<Position>(entity).unwrap().0,
        world.get::<LinearVelocity>(entity).unwrap().0,
        world.get::<RigidBodyDisabled>(entity).is_some(),
    )
    .unwrap()
}

#[test]
fn a_running_capture_samples_the_ephemeris_every_64_steps_without_changing_the_run() {
    let (app, handles) = q13_with_ephemeris(true, 200);
    let samples: Vec<EphemerisSample> =
        app.world().resource::<DataCollection>().ephemeris[&handles.debris].clone();
    let world_time = app.world().resource::<WorldTime>();
    let steps: Vec<u64> = samples
        .iter()
        .map(|s| ((s.epoch - world_time.start_epoch) / PHYS_DT).round() as u64)
        .collect();
    assert!(steps.len() >= 3, "samples at steps {steps:?}");
    for pair in steps.windows(2) {
        assert_eq!(pair[1] - pair[0], EPHEMERIS_EVERY_STEPS, "steps {steps:?}");
    }
    assert!(
        steps
            .iter()
            .all(|s| s.is_multiple_of(EPHEMERIS_EVERY_STEPS))
    );
    assert!(
        (samples[1].epoch - samples[0].epoch - 1.0).abs() < 1e-9,
        "64 steps = 1 s"
    );
    // The last sample is this step's end state of both bodies.
    let last = samples.last().unwrap();
    assert_eq!(last.epoch, world_time.epoch);
    assert_eq!(last.target, eci_now(&app, handles.debris));
    assert_eq!(last.chaser, eci_now(&app, handles.root));

    // Read-only: the same run without the collection ends bit for bit the same.
    let (other, other_handles) = q13_with_ephemeris(false, 200);
    assert!(
        other
            .world()
            .resource::<DataCollection>()
            .ephemeris
            .is_empty(),
        "nothing is sampled without a started collection"
    );
    for (a, b) in [
        (handles.debris, other_handles.debris),
        (handles.root, other_handles.root),
    ] {
        assert_eq!(
            app.world().get::<Position>(a).unwrap().0,
            other.world().get::<Position>(b).unwrap().0
        );
        assert_eq!(
            app.world().get::<LinearVelocity>(a).unwrap().0,
            other.world().get::<LinearVelocity>(b).unwrap().0
        );
    }
}

#[test]
fn ephemeris_epochs_are_utc_and_written_as_utc() {
    // The sim's epochs come from `parse_utc` (`SimConfig`) plus whole steps.
    let epoch = fixed_epoch() + 64.0 * PHYS_DT;
    let objects = [OemObject {
        name: "X",
        id: None,
        states: vec![(epoch, Vector6::new(7e6, 0.0, 0.0, 0.0, 7.5e3, 0.0))],
    }];
    let text = oem_text(fixed_epoch(), &objects).unwrap();
    assert!(
        text.contains("\n2026-04-03T16:03:48.002464 "),
        "the data line's epoch is the UTC time of day:\n{text}"
    );
}
