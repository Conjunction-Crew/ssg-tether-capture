//! The Space Catalog panel and the orbital selection cards on egui (P8 S6): the
//! search box (F-36), paging, the sliders, and the F-59 restore.

use std::path::PathBuf;

use bevy::prelude::*;
use bevy_egui::egui;
use bevy_egui::egui::accesskit::Role;
use egui_kittest::kittest::Queryable;
use ssg_sim::orbit::catalog::{SpaceCatalogEntry, SpaceObjectCatalog, gp_record_to_elements};
use ssg_sim::orbit::selection::{
    OrbitalSelectionSource, OrbitalSelectionState, SelectedOrbitalObject,
};
use ssg_sim::test_support::{fixed_epoch, q13};

use crate::tests::egui_support::{harness_for, snapshot};
use crate::tests::sim_screen::{send, setup_view, sim_screen_app};
use crate::ui::catalog_state::{OrbitalElementField, OrbitalSelectionRole, SpaceCatalogUiState};
use crate::ui::events::UiEvent;
use crate::ui::screens::project_detail::catalog::{catalog_panel_ui, filter_catalog};
use crate::ui::screens::project_detail::orbital_selection::{
    OrbitalView, catalog_pick, custom_orbit, restore_catalog_source, slider_value_from_elements,
};
use crate::ui::screens::project_detail::sidebar::{SimScreenView, sim_sidebar_ui};

const SIZE: [f32; 2] = [364.0, 900.0];

fn entry(gpu_index: usize, norad_id: u64, name: &str) -> SpaceCatalogEntry {
    let record = q13::fixture_record(q13::HST_NORAD_ID);
    let (_, mut gp, _) = gp_record_to_elements(&record, fixed_epoch()).unwrap();
    gp.norad_id = norad_id as u32;
    SpaceCatalogEntry {
        gpu_index,
        norad_id: norad_id as u32,
        object_name: name.to_string(),
        object_id: "1990-037B".to_string(),
        search_blob: format!("{} 1990-037b {norad_id}", name.to_lowercase()),
        gp,
    }
}

fn entries() -> Vec<SpaceCatalogEntry> {
    let mut entries = vec![entry(0, 20580, "HST"), entry(1, 25544, "ISS (ZARYA)")];
    for i in 0..100 {
        entries.push(entry(i + 2, 40000 + i as u64, &format!("ONEWEB-{i:04}")));
    }
    entries
}

struct Panel {
    state: SpaceCatalogUiState,
    entries: Vec<SpaceCatalogEntry>,
    selected: Option<usize>,
}

fn panel() -> Panel {
    Panel {
        state: SpaceCatalogUiState::default(),
        entries: entries(),
        selected: None,
    }
}

/// As the app does: the results follow the search text.
fn panel_ui(ui: &mut egui::Ui, panel: &mut Panel, _out: &mut Vec<UiEvent>) {
    let filtered = filter_catalog(&panel.entries, &panel.state.search_text);
    if let Some(index) = catalog_panel_ui(ui, &mut panel.state, &panel.entries, &filtered) {
        panel.selected = Some(index);
    }
}

fn type_search<'a>(
    harness: &mut egui_kittest::Harness<'a, crate::tests::egui_support::ScreenState<Panel>>,
    text: &str,
) {
    harness.get_by_role(Role::TextInput).focus();
    harness.run();
    harness.get_by_role(Role::TextInput).type_text(text);
    harness.run();
}

#[test]
fn search_finds_by_id_and_select_all_delete_clears_it() {
    let mut harness = harness_for(panel(), SIZE, panel_ui);
    harness.get_by_label("Space Catalog");
    harness.get_by_label("102 objects loaded · showing 1-48 of 102");

    type_search(&mut harness, "20580");
    harness.get_by_label("102 objects loaded · showing 1-1 of 1");
    harness.get_by_label("HST · NORAD 20580 · 1990-037B");

    // F-36: select all and delete, which rc.3's box could not do.
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    harness.run();
    harness.key_press(egui::Key::Backspace);
    harness.run();
    assert_eq!(harness.state().view.state.search_text, "");
    harness.get_by_label("102 objects loaded · showing 1-48 of 102");
}

