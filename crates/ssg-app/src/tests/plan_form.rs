//! The capture-plan form on egui (P8 S4; P9 S1 on the v1 plan): its controls →
//! `UiEvent`, text typed into the numeric fields and validated at Save (P0-001)
//! with each message under its field, the overwrite prompt's Cancel (P0-002),
//! "Phase" wording (A-3), every phase of a plan shown (E1), and the F-15 round
//! trip: an unchanged form gives the plan back, in metric and through the toggle.

use std::path::{Path, PathBuf};

use bevy_egui::egui::accesskit::Role;
use egui_kittest::kittest::{By, Queryable};
use ssg_sim::capture::plan::{CapturePlan, CapturePlanLibrary};
use ssg_sim::project::load_plans_from_dir_with_errors;
use ssg_sim::project::plan_files::plan_to_json;
use ssg_sim::test_support::TempDir;

use crate::tests::egui_support::harness_for;
use crate::ui::egui_theme::visuals_from_theme;
use crate::ui::events::UiEvent;
use crate::ui::form::{FormError, NewCapturePlanForm, UnitSystem};
use crate::ui::screens::capture_plan::form::{
    generate_filename, load_plan_into_form, new_plan_into_form, plan_from_form, save_plan_from_form,
};
use crate::ui::screens::capture_plan::window::{capture_plan_form_ui, form_fill};
use crate::ui::theme::UiTheme;

/// The shipped example plans' folder.
pub(crate) fn example_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/example_capture_plans")
}

/// The example plan, as the library loads it.
pub(crate) fn example_plan() -> CapturePlan {
    load_plans_from_dir_with_errors(&example_dir())
        .0
        .remove("example_plan")
        .expect("the example plan loads")
}

/// A new form as **+ New Plan** opens it: the example plan's phases, no name (A-2).
pub(crate) fn new_form() -> NewCapturePlanForm {
    let mut form = NewCapturePlanForm::default();
    new_plan_into_form(&mut form, Some(&example_plan()));
    form.open = true;
    form
}

/// A new form with a name: ready to save.
pub(crate) fn filled_form() -> NewCapturePlanForm {
    let mut form = new_form();
    form.plan_name = "P0 Created".to_string();
    form
}

const SIZE: [f32; 2] = [720.0, 4000.0];

fn error(pointer: &str, message: &str) -> FormError {
    FormError {
        pointer: pointer.to_string(),
        message: message.to_string(),
    }
}

#[test]
fn form_fill_is_the_panel_colour_without_its_alpha() {
    let visuals = visuals_from_theme(&UiTheme::default());
    assert!(
        visuals.panel_fill.a() < 255,
        "the theme's panel is see-through"
    );
    let fill = form_fill(&visuals);
    assert_eq!(fill.a(), 255);
    assert_eq!(fill, visuals.panel_fill.to_opaque());
}

/// Every phase of the plan has a section titled with its id, in file order
/// (E1); the conditions of each transition are listed (Q10 row 4).
#[test]
fn form_sections_say_phase() {
    let harness = harness_for(filled_form(), SIZE, capture_plan_form_ui);
    for label in [
        "New Capture Plan",
        "Save",
        "× Cancel",
        "Units",
        "m (metric)",
        "ft (imperial)",
        "General",
        "Tether Type",
        "Net (coming soon)",
        "Tether Length (m)",
        "Phase · rendezvous",
        "Phase · approach",
        "Phase · final_approach",
        "Phase · terminal",
        "Phase · capture",
        "Phase · hold",
        "Relative Speed less than (m/s)",
        "Time in Phase greater than (s)",
    ] {
        harness.get_by_label(label);
    }
    // One per transition: rendezvous, approach, final_approach, terminal ×2, capture.
    for label in [
        "To Phase",
        "Conditions (all must hold)",
        "Distance less than (m)",
    ] {
        let count = harness.get_all_by_label(label).count();
        assert!(count >= 5, "{label}: {count}");
    }
    assert_eq!(harness.get_all_by_label("Transition 1").count(), 5);
    assert_eq!(harness.get_all_by_label("Transition 2").count(), 1);
    // The section title and the tether-type button.
    assert_eq!(harness.get_all_by_label("Tether").count(), 2);
    assert!(harness.query_by_label_contains("State").is_none());
}

