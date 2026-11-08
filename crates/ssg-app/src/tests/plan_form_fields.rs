//! The plan form's fields for every v1 key (P9 S2): one widget test per section
//! (label, edit → plan value), the kitchen-sink plan edited field by field, the
//! units of the new numbers, and absent keys staying absent.

use std::path::PathBuf;

use bevy_egui::egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{By, NodeT, Queryable};
use ssg_sim::capture::plan::{
    Actuation, CapturePlan, ConditionBounds, PhaseEnd, SpawnOrientation, validate_capture_plan,
};
use ssg_sim::project::load_plans_from_dir_with_errors;
use ssg_sim::project::plan_files::plan_to_json;
use ssg_sim::test_support::TempDir;

use crate::tests::egui_support::{ScreenState, harness_for, snapshot_after};
use crate::tests::plan_form::{TALL, filled_form};
use crate::ui::form::{ControllerKey, NewCapturePlanForm, NumberField, UnitSystem};
use crate::ui::screens::capture_plan::form::{load_plan_into_form, plan_from_form, pointer};
use crate::ui::screens::capture_plan::window::capture_plan_form_ui;

type FormHarness<'a> = Harness<'a, ScreenState<NewCapturePlanForm>>;

/// The kitchen-sink plan (`p9_every_key.json`: every v1 key set), as Edit loads it.
fn every_key_plan() -> CapturePlan {
    // `TempDir` is named per process and tag: one tag per call, as the tests
    // run in parallel.
    static CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = TempDir::new(&format!("p9_s2_every_key_{n}"));
    std::fs::write(
        dir.path().join("p9_every_key.json"),
        include_str!("fixtures/p9_every_key.json"),
    )
    .unwrap();
    let (mut plans, errors) = load_plans_from_dir_with_errors(dir.path());
    assert!(errors.is_empty(), "{errors:?}");
    plans.remove("p9_every_key").unwrap()
}

fn every_key_form() -> NewCapturePlanForm {
    let mut form = NewCapturePlanForm::default();
    load_plan_into_form(&mut form, "p9_every_key", &every_key_plan(), false);
    form.open = true;
    form
}

/// Types `text` into the `nth` text field labelled `label`.
fn type_into(harness: &mut FormHarness, label: &str, nth: usize, text: &str) {
    let field = || By::new().role(Role::TextInput).label(label);
    harness.get_all(field()).nth(nth).unwrap().focus();
    harness.run();
    harness.get_all(field()).nth(nth).unwrap().type_text(text);
    harness.run();
}

fn click(harness: &mut FormHarness, label: &str, nth: usize) {
    harness
        .get_all_by_label(label)
        .nth(nth)
        .unwrap_or_else(|| panic!("no {label} #{nth}"))
        .click();
    harness.run();
}

fn plan(harness: &FormHarness) -> CapturePlan {
    plan_from_form(&harness.state().view).unwrap()
}

#[test]
fn tether_section_writes_every_device_key() {
    let mut form = filled_form();
    for field in [
        &mut form.segment_spacing,
        &mut form.segment_mass,
        &mut form.end_mass,
        &mut form.damping_linear,
        &mut form.damping_angular,
    ] {
        *field = NumberField::default();
    }
    let mut harness = harness_for(form, TALL, capture_plan_form_ui);
    for (label, text) in [
        ("Segment Spacing (m)", "0.5"),
        ("Segment Mass (kg)", "0.4"),
        ("End Mass (kg)", "3"),
        ("Linear Damping", "0.9"),
        ("Angular Damping (no effect on the rope model)", "0.8"),
    ] {
        type_into(&mut harness, label, 0, text);
    }
    click(&mut harness, "V-bar", 0);
    let device = plan(&harness).device.unwrap();
    assert_eq!(device.segment_spacing_m, Some(0.5));
    assert_eq!(device.segment_mass_kg, Some(0.4));
    assert_eq!(device.end_mass_kg, Some(3.0));
    assert_eq!(device.joint_damping_linear, Some(0.9));
    assert_eq!(device.joint_damping_angular, Some(0.8));
    assert_eq!(device.spawn_orientation, Some(SpawnOrientation::VBar));

    click(&mut harness, "Default (toward target)", 0);
    assert_eq!(plan(&harness).device.unwrap().spawn_orientation, None);
}

