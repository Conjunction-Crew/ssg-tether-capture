//! The P9 W1 walk fix-ups of the capture-plan form (O3, `results/o3-walk.md`):
//! the restart prompt only after an edit saved on the sim screen (W1.03 (b)),
//! the form opening at the top every time (W1.03 (c)), its solid scroll bar
//! (W1.01) and its collapsible sections (W1.06).

use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;
use egui_kittest::kittest::Queryable;
use ssg_sim::capture::SimCommand;
use ssg_sim::project::{SelectedProject, WorkingDirectory};
use ssg_sim::test_support::TempDir;
use ssg_sim::time::StepCap;

use crate::tests::egui_support::harness_for;
use crate::tests::plan_form::{example_dir, filled_form};
use crate::tests::sim_screen::sim_screen_app;
use crate::ui::events::UiEvent;
use crate::ui::events_handler::{FileDialogTask, UserPlansDirty, handle_ui_events};
use crate::ui::form::NewCapturePlanForm;
use crate::ui::screen::UiScreen;
use crate::ui::screens::capture_plan::form::load_plan_into_form;
use crate::ui::screens::capture_plan::window::{
    capture_plan_form_ui, form_closed, form_scroll_style,
};
use crate::ui::screens::working_directory_setup::WorkingDirectorySetupError;
use ssg_sim::project::load_plans_from_dir_with_errors;

/// Saves an edit of the user's `example_plan` with a project selected, on
/// `screen`; returns whether the restart prompt is up.
fn restart_prompt_after_an_edit_saved_on(screen: UiScreen) -> bool {
    let dir = TempDir::new("p9_w1_restart");
    std::fs::copy(
        example_dir().join("example_plan.json"),
        dir.path().join("example_plan.json"),
    )
    .unwrap();
    let (plans, _) = load_plans_from_dir_with_errors(dir.path());
    let plan = plans.get("example_plan").unwrap().clone();

    let mut app = sim_screen_app();
    app.init_resource::<FileDialogTask>()
        .init_resource::<UserPlansDirty>()
        .init_resource::<WorkingDirectorySetupError>()
        .init_resource::<StepCap>()
        .add_message::<SimCommand>()
        .insert_resource(SelectedProject {
            project_id: Some("o3".to_string()),
        })
        .insert_resource(WorkingDirectory {
            path: dir.path().to_string_lossy().to_string(),
            pending_path: String::new(),
        });
    app.world_mut()
        .resource_mut::<NextState<UiScreen>>()
        .set(screen.clone());
    app.update();
    {
        let mut form = app.world_mut().resource_mut::<NewCapturePlanForm>();
        load_plan_into_form(&mut form, "example_plan", &plan, false);
        form.open = true;
        form.phases[2].max_force.text = "4.0".to_string();
    }
    app.world_mut().write_message(UiEvent::SaveCapturePlan);
    app.world_mut().run_system_once(handle_ui_events).unwrap();
    let form = app.world().resource::<NewCapturePlanForm>();
    assert!(!form.open, "{screen:?}: the edit was saved");
    form.show_restart_prompt
}

/// O3 W1.03 (b): the project stays selected on Home after Exit Sim; an edit
/// saved there does not ask to restart a sim that is not running.
#[test]
fn an_edit_saved_on_home_does_not_ask_for_a_restart() {
    assert!(!restart_prompt_after_an_edit_saved_on(UiScreen::Home));
    assert!(restart_prompt_after_an_edit_saved_on(UiScreen::Sim));
}

/// O3 W1.03 (c): the form keeps its scroll position while open, and opens at
/// the top again after it closed.
#[test]
fn the_form_opens_at_the_top_every_time() {
    let mut harness = harness_for(filled_form(), [720.0, 600.0], capture_plan_form_ui);
    let top = harness.get_by_label("General").rect().top();

    harness.get_by_label("Phase · hold").scroll_to_me();
    harness.run();
    harness.run();
    let scrolled = harness.get_by_label("General").rect().top();
    assert!(scrolled < top - 100.0, "scrolled: {top} → {scrolled}");

    form_closed(&harness.ctx);
    harness.run();
    assert_eq!(harness.get_by_label("General").rect().top(), top);
}

/// O3 W1.01: a solid bar, wider than egui's default, its handle shown at rest.
#[test]
fn the_form_scroll_bar_is_solid_and_wide() {
    let style = form_scroll_style();
    assert!(!style.floating);
    assert!(style.bar_width > bevy_egui::egui::style::ScrollStyle::default().bar_width);
    assert!(style.dormant_handle_opacity > 0.0);
}

/// O3 W1.06: each titled section (General, Tether, Capture Sphere, each phase)
/// collapses its rows; the others stay.
#[test]
fn a_section_title_collapses_its_rows() {
    let mut harness = harness_for(filled_form(), [720.0, 4000.0], capture_plan_form_ui);
    harness.get_by_label("Plan Name *");
    harness.get_by_label("General").click();
    harness.run();
    assert!(harness.query_by_label("Plan Name *").is_none());
    harness.get_by_label("Tether Type");

    let maxima = harness.get_all_by_label_contains("Max Velocity").count();
    harness.get_by_label("Phase · rendezvous").click();
    harness.run();
    assert_eq!(
        harness.get_all_by_label_contains("Max Velocity").count(),
        maxima - 1
    );
    harness.get_by_label("General").click();
    harness.run();
    harness.get_by_label("Plan Name *");
}

/// The W1 fix-ups' snapshots (rule 28): the form at the top of a short window
/// with its solid scroll bar, and with every titled section collapsed.
pub(crate) fn p9_w1_snapshots() -> Vec<std::path::PathBuf> {
    vec![
        crate::tests::egui_support::snapshot(
            "p9_w1_form_scroll_bar",
            filled_form(),
            [720.0, 700.0],
            capture_plan_form_ui,
        ),
        crate::tests::egui_support::snapshot_after(
            "p9_w1_form_sections_collapsed",
            filled_form(),
            [720.0, 700.0],
            capture_plan_form_ui,
            |harness| {
                let titles: Vec<String> = ["General", "Tether", "Capture Sphere"]
                    .into_iter()
                    .map(str::to_string)
                    .chain(
                        filled_form()
                            .phases
                            .iter()
                            .map(|p| format!("Phase · {}", p.id)),
                    )
                    .collect();
                for title in titles {
                    // "Tether" is also the tether-type button; the title is first.
                    harness.get_all_by_label(&title).next().unwrap().click();
                    harness.run();
                }
            },
        ),
    ]
}