/// The sections come in the plan's order.
#[test]
fn phase_sections_follow_the_file_order() {
    let form = filled_form();
    let ids: Vec<&str> = form.phases.iter().map(|p| p.id.as_str()).collect();
    let source = example_plan();
    let source_ids: Vec<&str> = source.phases.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids, source_ids);
}

#[test]
fn form_buttons_emit_ui_events() {
    let mut harness = harness_for(filled_form(), SIZE, capture_plan_form_ui);
    harness.get_by_label("ft (imperial)").click();
    harness.run();
    harness.get_all_by_label("×").next().unwrap().click();
    harness.run();
    for label in ["Save", "× Cancel"] {
        harness.get_by_label(label).click();
        harness.run();
    }
    for add in harness
        .get_all_by_label("+ Add Transition")
        .collect::<Vec<_>>()
    {
        add.click();
    }
    harness.run();
    let events = &harness.state().events;
    assert!(matches!(
        events[0],
        UiEvent::SetUnitSystem(UnitSystem::Imperial)
    ));
    assert!(matches!(
        events[1],
        UiEvent::RemoveTransition { phase: 0, index: 0 }
    ));
    assert!(matches!(events[2], UiEvent::SaveCapturePlan));
    assert!(matches!(events[3], UiEvent::CloseNewCapturePlanForm));
    for (phase, event) in events[4..].iter().enumerate() {
        assert!(
            matches!(event, UiEvent::AddTransition(p) if *p == phase),
            "{event:?}"
        );
    }
    assert_eq!(events.len(), 4 + 6);
}

/// A blank name: the message comes from `validate_capture_plan` and is shown
/// under the field it names.
#[test]
fn blank_save_shows_the_error_at_the_field() {
    let mut harness = harness_for(new_form(), SIZE, capture_plan_form_ui);
    harness.get_by_label("Save").click();
    harness.run();
    assert!(matches!(
        harness.state().events.as_slice(),
        [UiEvent::SaveCapturePlan]
    ));

    // The handler validates on Save (P0-001).
    let errors = plan_from_form(&harness.state().view).unwrap_err();
    assert_eq!(errors, [error("/name", "must not be blank")]);
    harness.state_mut().view.validation_errors = errors;
    harness.run();
    harness.get_by_label("Please fix the fields marked below.");
    harness.get_by_label("must not be blank");
}

/// A message about a value the form has no field for goes in the list.
#[test]
fn an_error_without_a_field_is_listed() {
    let mut form = filled_form();
    form.validation_errors = vec![error("/capture_sphere/min_radius_m", "must be less")];
    let harness = harness_for(form, SIZE, capture_plan_form_ui);
    harness.get_by_label("Please fix the following errors:");
    harness.get_by_label("• /capture_sphere/min_radius_m: must be less");
}

#[test]
fn letters_typed_then_save_says_must_be_a_number() {
    let mut form = filled_form();
    form.phases[0].max_velocity.text.clear();
    let mut harness = harness_for(form, SIZE, capture_plan_form_ui);
    let field = By::new()
        .role(Role::TextInput)
        .label("Max Velocity * (m/s)");
    harness.get_all(field.clone()).next().unwrap().focus();
    harness.run();
    harness
        .get_all(field.clone())
        .next()
        .unwrap()
        .type_text("abc");
    harness.run();
    // Letters are accepted (P0-001): the field holds what was typed.
    assert_eq!(harness.state().view.phases[0].max_velocity.text, "abc");

    let errors = plan_from_form(&harness.state().view).unwrap_err();
    assert_eq!(
        errors,
        [error("/phases/0/max_velocity_mps", "must be a number")]
    );
    harness.state_mut().view.validation_errors = errors;
    harness.run();
    harness.get_by_label("must be a number");
}