#[test]
fn results_are_paged_48_at_a_time() {
    let mut harness = harness_for(panel(), SIZE, panel_ui);
    harness.get_by_label("Page 1 / 3");
    harness.get_by_label("Prev").click();
    harness.run();
    harness.get_by_label("Page 1 / 3");
    harness.get_by_label("Next").click();
    harness.run();
    harness.get_by_label("Page 2 / 3");
    harness.get_by_label("102 objects loaded · showing 49-96 of 102");
    harness.get_by_label("Next").click();
    harness.run();
    harness.get_by_label("Next").click();
    harness.run();
    harness.get_by_label("Page 3 / 3");
    harness.get_by_label("Prev").click();
    harness.run();
    harness.get_by_label("Page 2 / 3");

    // A new search goes back to the first page.
    type_search(&mut harness, "oneweb");
    assert_eq!(harness.state().view.state.page, 0);
}

#[test]
fn enter_selects_the_first_result_and_unfocuses() {
    let mut harness = harness_for(panel(), SIZE, panel_ui);
    type_search(&mut harness, "25544");
    harness.key_press(egui::Key::Enter);
    harness.run();
    assert_eq!(harness.state().view.state.selected_index, Some(1));
    assert!(!harness.ctx.wants_keyboard_input());
}

#[test]
fn hotkeys_ignored_while_search_focused() {
    // `egui_wants_keyboard` reads this flag through bevy_egui's `EguiWantsInput`.
    let mut harness = harness_for(panel(), SIZE, panel_ui);
    assert!(!harness.ctx.wants_keyboard_input());
    harness.get_by_role(Role::TextInput).focus();
    harness.run();
    assert!(harness.ctx.wants_keyboard_input());
    harness.get_by_role(Role::TextInput).type_text("m,.co");
    harness.run();
    assert_eq!(harness.state().view.state.search_text, "m,.co");

    // Esc unfocuses and keeps the text (O2 S3.01 was a Diff on rc.3).
    harness.key_press(egui::Key::Escape);
    harness.run();
    assert!(!harness.ctx.wants_keyboard_input());
    assert_eq!(harness.state().view.state.search_text, "m,.co");
    assert_eq!(harness.state().view.state.selected_index, None);
}

#[test]
fn clicking_a_row_selects_it_and_clicking_again_clears_it() {
    let mut harness = harness_for(panel(), SIZE, panel_ui);
    harness
        .get_by_label("HST · NORAD 20580 · 1990-037B")
        .click();
    harness.run();
    assert_eq!(harness.state().view.state.selected_index, Some(0));
    assert_eq!(harness.state().view.selected, Some(0));
    harness
        .get_by_label("HST · NORAD 20580 · 1990-037B")
        .click();
    harness.run();
    assert_eq!(harness.state().view.state.selected_index, None);
}

#[test]
fn data_points_button_names_what_it_will_do() {
    let mut harness = harness_for(panel(), SIZE, panel_ui);
    harness.get_by_label("Show Data Points").click();
    harness.run();
    assert!(harness.state().view.state.show_points);
    harness.get_by_label("Hide Data Points");
}

#[test]
fn an_empty_catalog_says_it_is_loading() {
    let empty = Panel {
        entries: Vec::new(),
        ..panel()
    };
    let harness = harness_for(empty, SIZE, panel_ui);
    harness.get_by_label("Loading catalog...");
    harness.get_by_label("Page 0 / 0");
}

fn catalog_orbit() -> SelectedOrbitalObject {
    catalog_pick(0, &entry(0, 20580, "HST"))
}

