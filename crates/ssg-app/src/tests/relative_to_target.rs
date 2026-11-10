//! P9 S5 (E3 (a), F-47): the chaser card's Relative to Target control, the
//! selection it makes, and Start writing `relative_to_target` to `project.json`.

use std::path::PathBuf;

use bevy::ecs::message::Messages;
use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;
use egui_kittest::kittest::Queryable;
use ssg_sim::orbit::selection::{
    EditableOrbitalElements, OrbitalSelectionSource, OrbitalSelectionState, SelectedOrbitalObject,
    relative_chaser_elements,
};
use ssg_sim::project::manifest::{
    ManifestOrbit, RelativePreset, RelativeToTarget, read_manifest_file,
};
use ssg_sim::telemetry::LogEvent;
use ssg_sim::test_support::{TempDir, fixed_epoch, q13};
use ssg_sim::time::SimConfig;

use crate::tests::egui_support::{harness_for, snapshot};
use crate::tests::sim_screen::{send, setup_view, sim_screen_app};
use crate::ui::catalog_state::{OrbitalElementField, OrbitalSelectionRole};
use crate::ui::events::UiEvent;
use crate::ui::events_handler::{restore_project_scenario, start_from_project_file};
use crate::ui::form::UnitSystem;
use crate::ui::screens::project_detail::orbital_selection::{OrbitalView, place_chaser_relative};
use crate::ui::screens::project_detail::sidebar::{SimScreenView, sim_sidebar_ui};

const SIDEBAR: [f32; 2] = [420.0, 1400.0];

fn hst() -> SelectedOrbitalObject {
    q13::catalog_pick("HST", q13::HST_NORAD_ID)
}

/// An e = 0 target given as elements.
fn circular() -> SelectedOrbitalObject {
    SelectedOrbitalObject {
        source: OrbitalSelectionSource::Custom {
            label: "Circular".to_string(),
        },
        elements: EditableOrbitalElements {
            semi_major_axis_m: 6_778_137.0,
            eccentricity: 0.0,
            inclination_rad: 0.9,
            raan_rad: 1.0,
            arg_perigee_rad: 0.0,
            mean_anomaly_rad: 0.5,
            epoch: fixed_epoch(),
        },
        gp: None,
        mass_kg: None,
        inertia_kg_m2: None,
    }
}

fn view(target: Option<SelectedOrbitalObject>, unit_system: UnitSystem) -> SimScreenView {
    SimScreenView {
        orbital: OrbitalView {
            catalog_selection: None,
            target,
            chaser: None,
            unit_system,
        },
        ..setup_view()
    }
}

#[test]
fn the_chaser_card_places_the_chaser_once_a_target_is_assigned() {
    // No target: the control shows but does nothing.
    let mut harness = harness_for(view(None, UnitSystem::Metric), SIDEBAR, sim_sidebar_ui);
    harness.get_by_label("Relative to Target").click();
    harness.run();
    assert!(
        harness.state().events.is_empty(),
        "{:?}",
        harness.state().events
    );

    // A target: V-bar +30 m by default, in metres whatever the toggle shows.
    for unit_system in [UnitSystem::Metric, UnitSystem::Imperial] {
        let mut harness = harness_for(view(Some(hst()), unit_system), SIDEBAR, sim_sidebar_ui);
        assert_eq!(harness.get_all_by_label("Relative to Target").count(), 1);
        harness.get_by_label("Relative to Target").click();
        harness.run();
        match harness.state().events.as_slice() {
            [
                UiEvent::PlaceChaserRelative {
                    preset: RelativePreset::VBar,
                    offset_m,
                },
            ] => assert_eq!(*offset_m, 30.0, "{unit_system:?}"),
            events => panic!("{unit_system:?}: unexpected events {events:?}"),
        }
    }
}

#[test]
fn placing_the_chaser_sets_its_elements_and_any_other_chaser_choice_drops_it() {
    let mut app = sim_screen_app();
    {
        let mut selection = app.world_mut().resource_mut::<OrbitalSelectionState>();
        selection.target = Some(hst());
        place_chaser_relative(&mut selection, fixed_epoch(), RelativePreset::VBar, 30.0).unwrap();
        let chaser = selection.chaser.as_ref().unwrap();
        assert!(matches!(
            &chaser.source,
            OrbitalSelectionSource::Custom { label } if label == "V-bar +30 m from the target"
        ));
        let expected =
            relative_chaser_elements(&hst(), fixed_epoch(), RelativePreset::VBar, 30.0).unwrap();
        assert_eq!(chaser.elements.to_vec6(), expected.to_vec6());
        assert_eq!(
            selection.chaser_relative,
            Some(RelativeToTarget {
                preset: RelativePreset::VBar,
                offset_m: 30.0
            })
        );
    }

    // A target edit places the chaser again from the new target.
    send(
        &mut app,
        UiEvent::SetOrbitalElement {
            role: OrbitalSelectionRole::Target,
            field: OrbitalElementField::MeanAnomaly,
            value: 10.0,
        },
    );
    let selection = app.world().resource::<OrbitalSelectionState>();
    let target = selection.target.clone().unwrap();
    let expected =
        relative_chaser_elements(&target, fixed_epoch(), RelativePreset::VBar, 30.0).unwrap();
    assert_eq!(
        selection.chaser.as_ref().unwrap().elements.to_vec6(),
        expected.to_vec6()
    );
    assert!(selection.chaser_relative.is_some());

    // Create Custom on the chaser: no longer relative.
    send(
        &mut app,
        UiEvent::CreateCustomOrbit(OrbitalSelectionRole::Chaser),
    );
    assert!(
        app.world()
            .resource::<OrbitalSelectionState>()
            .chaser_relative
            .is_none()
    );
}