/// Save of `filled_form()` after `edit` reports `expected` (with its pointer).
fn save_reports(edit: impl Fn(&mut NewCapturePlanForm), pointer: &str, message: &str) {
    let mut form = filled_form();
    edit(&mut form);
    let errors = plan_from_form(&form).unwrap_err();
    assert!(errors.contains(&error(pointer, message)), "{errors:?}");
}

#[test]
fn save_errors_on_empty_tether_name() {
    save_reports(|f| f.tether_name.clear(), "/tether", "must not be blank");
}

#[test]
fn save_errors_on_zero_tether_length() {
    save_reports(
        |f| f.tether_length.text = "0.0".to_string(),
        "/device/tether_length_m",
        "must be greater than 0",
    );
}

#[test]
fn save_errors_on_negative_tether_length() {
    save_reports(
        |f| f.tether_length.text = "-5.0".to_string(),
        "/device/tether_length_m",
        "must be greater than 0",
    );
}

#[test]
fn save_errors_on_non_numeric_tether_length() {
    save_reports(
        |f| f.tether_length.text = "abc".to_string(),
        "/device/tether_length_m",
        "must be a number",
    );
}

#[test]
fn save_errors_on_empty_max_force() {
    save_reports(
        |f| f.phases[1].max_force.text.clear(),
        "/phases/1/max_force_n",
        "is required",
    );
}

#[test]
fn save_errors_on_empty_transition_to() {
    save_reports(
        |f| f.phases[1].transitions[0].to.clear(),
        "/phases/1/transitions/0/to",
        "unknown phase ''",
    );
}

#[test]
fn save_errors_on_non_numeric_transition_bound() {
    save_reports(
        |f| f.phases[1].transitions[0].conditions[0].less_than.text = "far".to_string(),
        "/phases/1/transitions/0/all_of/distance_m/less_than",
        "must be a number",
    );
}

/// Both bounds emptied: the condition is gone, and with it the last one.
#[test]
fn save_errors_on_a_transition_without_conditions() {
    save_reports(
        |f| {
            f.phases[1].transitions[0].conditions[0]
                .less_than
                .text
                .clear()
        },
        "/phases/1/transitions/0/all_of",
        "at least one condition is required",
    );
}

/// + Add Transition gives one with no phase and an empty distance condition.
#[test]
fn save_errors_on_an_added_empty_transition() {
    save_reports(
        |f| f.phases[3].transitions.push(Default::default()),
        "/phases/3/transitions/2/to",
        "unknown phase ''",
    );
    save_reports(
        |f| f.phases[3].transitions.push(Default::default()),
        "/phases/3/transitions/2/all_of",
        "at least one condition is required",
    );
}

#[test]
fn save_errors_on_a_transition_of_an_end_phase() {
    save_reports(
        |f| f.phases[5].transitions.push(Default::default()),
        "/phases/5/transitions",
        "an end phase has no transitions",
    );
}

#[test]
fn transition_dropdown_sets_the_phase() {
    let mut form = filled_form();
    form.phases[0].transitions[0].to.clear();
    let mut harness = harness_for(form, SIZE, capture_plan_form_ui);
    harness
        .get_all(By::new().role(Role::ComboBox))
        .find(|combo| combo.value().as_deref() == Some("—"))
        .expect("the empty To Phase combo box")
        .click();
    harness.run();
    harness.get_by_label("rendezvous").click();
    harness.run();
    assert_eq!(
        harness.state().view.phases[0].transitions[0].to,
        "rendezvous"
    );
}

#[test]
fn overwrite_prompt_offers_overwrite_and_cancel() {
    let mut form = filled_form();
    form.overwrite_conflict_path = Some("/Users/walk/wd/p0_created.json".to_string());
    let mut harness = harness_for(form, SIZE, capture_plan_form_ui);
    harness.get_by_label("Overwrite Existing Plan?");
    harness.get_by_label("A plan already exists at:");
    harness.get_by_label("/Users/walk/wd/p0_created.json");
    assert!(harness.query_by_label("Keep Original Name").is_none());

    harness.get_by_label("Cancel").click();
    harness.run();
    harness.get_by_label("Overwrite").click();
    harness.run();
    assert!(matches!(
        harness.state().events.as_slice(),
        [
            UiEvent::CancelOverwriteCapturePlan,
            UiEvent::ConfirmOverwriteCapturePlan
        ]
    ));
}