#[test]
fn capture_sphere_section_writes_both_radii() {
    let mut form = filled_form();
    form.initial_radius = NumberField::default();
    form.min_radius = NumberField::default();
    let mut harness = harness_for(form, TALL, capture_plan_form_ui);
    harness.get_by_label("Capture Sphere");
    type_into(&mut harness, "Initial Radius (m)", 0, "60");
    type_into(&mut harness, "Min Radius (m)", 0, "1.5");
    let sphere = plan(&harness).capture_sphere.unwrap();
    assert_eq!(sphere.initial_radius_m, Some(60.0));
    assert_eq!(sphere.min_radius_m, Some(1.5));
}

/// The example plan's six phases each show End, Actuation, the three switches,
/// Tail Spacing and a collapsed Controller group.
#[test]
fn phase_section_writes_every_phase_key() {
    let mut form = filled_form();
    form.phases[0].tail_spacing = NumberField::default();
    let mut harness = harness_for(form, TALL, capture_plan_form_ui);
    assert_eq!(harness.get_all_by_label("End").count(), 6);
    assert_eq!(harness.get_all_by_label("Controller").count(), 6);
    // The controller's fields are hidden until the group is opened.
    assert!(harness.query_by_label("Radius Hold Band (m)").is_none());

    click(&mut harness, "Abort", 5);
    click(&mut harness, "Hold", 1);
    // Phase 0's three switches, in order: Tail Actuation, Hold Plane, Speed
    // Scales with Sphere.
    click(&mut harness, "Off", 0);
    click(&mut harness, "On", 1);
    click(&mut harness, "Default (off)", 2);
    type_into(&mut harness, "Tail Spacing (m)", 0, "5");
    click(&mut harness, "Controller", 0);
    type_into(&mut harness, "Radius Hold Band (m)", 0, "1.25");
    type_into(&mut harness, "Force Ramp (s)", 0, "2");

    let plan = plan(&harness);
    assert_eq!(plan.phases[5].end, Some(PhaseEnd::Abort));
    assert_eq!(plan.phases[1].actuation, Some(Actuation::Hold));
    let phase = &plan.phases[0];
    assert_eq!(phase.tail_actuation, Some(false));
    assert_eq!(phase.hold_plane, Some(true));
    assert_eq!(phase.velocity_scales_with_sphere, None);
    assert_eq!(phase.tail_spacing_m, Some(5.0));
    let controller = phase.controller.unwrap();
    assert_eq!(controller.radius_hold_band_m, Some(1.25));
    assert_eq!(controller.force_ramp_s, Some(2.0));
    assert_eq!(
        controller.spacing_band_m, None,
        "untouched gains stay absent"
    );
}

#[test]
fn the_controller_group_shows_each_default_as_its_hint() {
    let mut harness = harness_for(filled_form(), TALL, capture_plan_form_ui);
    click(&mut harness, "Controller", 0);
    for key in ControllerKey::ALL {
        let label = key.quantity().labelled(key.label(), UnitSystem::Metric);
        let field = harness
            .get_all(By::new().role(Role::TextInput).label(&label))
            .next()
            .unwrap_or_else(|| panic!("no field {label}"));
        let default = serde_json::Value::from(key.default_value()).to_string();
        assert_eq!(
            field.accesskit_node().placeholder(),
            Some(format!("default {default}").as_str()),
            "{label}"
        );
    }
}

/// Q10 rows 3, 5, 6 and E4 (b): "+ Condition" adds any condition the transition
/// does not have; all of them must hold.
#[test]
fn add_condition_offers_the_missing_conditions() {
    let mut harness = harness_for(filled_form(), TALL, capture_plan_form_ui);
    let before = plan(&harness).phases[0].transitions.clone().unwrap()[0].clone();
    assert!(before.all_of.closing_rate_mps.is_none());

    click(&mut harness, "+ Condition", 0);
    click(&mut harness, "Closing Rate", 0);
    // An added condition with no bound stays absent.
    assert_eq!(
        plan(&harness).phases[0].transitions.as_ref().unwrap()[0],
        before
    );
    type_into(&mut harness, "Closing Rate less than (m/s)", 0, "0.2");

    click(&mut harness, "+ Condition", 0);
    click(&mut harness, "Tether Straightness", 0);
    // A fraction: no unit in the label.
    type_into(&mut harness, "Tether Straightness less than", 0, "0.15");

    let all_of = plan(&harness).phases[0].transitions.as_ref().unwrap()[0]
        .all_of
        .clone();
    assert_eq!(
        all_of.closing_rate_mps,
        Some(ConditionBounds {
            less_than: Some(0.2),
            greater_than: None
        })
    );
    assert_eq!(
        all_of.tether_straightness,
        Some(ConditionBounds {
            less_than: Some(0.15),
            greater_than: None
        })
    );
    assert_eq!(all_of.distance_m, before.all_of.distance_m, "AND: kept");
}