fn orbital_view(target: Option<SelectedOrbitalObject>) -> SimScreenView {
    SimScreenView {
        orbital: OrbitalView {
            catalog_selection: Some("HST · NORAD 20580 · 1990-037B".to_string()),
            target,
            chaser: None,
        },
        ..setup_view()
    }
}

const SIDEBAR: [f32; 2] = [420.0, 1400.0];

#[test]
fn cards_show_the_source_and_their_buttons_emit_events() {
    let mut harness = harness_for(orbital_view(Some(catalog_orbit())), SIDEBAR, sim_sidebar_ui);
    harness.get_by_label("Selected catalog object: HST · NORAD 20580 · 1990-037B");
    harness.get_by_label("Target Orbit");
    harness.get_by_label("Target: Catalog: HST · NORAD 20580 · 1990-037B");
    harness.get_by_label("Chaser Orbit");
    harness.get_by_label("Chaser: No orbit selected");

    let use_selected = harness.get_all_by_label("Use Selected").next().unwrap();
    use_selected.click();
    harness.run();
    let custom = harness.get_all_by_label("Create Custom").last().unwrap();
    custom.click();
    harness.run();
    assert!(matches!(
        harness.state().events.as_slice(),
        [
            UiEvent::UseCatalogSelection(OrbitalSelectionRole::Target),
            UiEvent::CreateCustomOrbit(OrbitalSelectionRole::Chaser)
        ]
    ));
}

#[test]
fn sliders_show_only_for_a_chosen_orbit_with_rc3_values() {
    let custom = custom_orbit(OrbitalSelectionRole::Target, fixed_epoch());
    let harness = harness_for(orbital_view(Some(custom)), SIDEBAR, sim_sidebar_ui);
    for (label, value) in [
        ("Semi-major axis", "6799 km"),
        ("Eccentricity", "0.0011"),
        ("Inclination", "51.63 deg"),
        ("RAAN", "203.40 deg"),
        ("Argument of perigee", "86.69 deg"),
        ("Mean anomaly", "273.41 deg"),
    ] {
        harness.get_by_label(label);
        harness.get_by_label(value);
    }
    assert_eq!(harness.get_all_by_role(Role::Slider).count(), 6);
    assert!(
        harness.state().events.is_empty(),
        "showing a slider must not edit the orbit (F-27): {:?}",
        harness.state().events
    );

    let harness = harness_for(orbital_view(None), SIDEBAR, sim_sidebar_ui);
    assert_eq!(harness.query_all_by_role(Role::Slider).count(), 0);
}

#[test]
fn moving_a_slider_emits_the_new_value_in_the_sliders_units() {
    let custom = custom_orbit(OrbitalSelectionRole::Target, fixed_epoch());
    let mut harness = harness_for(orbital_view(Some(custom)), SIDEBAR, sim_sidebar_ui);
    harness
        .get_by_role_and_label(Role::Slider, "Mean anomaly")
        .focus();
    harness.run();
    harness.key_press(egui::Key::ArrowRight);
    harness.run();
    match harness.state().events.as_slice() {
        [
            UiEvent::SetOrbitalElement {
                role: OrbitalSelectionRole::Target,
                field: OrbitalElementField::MeanAnomaly,
                value,
            },
        ] => assert!(*value > 273.41 && *value < 360.0, "{value}"),
        events => panic!("unexpected events {events:?}"),
    }
}