/// The example plan in View: no Save, nothing editable, no transition controls.
#[test]
fn example_plan_opens_read_only() {
    let mut form = NewCapturePlanForm::default();
    load_plan_into_form(&mut form, "example_plan", &example_plan(), true);
    form.open = true;
    let mut harness = harness_for(form, SIZE, capture_plan_form_ui);
    harness.get_by_label("View Capture Plan");
    assert!(harness.query_by_label("Save").is_none());

    let name = By::new().role(Role::TextInput).label("Plan Name *");
    harness.get_all(name.clone()).next().unwrap().focus();
    harness.run();
    harness.get_all(name).next().unwrap().type_text("x");
    harness.run();
    assert_eq!(harness.state().view.plan_name, "example_plan");
    harness
        .get_all_by_label("+ Add Transition")
        .next()
        .unwrap()
        .click();
    harness.get_all_by_label("×").next().unwrap().click();
    harness.run();
    assert!(harness.state().events.is_empty());
}

/// + New Plan: the example plan's six phases and values, no name, a new file (A-2).
#[test]
fn new_plan_starts_from_the_example_plan() {
    let form = new_form();
    assert_eq!(form.plan_name, "");
    assert_eq!(form.editing_plan_id, None);
    assert!(!form.read_only);
    let mut expected = example_plan();
    expected.name = "P0 Created".to_string();
    expected.id = String::new();
    assert_eq!(plan_from_form(&filled_form()), Ok(expected));
}

// ── F-15 round trip (P9 S1) ──────────────────────────────────────────────────

/// How the round trip shows the plan.
#[derive(Debug, Clone, Copy)]
enum Units {
    Metric,
    /// Opened in metric, toggled to imperial and back.
    MetricImperialMetric,
}

/// The three plans of `form_round_trip_every_plan`: the folder each loads from
/// (as a user plan) and its id.
fn every_plan() -> Vec<(TempDir, &'static str)> {
    let example = TempDir::new("p9_rt_example");
    std::fs::copy(
        example_dir().join("example_plan.json"),
        example.path().join("example_plan.json"),
    )
    .unwrap();
    let mut dirs = vec![(example, "example_plan")];
    for (id, text) in [
        ("f15_roundtrip", include_str!("fixtures/f15_roundtrip.json")),
        ("p9_every_key", include_str!("fixtures/p9_every_key.json")),
    ] {
        let dir = TempDir::new(&format!("p9_rt_{id}"));
        std::fs::write(dir.path().join(format!("{id}.json")), text).unwrap();
        dirs.push((dir, id));
    }
    dirs
}

/// The plan `plan_id` as the library loads it from `dir`.
fn source_plan(dir: &Path, plan_id: &str) -> CapturePlan {
    let (mut plans, errors) = load_plans_from_dir_with_errors(dir);
    assert!(errors.is_empty(), "{errors:?}");
    plans
        .remove(plan_id)
        .unwrap_or_else(|| panic!("{plan_id} not in {dir:?}"))
}

/// The user plan `plan_id` of `dir`, loaded into the form as Edit does.
fn edit_form(plan: &CapturePlan, plan_id: &str, units: Units) -> NewCapturePlanForm {
    let mut form = NewCapturePlanForm::default();
    load_plan_into_form(&mut form, plan_id, plan, false);
    form.open = true;
    if let Units::MetricImperialMetric = units {
        let metric = form.clone();
        form.set_unit_system(UnitSystem::Imperial);
        assert_ne!(
            form.phases[0].max_velocity.text, metric.phases[0].max_velocity.text,
            "{plan_id}: imperial shows converted numbers"
        );
        form.set_unit_system(UnitSystem::Metric);
    }
    form
}