/// Every number field of the kitchen-sink plan, with the JSON Pointer of the
/// value it holds (the pointers `field_pointers` shows messages at).
fn numbers(form: &mut NewCapturePlanForm) -> Vec<(String, &mut NumberField)> {
    let mut fields: Vec<(String, &mut NumberField)> = vec![
        (pointer::TETHER_LENGTH.to_string(), &mut form.tether_length),
        (
            pointer::INITIAL_RADIUS.to_string(),
            &mut form.initial_radius,
        ),
        (pointer::MIN_RADIUS.to_string(), &mut form.min_radius),
    ];
    for (at, field) in pointer::DEVICE_NUMBERS.into_iter().zip([
        &mut form.segment_spacing,
        &mut form.segment_mass,
        &mut form.end_mass,
        &mut form.damping_linear,
        &mut form.damping_angular,
    ]) {
        fields.push((at.to_string(), field));
    }
    for (i, phase) in form.phases.iter_mut().enumerate() {
        fields.push((
            pointer::phase(i, "max_velocity_mps"),
            &mut phase.max_velocity,
        ));
        fields.push((pointer::phase(i, "max_force_n"), &mut phase.max_force));
        fields.push((pointer::phase(i, "shrink_rate_mps"), &mut phase.shrink_rate));
        fields.push((pointer::phase(i, "tail_spacing_m"), &mut phase.tail_spacing));
        for (field, key) in phase.controller.iter_mut().zip(ControllerKey::ALL) {
            fields.push((
                pointer::phase(i, &format!("controller/{}", key.key())),
                field,
            ));
        }
        for (j, transition) in phase.transitions.iter_mut().enumerate() {
            for condition in &mut transition.conditions {
                let key = condition.kind.key();
                for (bound, field) in [
                    ("less_than", &mut condition.less_than),
                    ("greater_than", &mut condition.greater_than),
                ] {
                    let at = format!("all_of/{key}/{bound}");
                    fields.push((pointer::transition(i, j, &at), field));
                }
            }
        }
    }
    fields
}

/// A new value for the field at `at` that keeps the plan valid.
fn edited_value(at: &str) -> f64 {
    match at {
        "/device/tether_length_m" => 40.5,
        "/capture_sphere/initial_radius_m" => 60.5,
        _ => 0.375,
    }
}

/// The kitchen-sink plan edited one field at a time: each edit changes that one
/// value of the written file and nothing else, for every key the form shows.
#[test]
fn every_key_plan_edited_field_by_field() {
    let source = every_key_plan();
    let source_json: serde_json::Value = serde_json::from_str(&plan_to_json(&source)).unwrap();
    let count = numbers(&mut every_key_form())
        .into_iter()
        .filter(|(_, field)| !field.text.is_empty())
        .count();
    // device 6 + capture_sphere 2; phases: limits 8, `circle`'s shrink rate,
    // tail spacing and 8 gains; condition bounds 9.
    assert_eq!(count, 35, "number fields with a value");
    let mut keys = std::collections::BTreeSet::new();
    for n in 0..count {
        let mut form = every_key_form();
        let (at, field) = numbers(&mut form)
            .into_iter()
            .filter(|(_, field)| !field.text.is_empty())
            .nth(n)
            .unwrap();
        let value = edited_value(&at);
        field.text = value.to_string();
        let plan = plan_from_form(&form).unwrap_or_else(|e| panic!("{at}: {e:?}"));
        assert!(
            validate_capture_plan("p9_every_key", &plan).is_empty(),
            "{at}"
        );
        let mut expected = source_json.clone();
        *expected.pointer_mut(&at).unwrap_or_else(|| panic!("{at}")) = value.into();
        let written: serde_json::Value = serde_json::from_str(&plan_to_json(&plan)).unwrap();
        assert_eq!(written, expected, "{at}");
        keys.insert(at.rsplit('/').next().unwrap().to_string());
    }
    // Every number key of the v1 plan is among them.
    for key in [
        "tether_length_m",
        "segment_spacing_m",
        "segment_mass_kg",
        "end_mass_kg",
        "joint_damping_linear",
        "joint_damping_angular",
        "initial_radius_m",
        "min_radius_m",
        "max_velocity_mps",
        "max_force_n",
        "shrink_rate_mps",
        "tail_spacing_m",
        "less_than",
        "greater_than",
    ]
    .into_iter()
    .chain(ControllerKey::ALL.map(ControllerKey::key))
    {
        assert!(keys.contains(key), "{key} not edited");
    }

    // The choices: each written as chosen, the rest unchanged.
    let mut form = every_key_form();
    form.spawn_orientation = Some(SpawnOrientation::RBar);
    form.phases[1].actuation = Some(Actuation::Hold);
    form.phases[1].tail_actuation = None;
    form.phases[1].hold_plane = Some(false);
    form.phases[1].velocity_scales_with_sphere = Some(false);
    form.phases[2].end = Some(PhaseEnd::Abort);
    let written: serde_json::Value =
        serde_json::from_str(&plan_to_json(&plan_from_form(&form).unwrap())).unwrap();
    let mut expected = source_json.clone();
    expected["device"]["spawn_orientation"] = "r_bar".into();
    let circle = expected["phases"][1].as_object_mut().unwrap();
    circle.insert("actuation".into(), "hold".into());
    circle.remove("tail_actuation");
    circle.insert("hold_plane".into(), false.into());
    circle.insert("velocity_scales_with_sphere".into(), false.into());
    expected["phases"][2]["end"] = "abort".into();
    assert_eq!(written, expected);
}

