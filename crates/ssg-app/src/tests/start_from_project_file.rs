//! `start_from_project_file` and `restore_project_scenario` against real files.

use std::path::Path;

use bevy::ecs::message::Messages;
use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;
use ssg_sim::orbit::catalog::gp_record_to_elements;
use ssg_sim::orbit::selection::{
    EditableOrbitalElements, OrbitalSelectionState, SelectedOrbitalObject,
};
use ssg_sim::project::manifest::read_manifest_file;
use ssg_sim::telemetry::LogEvent;
use ssg_sim::test_support::{TempDir, fixed_epoch, q13};
use ssg_sim::time::SimConfig;

use crate::ui::events_handler::{restore_project_scenario, start_from_project_file};

fn catalog_pick(norad_id: u64, label: &str) -> SelectedOrbitalObject {
    let (_, gp, _) = gp_record_to_elements(&q13::fixture_record(norad_id), fixed_epoch()).unwrap();
    SelectedOrbitalObject {
        source: ssg_sim::orbit::selection::OrbitalSelectionSource::Catalog {
            catalog_index: 7,
            gpu_index: 7,
            label: label.to_string(),
        },
        elements: EditableOrbitalElements::from_gp(&gp),
        gp: Some(gp),
        mass_kg: None,
        inertia_kg_m2: None,
    }
}

/// Runs [`start_from_project_file`] on `dir`; returns what it left and the
/// messages it logged.
fn start(
    dir: &Path,
    mut config: SimConfig,
    mut selection: OrbitalSelectionState,
) -> (SimConfig, OrbitalSelectionState, Vec<String>) {
    let mut app = App::new();
    app.add_message::<LogEvent>();
    let dir = dir.to_path_buf();
    let (config, selection) = app
        .world_mut()
        .run_system_once(move |mut log: MessageWriter<LogEvent>| {
            start_from_project_file(
                &dir,
                Some("example_plan"),
                &mut config,
                &mut selection,
                &mut log,
            );
            (config.clone(), selection.clone())
        })
        .unwrap();
    let logged = app
        .world()
        .resource::<Messages<LogEvent>>()
        .iter_current_update_messages()
        .map(|event| event.message.clone())
        .collect();
    (config, selection, logged)
}

/// Runs [`restore_project_scenario`] on `dir`.
fn open(dir: &Path) -> (SimConfig, OrbitalSelectionState, Vec<String>) {
    let mut app = App::new();
    app.add_message::<LogEvent>();
    let dir = dir.to_path_buf();
    let (config, selection) = app
        .world_mut()
        .run_system_once(move |mut log: MessageWriter<LogEvent>| {
            let mut config = SimConfig {
                start_epoch: fixed_epoch(),
            };
            let mut selection = OrbitalSelectionState::default();
            restore_project_scenario(&dir, &mut config, &mut selection, &mut log);
            (config, selection)
        })
        .unwrap();
    let logged = app
        .world()
        .resource::<Messages<LogEvent>>()
        .iter_current_update_messages()
        .map(|event| event.message.clone())
        .collect();
    (config, selection, logged)
}

#[test]
fn open_restores_the_epoch_and_the_bodies_from_project_json() {
    let dir = TempDir::new("app-open");
    let selection = both_bodies();
    let epoch = ssg_sim::project::parse_utc("2026-04-03T16:03:47.002464Z").unwrap();
    start(
        dir.path(),
        SimConfig { start_epoch: epoch },
        selection.clone(),
    );

    let (config, restored, logged) = open(dir.path());

    assert!(logged.is_empty(), "{logged:?}");
    assert_eq!(config.start_epoch, epoch);
    assert_eq!(restored.target.unwrap().gp, selection.target.unwrap().gp);
    assert_eq!(restored.chaser.unwrap().gp, selection.chaser.unwrap().gp);
}

#[test]
fn open_without_project_json_starts_a_new_scenario_at_a_rounded_now() {
    let dir = TempDir::new("app-open-new");

    let (config, restored, logged) = open(dir.path());

    assert!(logged.is_empty());
    assert!(restored.target.is_none() && restored.chaser.is_none());
    assert_ne!(
        config.start_epoch,
        fixed_epoch(),
        "the wall clock, not the old epoch"
    );
    assert_eq!(
        ssg_sim::project::parse_utc(&ssg_sim::project::format_utc(config.start_epoch)),
        Some(config.start_epoch),
        "a whole number of microseconds"
    );
}

#[test]
fn open_with_an_unusable_project_json_logs_and_opens_as_new() {
    let dir = TempDir::new("app-open-bad");
    dir.write("project.json", "{ not json");

    let (config, restored, logged) = open(dir.path());

    assert_eq!(logged.len(), 1, "{logged:?}");
    assert!(
        logged[0].starts_with("project.json not used: [project.json] Invalid JSON: "),
        "{logged:?}"
    );
    assert!(restored.target.is_none());
    assert_ne!(config.start_epoch, fixed_epoch());
}