/// Save as the handler calls it: into `dir`, no project open.
fn save(form: &mut NewCapturePlanForm, dir: &Path) -> bool {
    let mut library = CapturePlanLibrary::default();
    save_plan_from_form(form, dir, false, false, &mut library, &mut |_, _| {})
}

fn json_file(path: &Path) -> serde_json::Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// F-15's invariant: for each plan, in metric and through the toggle, the form
/// gives the loaded plan back, an unchanged Save writes nothing, and the file
/// the form's plan writes equals the source (the v0 fixture: its v1 form, with
/// `migrated_from_v0`). Prints the Proof table (`-- --nocapture`).
#[test]
fn form_round_trip_every_plan() {
    println!(
        "| Plan | Units | form → plan == source | written == source | unchanged Save writes nothing |"
    );
    println!("|---|---|---|---|---|");
    for (dir, plan_id) in every_plan() {
        let source = source_plan(dir.path(), plan_id);
        for units in [Units::Metric, Units::MetricImperialMetric] {
            let mut form = edit_form(&source, plan_id, units);
            let plan = plan_from_form(&form).unwrap();
            assert_eq!(plan, source, "{plan_id} {units:?}");

            let out = TempDir::new(&format!("p9_rt_out_{plan_id}"));
            ssg_sim::project::plan_files::save_plan(out.path(), "written.json", &plan).unwrap();
            let written = json_file(&out.path().join("written.json"));
            let expected = if plan_id == "f15_roundtrip" {
                serde_json::from_str(&plan_to_json(&source)).unwrap()
            } else {
                json_file(&dir.path().join(format!("{plan_id}.json")))
            };
            assert_eq!(written, expected, "{plan_id} {units:?}");

            let unchanged = TempDir::new(&format!("p9_rt_unchanged_{plan_id}"));
            assert!(!save(&mut form, unchanged.path()), "{plan_id} {units:?}");
            assert!(!form.open, "{plan_id}: Save closes the form");
            assert_eq!(
                std::fs::read_dir(unchanged.path()).unwrap().count(),
                0,
                "{plan_id} {units:?}: an unchanged Save wrote a file"
            );
            println!("| `{plan_id}` | {units:?} | yes | yes | yes |");
        }
    }
}

/// An edit of one field writes the user's copy of the example plan with only
/// that field changed: all six phases and every key survive (was: refused with
/// "unknown phase 'final_approach'", F-15).
#[test]
fn form_round_trip_example_plan() {
    let dir = TempDir::new("p9_f15_example");
    std::fs::copy(
        example_dir().join("example_plan.json"),
        dir.path().join("example_plan.json"),
    )
    .unwrap();
    let source = source_plan(dir.path(), "example_plan");
    let mut form = edit_form(&source, "example_plan", Units::Metric);
    form.phases[1].max_velocity.text = "1.5".to_string();
    assert!(save(&mut form, dir.path()));

    let mut expected = json_file(&example_dir().join("example_plan.json"));
    assert_eq!(expected["phases"][1]["id"], "approach");
    expected["phases"][1]["max_velocity_mps"] = serde_json::json!(1.5);
    assert_eq!(
        json_file(&dir.path().join(generate_filename("example_plan"))),
        expected
    );
}

/// P0's fixture keeps its four phases (`rendezvous` included) and the
/// `migrated_from_v0` entry for the `units` note (was: both dropped, F-15).
#[test]
fn form_round_trip_f15_fixture() {
    let dir = TempDir::new("p9_f15_fixture");
    std::fs::write(
        dir.path().join("f15_roundtrip.json"),
        include_str!("fixtures/f15_roundtrip.json"),
    )
    .unwrap();
    let source = source_plan(dir.path(), "f15_roundtrip");
    let mut form = edit_form(&source, "f15_roundtrip", Units::Metric);
    form.phases[1].max_velocity.text = "1.5".to_string();
    assert!(save(&mut form, dir.path()));

    let written = json_file(&dir.path().join("f15_roundtrip.json"));
    assert_eq!(written["schema_version"], 1);
    let ids: Vec<&str> = written["phases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|phase| phase["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["rendezvous", "approach", "terminal", "capture"]);
    assert_eq!(
        written["migrated_from_v0"]["unmapped"],
        serde_json::json!([{"pointer": "/states/2/transitions/0/distance/units", "value": "m"}])
    );
    let mut expected = source.clone();
    expected.phases[1].max_velocity_mps = 1.5;
    assert_eq!(
        written,
        serde_json::from_str::<serde_json::Value>(&plan_to_json(&expected)).unwrap()
    );
}

