//! The capture-plan form on egui (P8 S4): its controls → `UiEvent`, text
//! typed into the numeric fields and validated at Save (P0-001), the overwrite
//! prompt's Cancel (P0-002), "Phase" wording (A-3), and the F-15 round trip of
//! each example plan and P0's `f15_roundtrip` fixture as the form does it today.

use std::path::Path;

use bevy_egui::egui::accesskit::Role;
use egui_kittest::kittest::{By, Queryable};

use crate::tests::egui_support::harness_for;
use crate::ui::egui_theme::visuals_from_theme;
use crate::ui::events::UiEvent;
use crate::ui::form::{NewCapturePlanForm, UnitSystem};
use crate::ui::plugin::{load_plan_into_form, save_form_plan};
use crate::ui::screens::capture_plan::window::{capture_plan_form_ui, form_fill};
use crate::ui::screens::capture_plan::{build_capture_plan_json, generate_filename, validate_form};
use crate::ui::theme::UiTheme;
use ssg_sim::project::load_plans_from_dir_with_errors;
use ssg_sim::test_support::TempDir;

/// A new, blank form as **+ New Plan** opens it.
pub(crate) fn new_form() -> NewCapturePlanForm {
    let mut form = NewCapturePlanForm::default();
    form.reset();
    form.open = true;
    form
}

/// The form filled as the O2 walk filled it (S2.03).
pub(crate) fn filled_form() -> NewCapturePlanForm {
    let mut form = new_form();
    form.plan_name = "P0 Created".to_string();
    form.approach_max_velocity = "2".to_string();
    form.approach_max_force = "3".to_string();
    form.approach_transitions = vec![crate::ui::form::TransitionForm {
        to: "terminal".to_string(),
        distance_kind: "less_than".to_string(),
        distance_value: "50".to_string(),
    }];
    form.terminal_max_velocity = "0.5".to_string();
    form.terminal_max_force = "2".to_string();
    form.terminal_shrink_rate = "0.01".to_string();
    form.capture_max_velocity = "0.5".to_string();
    form.capture_max_force = "2".to_string();
    form.capture_shrink_rate = "0.005".to_string();
    form
}

const SIZE: [f32; 2] = [720.0, 2000.0];

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
        "Tether Type *",
        "Net (coming soon)",
        "Approach Phase",
        "Terminal Phase",
        "Capture Phase",
        "To Phase",
        "Condition",
        "Transition 1",
        "Distance (m)",
    ] {
        harness.get_by_label(label);
    }
    // The section title and the tether-type button.
    assert_eq!(harness.get_all_by_label("Tether").count(), 2);
    assert!(harness.query_by_label_contains("State").is_none());
}

