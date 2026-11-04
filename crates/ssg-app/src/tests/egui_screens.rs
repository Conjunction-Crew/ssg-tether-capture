//! egui foundation (P8 S1): the theme mapping, the keyboard run condition, the
//! screen harness and the local snapshot test that every port slice extends.

use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;
use bevy_egui::egui;
use bevy_egui::input::EguiWantsInput;
use egui_kittest::kittest::Queryable;

use crate::tests::egui_support::{harness_for, snapshot};
use crate::tests::home_screens::{home_view, many_plans, setup_view};
use crate::tests::plan_form::{filled_form, new_form};
use crate::ui::egui_theme::{color32_from, visuals_from_theme};
use crate::ui::events::UiEvent;
use crate::ui::input::egui_wants_keyboard;
use crate::ui::screens::capture_plan::validate_form;
use crate::ui::screens::capture_plan::window::capture_plan_form_ui;
use crate::ui::screens::home::home_ui;
use crate::ui::screens::working_directory_setup::working_directory_setup_ui;
use crate::ui::theme::UiTheme;

/// Every colour the mapping can carry, for the coverage check.
fn mapped_colours(visuals: &egui::Visuals) -> Vec<egui::Color32> {
    let mut colours = vec![
        visuals.extreme_bg_color,
        visuals.panel_fill,
        visuals.window_fill,
        visuals.faint_bg_color,
        visuals.hyperlink_color,
        visuals.selection.bg_fill,
    ];
    colours.extend(visuals.override_text_color);
    colours.extend(visuals.weak_text_color);
    let widgets = &visuals.widgets;
    for state in [
        &widgets.noninteractive,
        &widgets.inactive,
        &widgets.hovered,
        &widgets.active,
        &widgets.open,
    ] {
        colours.extend([state.bg_fill, state.weak_bg_fill, state.fg_stroke.color]);
    }
    colours
}

#[test]
fn visuals_from_theme_maps_every_colour() {
    let theme = UiTheme::default();
    let visuals = visuals_from_theme(&theme);
    let colours = mapped_colours(&visuals);

    assert!(visuals.dark_mode);
    for (field, colour) in [
        ("background", theme.background),
        ("header_background", theme.header_background),
        ("panel_background", theme.panel_background),
        ("panel_background_soft", theme.panel_background_soft),
        ("text_primary", theme.text_primary),
        ("text_muted", theme.text_muted),
        ("text_accent", theme.text_accent),
        ("button_background", theme.button_background),
        ("button_background_hover", theme.button_background_hover),
        ("button_text", theme.button_text),
    ] {
        assert!(
            colours.contains(&color32_from(colour)),
            "UiTheme::{field} is not in the egui visuals"
        );
    }
    assert_eq!(visuals.panel_fill, color32_from(theme.panel_background));
    assert_eq!(
        visuals.widgets.inactive.bg_fill,
        color32_from(theme.button_background)
    );
}

#[test]
fn color32_from_keeps_srgb_bytes_and_alpha() {
    assert_eq!(
        color32_from(Color::srgba_u8(10, 20, 30, 128)),
        egui::Color32::from_rgba_unmultiplied(10, 20, 30, 128)
    );
}

#[test]
fn egui_wants_keyboard_false_without_egui_or_focus() {
    let mut world = World::new();
    assert!(!world.run_system_once(egui_wants_keyboard).unwrap());

    world.init_resource::<EguiWantsInput>();
    assert!(!world.run_system_once(egui_wants_keyboard).unwrap());
}

/// A stand-in screen for the harness checks until S3 ports the first one.
fn theme_sample_ui(ui: &mut egui::Ui, title: &mut &str, out: &mut Vec<UiEvent>) {
    ui.heading(*title);
    ui.label("Primary text");
    ui.weak("Muted text");
    if ui.button("Start Simulation").clicked() {
        out.push(UiEvent::StartSim);
    }
    ui.add_enabled(false, egui::Button::new("Disabled"));
    egui::CollapsingHeader::new("Section")
        .default_open(true)
        .show(ui, |ui| ui.label("Body"));
}

#[test]
fn harness_click_emits_ui_event() {
    let mut harness = harness_for("Theme sample", [320.0, 240.0], theme_sample_ui);
    harness.get_by_label("Theme sample");
    assert!(harness.state().events.is_empty());

    harness.get_by_label("Start Simulation").click();
    harness.run();

    assert!(matches!(
        harness.state().events.as_slice(),
        [UiEvent::StartSim]
    ));
}

/// Renders each ported screen to `target/p8-snapshots/*.png` for review
/// (rule 28). Needs a GPU, so it never runs on CI:
/// `cargo test -p ssg-app --locked p8_screen_snapshots -- --ignored`.
#[test]
#[ignore = "needs a GPU; run locally to write target/p8-snapshots"]
fn p8_screen_snapshots() {
    let path = snapshot(
        "s1_theme_sample",
        "Theme sample",
        [320.0, 240.0],
        theme_sample_ui,
    );
    assert!(path.is_file());

    // S3: the setup screen and Home.
    let mut paths = vec![
        snapshot(
            "s3_setup_empty",
            setup_view("", None),
            [1280.0, 800.0],
            working_directory_setup_ui,
        ),
        snapshot(
            "s3_setup_error",
            setup_view(
                "/Users/walk/not_a_folder/wd",
                Some("Failed to create working directory: Not a directory (os error 20)"),
            ),
            [1280.0, 800.0],
            working_directory_setup_ui,
        ),
        snapshot(
            "s3_home_3_plans",
            home_view(&["F15 Roundtrip", "Q10 RelVel", "P0 Created"]),
            [1280.0, 800.0],
            home_ui,
        ),
    ];
    let names = many_plans(40);
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    paths.push(snapshot(
        "s3_home_40_plans",
        home_view(&names),
        [800.0, 600.0],
        home_ui,
    ));

    // S4: the plan form.
    let mut blank = new_form();
    blank.approach_max_velocity = "abc".to_string();
    blank.validation_errors = validate_form(&blank);
    let mut conflict = filled_form();
    conflict.overwrite_conflict_path = Some("/Users/walk/wd/p0_created.json".to_string());
    let mut view = filled_form();
    view.read_only = true;
    view.editing_plan_id = Some("example_plan".to_string());
    for (name, form) in [
        ("s4_form_filled", filled_form()),
        ("s4_form_errors", blank),
        ("s4_form_overwrite_prompt", conflict),
        ("s4_form_view_example", view),
    ] {
        paths.push(snapshot(name, form, [720.0, 1100.0], capture_plan_form_ui));
    }
    assert!(paths.iter().all(|path| path.is_file()));
}
