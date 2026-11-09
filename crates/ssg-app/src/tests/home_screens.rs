//! The working-directory setup and Home screens on egui (P8 S3): labels,
//! buttons → `UiEvent`, the typed path and its error (F-33), and the scrolling
//! Home body (F-37).

use bevy_egui::egui;
use bevy_egui::egui::accesskit::Role;
use egui_kittest::kittest::Queryable;

use crate::tests::egui_support::harness_for;
use crate::ui::events::UiEvent;
use crate::ui::events_handler::{
    continues_without_saved_config, select_working_directory, select_working_directory_with,
};
use crate::ui::screens::home::{HomeView, PlanCard, home_ui};
use crate::ui::screens::working_directory_setup::{
    WorkingDirectorySetupView, working_directory_setup_ui,
};
use ssg_sim::project::WorkingDirectory;
use ssg_sim::test_support::TempDir;

pub(crate) fn setup_view(path: &str, error: Option<&str>) -> WorkingDirectorySetupView {
    WorkingDirectorySetupView {
        path: path.to_string(),
        error: error.map(str::to_string),
    }
}

fn cards(names: &[&str]) -> Vec<PlanCard> {
    names
        .iter()
        .map(|name| PlanCard {
            id: name.to_lowercase().replace(' ', "_"),
            name: name.to_string(),
        })
        .collect()
}

/// Home with `user` plan names and the example plan.
pub(crate) fn home_view(user: &[&str]) -> HomeView {
    HomeView {
        working_directory: "/Users/walk/wd".to_string(),
        user_plans: cards(user),
        example_plans: cards(&["Example Plan"]),
        load_errors: Vec::new(),
        logo: None,
        content_max_width: 1080.0,
    }
}

pub(crate) fn many_plans(count: usize) -> Vec<String> {
    (1..=count).map(|i| format!("Plan {i:02}")).collect()
}

#[test]
fn setup_screen_labels_and_buttons() {
    let mut harness = harness_for(
        setup_view("/Users/walk/wd", None),
        [800.0, 600.0],
        working_directory_setup_ui,
    );
    harness.get_by_label("Select Working Directory");
    harness.get_by_label("Choose where your projects will be stored.");
    assert_eq!(
        harness.get_by_role(Role::TextInput).value().as_deref(),
        Some("/Users/walk/wd")
    );

    harness.get_by_label("Browse").click();
    harness.run();
    harness.get_by_label("Select").click();
    harness.run();

    match harness.state().events.as_slice() {
        [
            UiEvent::BrowseForWorkingDirectory,
            UiEvent::WorkingDirectorySelected(path),
        ] => assert_eq!(path, "/Users/walk/wd"),
        events => panic!("unexpected events {events:?}"),
    }
}

#[test]
fn typed_path_selects_directory() {
    let mut harness = harness_for(
        setup_view("", None),
        [800.0, 600.0],
        working_directory_setup_ui,
    );
    harness.get_by_role(Role::TextInput).focus();
    harness.run();
    harness
        .get_by_role(Role::TextInput)
        .type_text("/Users/walk/typed");
    harness.run();
    assert_eq!(harness.state().view.path, "/Users/walk/typed");

    harness.get_by_label("Select").click();
    harness.run();
    match harness.state().events.as_slice() {
        [UiEvent::WorkingDirectorySelected(path)] => assert_eq!(path, "/Users/walk/typed"),
        events => panic!("unexpected events {events:?}"),
    }
}

#[test]
fn enter_in_the_path_field_selects_directory() {
    let mut harness = harness_for(
        setup_view("", None),
        [800.0, 600.0],
        working_directory_setup_ui,
    );
    harness.get_by_role(Role::TextInput).focus();
    harness.run();
    harness
        .get_by_role(Role::TextInput)
        .type_text("/Users/walk/typed");
    harness.run();
    assert!(harness.state().events.is_empty());

    harness.key_press(egui::Key::Enter);
    harness.run();
    match harness.state().events.as_slice() {
        [UiEvent::WorkingDirectorySelected(path)] => assert_eq!(path, "/Users/walk/typed"),
        events => panic!("unexpected events {events:?}"),
    }
}

#[test]
fn bad_path_shows_error() {
    // A folder cannot be created under a regular file.
    let dir = TempDir::new("p8_bad_working_directory");
    let file = dir.path().join("not_a_folder");
    std::fs::write(&file, "").unwrap();
    let bad = file.join("wd").to_string_lossy().to_string();
    let mut working_directory = WorkingDirectory {
        path: "/before".to_string(),
        pending_path: bad.clone(),
    };

    let message = select_working_directory(&mut working_directory, &bad)
        .expect_err("a folder under a file cannot be created");
    assert!(message.starts_with("Failed to create working directory: "));
    // Nothing changed: the old directory stays (and the config is not written).
    assert_eq!(working_directory.path, "/before");

    let harness = harness_for(
        setup_view(&bad, Some(&message)),
        [800.0, 600.0],
        working_directory_setup_ui,
    );
    harness.get_by_label(&message);
    harness.get_by_label("Select");
}