/// A–6: masses in lb, lengths in ft; the file stays metric.
#[test]
fn imperial_shows_the_new_numbers_converted_and_writes_metric() {
    let mut form = every_key_form();
    form.set_unit_system(UnitSystem::Imperial);
    let harness = harness_for(form.clone(), TALL, capture_plan_form_ui);
    for label in [
        "Segment Spacing (ft)",
        "Segment Mass (lb)",
        "End Mass (lb)",
        "Initial Radius (ft)",
        "Tail Spacing (ft)",
        "Closing Rate less than (ft/s)",
        "Linear Damping",
    ] {
        assert!(
            harness.get_all_by_label(label).next().is_some(),
            "no {label}"
        );
    }
    form.segment_mass.text = "1".to_string();
    form.initial_radius.text = "100".to_string();
    form.phases[1].tail_spacing.text = "10".to_string();
    let plan = plan_from_form(&form).unwrap();
    let device = plan.device.unwrap();
    assert_eq!(device.segment_mass_kg, Some(0.45359237));
    assert_eq!(plan.capture_sphere.unwrap().initial_radius_m, Some(30.48));
    assert_eq!(plan.phases[1].tail_spacing_m, Some(3.048));
    // Untouched values come back exactly, as loaded.
    assert_eq!(device.end_mass_kg, Some(3.25));
}

/// A sparse plan stays sparse: an empty field is an absent key.
#[test]
fn empty_fields_leave_absent_keys_absent() {
    let mut form = filled_form();
    let source = plan_from_form(&form).unwrap();
    // Opening the Controller group and adding a condition without typing a value
    // changes nothing.
    form.phases[0].transitions[0]
        .conditions
        .push(crate::ui::form::ConditionForm {
            kind: crate::ui::form::ConditionKind::ClosingRate,
            less_than: NumberField::default(),
            greater_than: NumberField::default(),
        });
    let plan = plan_from_form(&form).unwrap();
    assert_eq!(plan, source);
    assert!(plan.phases.iter().all(|phase| phase.controller.is_none()));
}

/// The S2 PNGs: the Tether and Capture Sphere sections, a phase with its
/// Controller group open, and a transition with all five conditions.
pub(crate) fn p9_s2_snapshots() -> Vec<PathBuf> {
    vec![
        snapshot_after(
            "p9_form_every_key",
            every_key_form(),
            [720.0, 8000.0],
            capture_plan_form_ui,
            |_| {},
        ),
        snapshot_after(
            "p9_form_controller_open",
            every_key_form(),
            [720.0, 8000.0],
            capture_plan_form_ui,
            |harness| click(harness, "Controller", 1),
        ),
        snapshot_after(
            "p9_form_add_condition_menu",
            filled_form(),
            [720.0, 1700.0],
            capture_plan_form_ui,
            |harness| click(harness, "+ Condition", 0),
        ),
    ]
}