#[test]
fn orbital_events_update_the_selection() {
    let mut app = sim_screen_app();
    app.insert_resource(SpaceObjectCatalog {
        entries: vec![entry(5, 20580, "HST")],
    });
    // Nothing selected in the catalog: Use Selected does nothing (rc.3).
    send(
        &mut app,
        UiEvent::UseCatalogSelection(OrbitalSelectionRole::Target),
    );
    assert!(
        app.world()
            .resource::<OrbitalSelectionState>()
            .target
            .is_none()
    );

    app.world_mut()
        .resource_mut::<SpaceCatalogUiState>()
        .selected_index = Some(0);
    send(
        &mut app,
        UiEvent::UseCatalogSelection(OrbitalSelectionRole::Target),
    );
    send(
        &mut app,
        UiEvent::CreateCustomOrbit(OrbitalSelectionRole::Chaser),
    );
    let selection = app.world().resource::<OrbitalSelectionState>();
    let target = selection.target.as_ref().unwrap();
    assert!(target.gp.is_some());
    assert!(matches!(
        &target.source,
        OrbitalSelectionSource::Catalog { gpu_index: 5, label, .. }
            if label == "HST · NORAD 20580 · 1990-037B"
    ));
    let chaser = selection.chaser.as_ref().unwrap();
    assert!(matches!(
        &chaser.source,
        OrbitalSelectionSource::Custom { label } if label == "Custom Chaser"
    ));

    // An edited element is written back and drops the catalog record (F-27).
    send(
        &mut app,
        UiEvent::SetOrbitalElement {
            role: OrbitalSelectionRole::Target,
            field: OrbitalElementField::SemiMajorAxis,
            value: 7000.0,
        },
    );
    let target = app
        .world()
        .resource::<OrbitalSelectionState>()
        .target
        .clone()
        .unwrap();
    assert!(target.gp.is_none());
    assert!((target.elements.semi_major_axis_m - 7_000_000.0).abs() < 1e-6);
    assert_eq!(
        slider_value_from_elements(&target.elements, OrbitalElementField::SemiMajorAxis),
        7000.0
    );
}

/// F-59: a restored body with its catalog record highlights again; its label
/// stays the manifest's.
#[test]
fn a_restored_catalog_body_is_a_catalog_selection_again() {
    let catalog = [entry(3, 1, "Other"), entry(9, 20580, "HST")];
    let mut restored = SelectedOrbitalObject {
        source: OrbitalSelectionSource::Custom {
            label: "Hubble".to_string(),
        },
        ..catalog_pick(1, &catalog[1])
    };
    restore_catalog_source(&mut restored, &catalog);
    assert!(matches!(
        &restored.source,
        OrbitalSelectionSource::Catalog { catalog_index: 1, gpu_index: 9, label } if label == "Hubble"
    ));
}

#[test]
fn a_restored_edited_or_unknown_body_stays_custom() {
    let catalog = [entry(9, 20580, "HST")];
    let custom = |selected: SelectedOrbitalObject| SelectedOrbitalObject {
        source: OrbitalSelectionSource::Custom {
            label: "Mine".to_string(),
        },
        ..selected
    };

    let mut edited = custom(catalog_pick(0, &catalog[0]));
    edited.gp = None;
    restore_catalog_source(&mut edited, &catalog);
    assert!(matches!(
        edited.source,
        OrbitalSelectionSource::Custom { .. }
    ));

    let mut unknown = custom(catalog_pick(0, &entry(0, 99, "Gone")));
    restore_catalog_source(&mut unknown, &catalog);
    assert!(matches!(
        unknown.source,
        OrbitalSelectionSource::Custom { .. }
    ));
}

/// The S6 PNGs for `p8_screen_snapshots`.
pub(crate) fn s6_snapshots() -> Vec<PathBuf> {
    let mut search = panel();
    search.state.search_text = "oneweb".to_string();
    search.state.selected_index = Some(3);
    vec![
        snapshot("s6_catalog_all", panel(), SIZE, panel_ui),
        snapshot("s6_catalog_search", search, SIZE, panel_ui),
        snapshot(
            "s6_orbital_none",
            orbital_view(None),
            [420.0, 900.0],
            sim_sidebar_ui,
        ),
        snapshot(
            "s6_orbital_target_chosen",
            orbital_view(Some(catalog_orbit())),
            SIDEBAR,
            sim_sidebar_ui,
        ),
    ]
}