fn both_bodies() -> OrbitalSelectionState {
    OrbitalSelectionState {
        target: Some(catalog_pick(q13::HST_NORAD_ID, "HST")),
        chaser: Some(catalog_pick(q13::CHASER_NORAD_ID, "Chaser")),
    }
}

#[test]
fn start_keeps_the_recording_settings_of_project_json() {
    let dir = TempDir::new("app-start-recording");
    let selection = both_bodies();
    start(
        dir.path(),
        SimConfig {
            start_epoch: fixed_epoch(),
        },
        selection.clone(),
    );
    // The Recording window's Save put a block into the file.
    let mut manifest = read_manifest_file(dir.path()).unwrap().manifest.unwrap();
    let block = ssg_sim::project::manifest::RecordingBlock {
        auto_start_phase: Some("capture".to_string()),
        keyframe_interval_steps: Some(16),
        ..Default::default()
    };
    manifest.recording = Some(block.clone());
    ssg_sim::project::manifest::save_manifest(dir.path(), &manifest).unwrap();

    let (_, _, logged) = start(
        dir.path(),
        SimConfig {
            start_epoch: fixed_epoch(),
        },
        selection,
    );

    assert!(logged.is_empty(), "{logged:?}");
    let kept = read_manifest_file(dir.path()).unwrap().manifest.unwrap();
    assert_eq!(kept.recording, Some(block));
}

#[test]
fn start_writes_project_json_and_starts_from_what_it_read() {
    let dir = TempDir::new("app-start");
    let selection = both_bodies();

    let (config, after, logged) = start(
        dir.path(),
        SimConfig {
            start_epoch: fixed_epoch(),
        },
        selection.clone(),
    );

    assert!(logged.is_empty(), "{logged:?}");
    let written = std::fs::read_to_string(dir.path().join("project.json")).unwrap();
    assert!(
        written.contains("\"active_plan\": \"example_plan\""),
        "{written}"
    );
    assert_eq!(config.start_epoch, fixed_epoch());
    let target = after.target.unwrap();
    assert_eq!(target.gp, selection.target.as_ref().unwrap().gp);
    assert!(
        matches!(
            target.source,
            ssg_sim::orbit::selection::OrbitalSelectionSource::Catalog { gpu_index: 7, .. }
        ),
        "the selection keeps its source for the map highlight"
    );
}

#[test]
fn start_keeps_the_name_of_an_existing_project_json() {
    let dir = TempDir::new("app-start-name");
    let first = start(
        dir.path(),
        SimConfig {
            start_epoch: fixed_epoch(),
        },
        both_bodies(),
    );
    let text = std::fs::read_to_string(dir.path().join("project.json")).unwrap();
    let renamed = text.replacen(
        &format!(
            "\"name\": \"{}\"",
            dir.path().file_name().unwrap().to_string_lossy()
        ),
        "\"name\": \"Hubble study\"",
        1,
    );
    assert_ne!(
        text, renamed,
        "the folder name was the default project name"
    );
    std::fs::write(dir.path().join("project.json"), renamed).unwrap();
    drop(first);

    start(
        dir.path(),
        SimConfig {
            start_epoch: fixed_epoch(),
        },
        both_bodies(),
    );

    let text = std::fs::read_to_string(dir.path().join("project.json")).unwrap();
    assert!(text.contains("\"name\": \"Hubble study\""), "{text}");
}

#[test]
fn start_without_both_bodies_writes_nothing() {
    let dir = TempDir::new("app-start-incomplete");
    let selection = OrbitalSelectionState {
        target: Some(catalog_pick(q13::HST_NORAD_ID, "HST")),
        chaser: None,
    };

    let (_, after, logged) = start(
        dir.path(),
        SimConfig {
            start_epoch: fixed_epoch(),
        },
        selection,
    );

    assert!(logged.is_empty());
    assert!(!dir.path().join("project.json").exists());
    assert!(after.target.is_some() && after.chaser.is_none());
}

#[test]
fn start_in_a_folder_that_cannot_be_written_logs_and_keeps_the_selection() {
    let dir = TempDir::new("app-start-unwritable");
    let missing = dir.path().join("not").join("there");
    let selection = both_bodies();

    let (_, after, logged) = start(
        &missing,
        SimConfig {
            start_epoch: fixed_epoch(),
        },
        selection.clone(),
    );

    assert_eq!(logged.len(), 1, "{logged:?}");
    assert!(
        logged[0].starts_with("could not write project.json: "),
        "{logged:?}"
    );
    assert_eq!(after.target.unwrap().gp, selection.target.unwrap().gp);
}