/// #176 (F-33), OQ2 (a): the folder is fine but the config cannot be written.
#[test]
fn an_unsaved_config_sets_the_folder_for_the_session_and_says_so() {
    let dir = TempDir::new("p10_5_unsaved_config");
    let good = dir.path().join("wd").to_string_lossy().to_string();
    let mut working_directory = WorkingDirectory {
        path: "/before".to_string(),
        pending_path: good.clone(),
    };

    let outcome = select_working_directory_with(&mut working_directory, &good, |wd, path| {
        // set_working_directory: the path is set, then the config write fails.
        wd.path = path.to_string();
        Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "Permission denied (os error 13)",
        ))
    });

    let message = outcome
        .expect("the folder was created")
        .expect("the config was not saved");
    assert_eq!(
        message,
        "The folder is set for this session, but it could not be saved for the next \
         launch: Permission denied (os error 13). Select again to continue."
    );
    assert_eq!(working_directory.path, good, "set for this session");
    assert!(std::path::Path::new(&good).is_dir());

    let harness = harness_for(
        setup_view(&good, Some(&message)),
        [800.0, 600.0],
        working_directory_setup_ui,
    );
    harness.get_by_label(&message);
    harness.get_by_label("Select");
}

#[test]
fn a_saved_config_selects_without_a_message() {
    let dir = TempDir::new("p10_5_saved_config");
    let good = dir.path().join("wd").to_string_lossy().to_string();
    let mut working_directory = WorkingDirectory {
        path: "/before".to_string(),
        pending_path: good.clone(),
    };
    let outcome = select_working_directory_with(&mut working_directory, &good, |wd, path| {
        wd.path = path.to_string();
        Ok(())
    });
    assert_eq!(outcome, Ok(None));
    assert_eq!(working_directory.path, good);
}

#[test]
fn a_second_select_of_the_same_path_continues_without_a_saved_config() {
    let message = "The folder is set for this session, but it could not be saved …";
    // First Select: nothing showing yet → stay and show the message.
    assert!(!continues_without_saved_config(
        None, message, "/before", "/wd"
    ));
    // Second Select of the same path with that message showing → Home.
    assert!(continues_without_saved_config(
        Some(message),
        message,
        "/wd",
        "/wd"
    ));
    // Another path, or another message showing → stay.
    assert!(!continues_without_saved_config(
        Some(message),
        message,
        "/wd",
        "/other"
    ));
    assert!(!continues_without_saved_config(
        Some("Failed to create working directory: …"),
        message,
        "/wd",
        "/wd"
    ));
}

#[test]
fn home_labels_and_buttons() {
    let mut harness = harness_for(
        home_view(&["F15 Roundtrip", "Q10 RelVel", "P0 Created"]),
        [1280.0, 800.0],
        home_ui,
    );
    for label in [
        "Tether Capture",
        "Conjunction Crew",
        "Working Directory",
        "Set the location where your projects are stored",
        "/Users/walk/wd",
        "My Capture Plans",
        "3 plans in working directory",
        "Example Capture Plans",
        "1 example plan",
        "F15 Roundtrip",
        "f15_roundtrip",
        "Example Plan",
        "example_plan",
    ] {
        harness.get_by_label(label);
    }
    // One edit button per user plan, none on the example plan.
    assert_eq!(harness.get_all_by_label("edit").count(), 3);
    assert!(harness.query_by_label_contains("Warning").is_none());

    harness.get_by_label("Change Directory").click();
    harness.run();
    harness.get_by_label("+ New Plan").click();
    harness.run();
    // The card's name is part of the card: clicking it opens the plan.
    harness.get_by_label("Q10 RelVel").click();
    harness.run();
    harness.get_all_by_label("edit").next().unwrap().click();
    harness.run();
    harness.get_by_label("Example Plan").click();
    harness.run();

    let events = &harness.state().events;
    assert!(matches!(events[0], UiEvent::ChangeWorkingDirectory));
    assert!(matches!(events[1], UiEvent::OpenNewCapturePlanForm));
    assert!(matches!(&events[2], UiEvent::OpenProject(id) if id == "q10_relvel"));
    // The edit button does not also open the plan.
    assert!(matches!(&events[3], UiEvent::EditCapturePlan(id) if id == "f15_roundtrip"));
    assert!(matches!(&events[4], UiEvent::OpenProject(id) if id == "example_plan"));
    assert_eq!(events.len(), 5);
}

#[test]
fn home_shows_the_load_error_banner() {
    let mut view = home_view(&["P0 Created"]);
    view.load_errors = vec![(
        "broken".to_string(),
        vec!["missing field `name`".to_string()],
    )];
    let harness = harness_for(view, [1280.0, 800.0], home_ui);
    harness.get_by_label("Warning: Some capture plan files could not be loaded");
    harness.get_by_label("  \u{2022} broken: missing field `name`");
    harness.get_by_label("1 plan in working directory");
}

#[test]
fn home_scrolls_to_last_card() {
    let names = many_plans(40);
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    let mut harness = harness_for(home_view(&names), [800.0, 600.0], home_ui);
    let below = harness.get_by_label("Example Plan").rect();
    assert!(
        below.min.y > 600.0,
        "the example card starts in view: {below:?}"
    );

    // The mouse wheel over the body scrolls it (F-37).
    harness.hover_at(egui::pos2(400.0, 400.0));
    for _ in 0..40 {
        harness.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, -400.0),
            modifiers: egui::Modifiers::default(),
        });
        harness.run();
    }
    let last = harness.get_by_label("Example Plan").rect();
    assert!(
        last.min.y >= 0.0 && last.max.y <= 600.0,
        "the last card is not in view after scrolling: {last:?}"
    );
    // The header stays on top.
    let header = harness.get_by_label("Tether Capture").rect();
    assert!(header.min.y < 84.0, "the header scrolled away: {header:?}");
}