#[test]
fn form_buttons_emit_ui_events() {
    let mut harness = harness_for(filled_form(), SIZE, capture_plan_form_ui);
    for label in ["ft (imperial)", "×", "Save", "× Cancel"] {
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
    assert!(matches!(events[1], UiEvent::RemoveApproachTransition(0)));
    assert!(matches!(events[2], UiEvent::SaveCapturePlan));
    assert!(matches!(events[3], UiEvent::CloseNewCapturePlanForm));
    assert!(matches!(events[4], UiEvent::AddApproachTransition));
    assert!(matches!(events[5], UiEvent::AddTerminalTransition));
    assert_eq!(events.len(), 6);
}

#[test]
fn blank_save_lists_the_errors() {
    let mut harness = harness_for(new_form(), SIZE, capture_plan_form_ui);
    harness.get_by_label("Save").click();
    harness.run();
    assert!(matches!(
        harness.state().events.as_slice(),
        [UiEvent::SaveCapturePlan]
    ));

    // The handler validates on Save (P0-001).
    let errors = validate_form(&harness.state().view);
    harness.state_mut().view.validation_errors = errors;
    harness.run();
    harness.get_by_label("Please fix the following errors:");
    harness.get_by_label("• Plan Name is required");
    harness.get_by_label("• Approach Max Velocity is required");
}

#[test]
fn letters_typed_then_save_says_must_be_a_number() {
    let mut harness = harness_for(new_form(), SIZE, capture_plan_form_ui);
    let field = By::new()
        .role(Role::TextInput)
        .label("Max Velocity * (m/s)");
    harness.get_all(field.clone()).next().unwrap().focus();
    harness.run();
    harness.get_all(field).next().unwrap().type_text("abc");
    harness.run();
    // Letters are accepted (P0-001): the field holds what was typed.
    assert_eq!(harness.state().view.approach_max_velocity, "abc");

    let errors = validate_form(&harness.state().view);
    assert!(errors.contains(&"Approach Max Velocity must be a number".to_string()));
    harness.state_mut().view.validation_errors = errors;
    harness.run();
    harness.get_by_label("• Approach Max Velocity must be a number");
}

#[test]
fn transition_dropdown_sets_the_phase() {
    let mut form = filled_form();
    form.approach_transitions[0].to.clear();
    let mut harness = harness_for(form, SIZE, capture_plan_form_ui);
    harness
        .get_all(By::new().role(Role::ComboBox))
        .find(|combo| combo.value().as_deref() == Some("—"))
        .expect("the empty To Phase combo box")
        .click();
    harness.run();
    harness.get_by_label("capture").click();
    harness.run();
    assert_eq!(harness.state().view.approach_transitions[0].to, "capture");
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

#[test]
fn example_plan_opens_read_only() {
    let mut form = filled_form();
    form.read_only = true;
    form.editing_plan_id = Some("example_plan".to_string());
    let mut harness = harness_for(form, SIZE, capture_plan_form_ui);
    harness.get_by_label("View Capture Plan");
    assert!(harness.query_by_label("Save").is_none());

    let name = By::new().role(Role::TextInput).label("Plan Name *");
    harness.get_all(name.clone()).next().unwrap().focus();
    harness.run();
    harness.get_all(name).next().unwrap().type_text("x");
    harness.run();
    assert_eq!(harness.state().view.plan_name, "P0 Created");
}

// ── F-15 round trip (A-3; fixed in P9, recorded here) ────────────────────────

/// The phase ids and each phase's keys of a written plan file.
fn phase_keys(json: &serde_json::Value) -> Vec<(String, Vec<String>)> {
    json["phases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|phase| {
            (
                phase["id"].as_str().unwrap().to_string(),
                phase.as_object().unwrap().keys().cloned().collect(),
            )
        })
        .collect()
}

/// Loads `plan_id` from `dir` into the form as **edit** does, checks that an
/// unchanged Save writes nothing, saves with Approach Max Velocity edited and
/// returns the source phase ids and the written file (or the save error).
fn round_trip(dir: &Path, plan_id: &str) -> (Vec<String>, Result<serde_json::Value, String>) {
    let (plans, errors) = load_plans_from_dir_with_errors(dir);
    assert!(errors.is_empty(), "{errors:?}");
    let plan = plans
        .get(plan_id)
        .unwrap_or_else(|| panic!("{plan_id} not in {:?} ({dir:?})", plans.keys()));
    let source_phases = plan.phases.iter().map(|p| p.id.clone()).collect();

    let mut form = NewCapturePlanForm::default();
    load_plan_into_form(&mut form, plan_id, plan, false);
    // Save with no change closes the form without writing (the handler's check).
    assert_eq!(
        form.original_json.as_ref(),
        Some(&build_capture_plan_json(&form)),
        "{plan_id}: an unchanged form would be written"
    );

    form.approach_max_velocity = "1.5".to_string();
    assert!(
        validate_form(&form).is_empty(),
        "{:?}",
        validate_form(&form)
    );
    let out = TempDir::new(&format!("p8_f15_{plan_id}"));
    let filename = generate_filename(plan_id);
    let written =
        save_form_plan(out.path(), &filename, &build_capture_plan_json(&form)).map(|()| {
            let text = std::fs::read_to_string(out.path().join(&filename)).unwrap();
            serde_json::from_str(&text).unwrap()
        });
    (source_phases, written)
}

const FORM_PHASES: [&str; 3] = ["approach", "terminal", "capture"];

/// F-15 → P9: the example plan's six phases do not fit the form's three, and
/// its approach and terminal transitions lead to `final_approach`, which the
/// form drops: an edited save is refused (logged as "Failed to write capture
/// plan"). The example is read-only in the app; a user copy of it is not.
#[test]
fn form_round_trip_example_plan() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/example_capture_plans");
    let (source, written) = round_trip(&dir, "example_plan");
    assert_eq!(
        source,
        [
            "rendezvous",
            "approach",
            "final_approach",
            "terminal",
            "capture",
            "hold"
        ]
    );
    let error = written.expect_err("the edited example plan is refused today");
    assert!(error.contains("unknown phase 'final_approach'"), "{error}");
}

/// F-15 → P9: P0's fixture loses its `rendezvous` phase (with the transition
/// into `approach`) and the `units` note of a distance condition; the rest,
/// both terminal transitions included, is written as v1.
#[test]
fn form_round_trip_f15_fixture() {
    let dir = TempDir::new("p8_f15_fixture_source");
    std::fs::write(
        dir.path().join("f15_roundtrip.json"),
        include_str!("fixtures/f15_roundtrip.json"),
    )
    .unwrap();
    let (source, written) = round_trip(dir.path(), "f15_roundtrip");
    assert_eq!(source, ["rendezvous", "approach", "terminal", "capture"]);
    let written = written.unwrap();
    assert_eq!(written["schema_version"], 1);
    let keys = |list: &[&str]| list.iter().map(|k| k.to_string()).collect::<Vec<_>>();
    assert_eq!(
        phase_keys(&written),
        [
            (
                FORM_PHASES[0].to_string(),
                keys(&["id", "max_force_n", "max_velocity_mps", "transitions"])
            ),
            (
                FORM_PHASES[1].to_string(),
                keys(&[
                    "id",
                    "max_force_n",
                    "max_velocity_mps",
                    "shrink_rate_mps",
                    "transitions"
                ])
            ),
            (
                FORM_PHASES[2].to_string(),
                keys(&["id", "max_force_n", "max_velocity_mps", "shrink_rate_mps"])
            ),
        ]
    );
    assert_eq!(written["phases"][0]["max_velocity_mps"], 1.5);
    let terminal = &written["phases"][1]["transitions"];
    assert_eq!(
        terminal[0]["all_of"]["distance_m"],
        serde_json::json!({"less_than": 5.0})
    );
    assert_eq!(
        terminal[1]["all_of"]["distance_m"],
        serde_json::json!({"greater_than": 50.0})
    );
}