#[test]
fn a_placement_the_sim_refuses_changes_nothing() {
    let mut selection = OrbitalSelectionState::default();
    assert_eq!(
        place_chaser_relative(&mut selection, fixed_epoch(), RelativePreset::VBar, 30.0),
        Err("Relative to Target needs a target orbit".to_string())
    );
    selection.target = Some(circular());
    assert_eq!(
        place_chaser_relative(&mut selection, fixed_epoch(), RelativePreset::VBar, 0.0),
        Err("Relative to Target needs an offset other than 0".to_string())
    );
    // A degenerate target (its state is not a number) fails the 1 mm start guard.
    let mut degenerate = circular();
    degenerate.elements.semi_major_axis_m = f64::NAN;
    selection.target = Some(degenerate);
    let refused = place_chaser_relative(&mut selection, fixed_epoch(), RelativePreset::RBar, 30.0)
        .unwrap_err();
    assert!(refused.starts_with("the chaser would start "), "{refused}");
    assert!(selection.chaser.is_none() && selection.chaser_relative.is_none());
}

/// Runs [`start_from_project_file`], then [`restore_project_scenario`], on `dir`.
fn start_then_open(
    dir: PathBuf,
    selection: OrbitalSelectionState,
) -> (OrbitalSelectionState, OrbitalSelectionState, Vec<String>) {
    let mut app = App::new();
    app.add_message::<LogEvent>();
    let (started, opened) = app
        .world_mut()
        .run_system_once(move |mut log: MessageWriter<LogEvent>| {
            let mut config = SimConfig {
                start_epoch: fixed_epoch(),
            };
            let mut started = selection.clone();
            start_from_project_file(
                &dir,
                Some("example_plan"),
                &mut config,
                &mut started,
                &mut log,
            );
            let mut opened = OrbitalSelectionState::default();
            restore_project_scenario(&dir, &mut config, &mut opened, &mut log);
            (started, opened)
        })
        .unwrap();
    let logged = app
        .world()
        .resource::<Messages<LogEvent>>()
        .iter_current_update_messages()
        .map(|event| event.message.clone())
        .collect();
    (started, opened, logged)
}

#[test]
fn start_writes_relative_to_target_and_open_restores_it() {
    let dir = TempDir::new("app-relative");
    let mut selection = OrbitalSelectionState {
        target: Some(hst()),
        ..Default::default()
    };
    place_chaser_relative(&mut selection, fixed_epoch(), RelativePreset::RBar, -50.0).unwrap();
    let relative = selection.chaser_relative.clone();

    let (started, opened, logged) = start_then_open(dir.path().to_path_buf(), selection.clone());

    assert!(logged.is_empty(), "{logged:?}");
    let manifest = read_manifest_file(dir.path()).unwrap().manifest.unwrap();
    assert_eq!(
        manifest.chaser.orbit,
        ManifestOrbit::RelativeToTarget(relative.clone().unwrap())
    );
    for (what, state) in [("started", &started), ("opened", &opened)] {
        assert_eq!(state.chaser_relative, relative, "{what}");
        assert_eq!(
            state.chaser.as_ref().unwrap().elements.to_vec6(),
            selection.chaser.as_ref().unwrap().elements.to_vec6(),
            "{what}"
        );
    }
}

/// Rule 28: the chaser card with the Relative to Target row, with and without a
/// target.
pub(crate) fn p9_s5_snapshots() -> Vec<PathBuf> {
    vec![
        snapshot(
            "p9_chaser_relative_no_target",
            view(None, UnitSystem::Metric),
            SIDEBAR,
            sim_sidebar_ui,
        ),
        snapshot(
            "p9_chaser_relative_target",
            view(Some(hst()), UnitSystem::Imperial),
            SIDEBAR,
            sim_sidebar_ui,
        ),
    ]
}