/// The units a plan file is written in do not depend on the toggle: 1 ft/s and
/// 1 lbf typed in imperial are 0.3048 m/s and 4.44822 N.
#[test]
fn imperial_typed_values_are_written_metric() {
    let mut form = filled_form();
    form.set_unit_system(UnitSystem::Imperial);
    form.phases[1].max_velocity.text = "1.0".to_string();
    form.phases[1].max_force.text = "1.0".to_string();
    let plan = plan_from_form(&form).unwrap();
    assert_eq!(plan.phases[1].max_velocity_mps, 0.3048);
    assert!((plan.phases[1].max_force_n - 4.44822).abs() < 1e-9);
    // Metric numbers are written as typed.
    form.set_unit_system(UnitSystem::Metric);
    form.phases[1].max_velocity.text = "2.0".to_string();
    assert_eq!(
        plan_from_form(&form).unwrap().phases[1].max_velocity_mps,
        2.0
    );
}

/// An edited plan is written as v1, with no v0 shape and nothing migrated.
#[test]
fn a_new_plan_is_written_as_v1() {
    let dir = TempDir::new("p9_new_plan");
    let mut form = filled_form();
    assert!(save(&mut form, dir.path()));
    let text = std::fs::read_to_string(dir.path().join("p0_created.json")).unwrap();
    assert!(text.starts_with("{\n  \"schema_version\": 1,"), "{text}");
    let plan = source_plan(dir.path(), "p0_created");
    assert_eq!(plan.migrated_from_v0, None);
    assert_eq!(plan.phases.len(), 6);
}

/// Save of a new plan whose file exists asks first; Overwrite writes it.
#[test]
fn a_new_plan_over_an_existing_file_asks_first() {
    let dir = TempDir::new("p9_overwrite");
    std::fs::write(dir.path().join("p0_created.json"), "{}").unwrap();
    let mut form = filled_form();
    assert!(!save(&mut form, dir.path()));
    let conflict = form.overwrite_conflict_path.clone().unwrap();
    assert!(conflict.ends_with("p0_created.json"), "{conflict}");

    let mut library = CapturePlanLibrary::default();
    assert!(save_plan_from_form(
        &mut form,
        dir.path(),
        true,
        false,
        &mut library,
        &mut |_, _| {}
    ));
    assert_eq!(source_plan(dir.path(), "p0_created").name, "P0 Created");
    assert!(library.user_plans.contains_key("p0_created"));
}

/// An edit saved while a project is open asks for a restart, unless only the
/// name changed.
#[test]
fn an_edit_in_a_project_asks_for_a_restart_unless_only_the_name_changed() {
    let dir = TempDir::new("p9_restart");
    std::fs::copy(
        example_dir().join("example_plan.json"),
        dir.path().join("example_plan.json"),
    )
    .unwrap();
    let source = source_plan(dir.path(), "example_plan");
    for (edit, restart) in [(true, false), (false, true)] {
        let mut form = edit_form(&source, "example_plan", Units::Metric);
        if edit {
            form.plan_name = "Renamed".to_string();
        } else {
            form.phases[2].max_force.text = "4.0".to_string();
        }
        let mut library = CapturePlanLibrary::default();
        assert!(save_plan_from_form(
            &mut form,
            dir.path(),
            false,
            true,
            &mut library,
            &mut |_, _| {}
        ));
        assert_eq!(form.show_restart_prompt, restart);
    }
}
