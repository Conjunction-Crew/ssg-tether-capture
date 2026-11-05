//! The sim screen's header, sidebar and modals on egui (P8 S5): labels, the
//! section order, buttons → `UiEvent`, the dimmed Start (F-36, P0-004), the
//! readouts in a resource (E1) and Tab staying out of egui's focus (P0-006).

use bevy::ecs::message::Messages;
use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use bevy_egui::EguiInput;
use bevy_egui::egui;
use egui_kittest::kittest::{NodeT, Queryable};

use crate::tests::egui_support::harness_for;
use crate::ui::catalog_state::SpaceCatalogUiState;
use crate::ui::events::UiEvent;
use crate::ui::form::{NewCapturePlanForm, SimPlanSyncState};
use crate::ui::hud::{HudReadouts, init_hud_readouts, update_time_warp_readout};
use crate::ui::input::{keep_tab_from_egui, strip_tab};
use crate::ui::recording::window_pos;
use crate::ui::screen::UiScreen;
use crate::ui::screens::project_detail::modals::{exit_confirm_ui, restart_prompt_ui};
use crate::ui::screens::project_detail::sidebar::{SimScreenView, sim_header_ui, sim_sidebar_ui};
use crate::ui::screens::project_detail::{
    ExitConfirmOpen, sim_screen_events, sim_screen_modals, sim_screen_panels,
};
use ssg_sim::capture::plan::CapturePlanLibrary;
use ssg_sim::orbit::cache::OrbitalCache;
use ssg_sim::orbit::selection::OrbitalSelectionState;
use ssg_sim::project::{SelectedProject, WorkingDirectory};
use ssg_sim::session::{PauseState, SimState};
use ssg_sim::telemetry::LogEvent;
use ssg_sim::time::{SimConfig, WorldTime};

const SIZE: [f32; 2] = [420.0, 900.0];

/// The sidebar before Start, with a user plan and no orbits chosen.
pub(crate) fn setup_view() -> SimScreenView {
    SimScreenView {
        title: "P0 Created".to_string(),
        working_directory: "/Users/walk/wd".to_string(),
        main_file: "p0_created.json".to_string(),
        plan_id: Some("p0_created".to_string()),
        in_sync: true,
        show_satellite_indicator: true,
        time_warp: "1x".to_string(),
        telemetry: "Waiting for live capture telemetry...".to_string(),
        guidance: "Target: Satellite1\nStatus: Idle\nPlan: P0 Created".to_string(),
        ..Default::default()
    }
}

pub(crate) fn running_view() -> SimScreenView {
    SimScreenView {
        running: true,
        can_start: true,
        telemetry: "Target: Satellite1\nCapture status: Idle\nRange to tether root: 120.00 m"
            .to_string(),
        ..setup_view()
    }
}

/// Clicks `label` in `screen` and returns the one event it emitted.
fn event_of(
    view: SimScreenView,
    screen: fn(&mut egui::Ui, &mut SimScreenView, &mut Vec<UiEvent>),
    label: &str,
) -> UiEvent {
    let mut harness = harness_for(view, SIZE, screen);
    harness.get_by_label(label).click();
    harness.run();
    let events = &harness.state().events;
    assert_eq!(events.len(), 1, "{label}: {events:?}");
    events[0].clone()
}

fn section_order(view: SimScreenView, titles: &[&str]) {
    let harness = harness_for(view, SIZE, sim_sidebar_ui);
    let tops: Vec<f32> = titles
        .iter()
        .map(|title| harness.get_by_label(title).rect().top())
        .collect();
    assert!(tops.is_sorted(), "{titles:?} at {tops:?}");
}

#[test]
fn header_shows_the_plan_and_back_asks_to_exit() {
    let mut harness = harness_for(setup_view(), [1280.0, 72.0], sim_header_ui);
    harness.get_by_label("P0 Created");
    harness.get_by_label("Project View");
    harness.get_by_label("Back").click();
    harness.run();
    assert!(matches!(
        harness.state().events.as_slice(),
        [UiEvent::ShowExitConfirm]
    ));
}

#[test]
fn sections_before_start_keep_controls_above_orbital_selection() {
    section_order(
        setup_view(),
        &[
            "Project Information",
            "Simulation Controls",
            "Orbital Selection",
            "Simulation HUD",
            "Reference",
        ],
    );
    let harness = harness_for(setup_view(), SIZE, sim_sidebar_ui);
    assert!(harness.query_by_label("Time Warp").is_none());
}

#[test]
fn sections_after_start_show_time_warp_and_no_orbital_selection() {
    section_order(
        running_view(),
        &[
            "Project Information",
            "Simulation Controls",
            "Time Warp",
            "Simulation HUD",
            "Reference",
        ],
    );
    let harness = harness_for(running_view(), SIZE, sim_sidebar_ui);
    assert!(harness.query_by_label("Orbital Selection").is_none());
}

#[test]
fn project_information_lists_the_files_and_opens_the_plan() {
    let harness = harness_for(setup_view(), SIZE, sim_sidebar_ui);
    harness.get_by_label("Working Directory: /Users/walk/wd");
    harness.get_by_label("Main File: p0_created.json");
    assert!(
        harness
            .query_by_label("Plan changed — sim out of sync")
            .is_none()
    );
    match event_of(setup_view(), sim_sidebar_ui, "View / Edit Plan") {
        UiEvent::EditCapturePlan(id) => assert_eq!(id, "p0_created"),
        event => panic!("{event:?}"),
    }
    let example = SimScreenView {
        is_example_plan: true,
        ..setup_view()
    };
    assert!(matches!(
        event_of(example, sim_sidebar_ui, "View Plan"),
        UiEvent::EditCapturePlan(_)
    ));
}

#[test]
fn sync_indicator_offers_apply_live_and_reset_when_out_of_sync() {
    let stale = || SimScreenView {
        in_sync: false,
        ..setup_view()
    };
    let harness = harness_for(stale(), SIZE, sim_sidebar_ui);
    harness.get_by_label("Plan changed — sim out of sync");
    drop(harness);
    assert!(matches!(
        event_of(stale(), sim_sidebar_ui, "Apply Live"),
        UiEvent::ApplyPlanLive
    ));
    assert!(matches!(
        event_of(stale(), sim_sidebar_ui, "Reset Sim"),
        UiEvent::RestartSim
    ));
}

#[test]
fn start_disabled_until_both_orbits() {
    let mut harness = harness_for(setup_view(), SIZE, sim_sidebar_ui);
    let start = harness.get_by_label("Start Simulation");
    assert!(start.accesskit_node().is_disabled());
    start.click();
    harness.run();
    assert!(harness.state().events.is_empty());

    let ready = SimScreenView {
        can_start: true,
        ..setup_view()
    };
    let harness = harness_for(ready.clone(), SIZE, sim_sidebar_ui);
    assert!(
        !harness
            .get_by_label("Start Simulation")
            .accesskit_node()
            .is_disabled()
    );
    assert!(matches!(
        event_of(ready, sim_sidebar_ui, "Start Simulation"),
        UiEvent::StartSim
    ));
}

#[test]
fn running_controls_emit_their_events() {
    let click = |label| event_of(running_view(), sim_sidebar_ui, label);
    assert!(matches!(click("Map View (M)"), UiEvent::ToggleMapView));
    assert!(matches!(
        click("Hide Satellite Indicator"),
        UiEvent::ToggleSatelliteIndicator
    ));
    assert!(matches!(
        click("Toggle Capture Gizmos (C)"),
        UiEvent::ToggleCaptureGizmos
    ));
    assert!(matches!(
        click("Cycle Target (Tab)"),
        UiEvent::CycleCameraTarget
    ));
    assert!(matches!(click("Reset Sim"), UiEvent::RestartSim));
    assert!(matches!(
        click("<"),
        UiEvent::ChangeTimeWarp { increase: false }
    ));
    assert!(matches!(
        click(">"),
        UiEvent::ChangeTimeWarp { increase: true }
    ));
    match click("Capture") {
        UiEvent::CaptureDebris { plan_id, .. } => assert_eq!(plan_id, "p0_created"),
        event => panic!("{event:?}"),
    }
}

#[test]
fn indicator_button_names_what_it_will_do() {
    let hidden = SimScreenView {
        show_satellite_indicator: false,
        ..running_view()
    };
    let harness = harness_for(hidden, SIZE, sim_sidebar_ui);
    harness.get_by_label("Show Satellite Indicator");
}

#[test]
fn capture_is_disabled_while_a_capture_runs() {
    let active = SimScreenView {
        capture_active: true,
        ..running_view()
    };
    let harness = harness_for(active, SIZE, sim_sidebar_ui);
    assert!(
        harness
            .get_by_label("Capture")
            .accesskit_node()
            .is_disabled()
    );
}

#[test]
fn readouts_show_as_sidebar_labels() {
    let view = SimScreenView {
        time_warp: "64x".to_string(),
        telemetry: "Capture status: Idle".to_string(),
        guidance: "Initial state: approach".to_string(),
        ..running_view()
    };
    let harness = harness_for(view, SIZE, sim_sidebar_ui);
    harness.get_by_label("64x");
    harness.get_by_label("Capture status: Idle");
    harness.get_by_label("Capture Guidance");
    harness.get_by_label("Initial state: approach");
}

#[test]
fn exit_confirm_buttons_emit_their_events() {
    let mut harness = harness_for((), [500.0, 240.0], exit_confirm_ui);
    harness.get_by_label("Exit Simulation?");
    harness.get_by_label("Your orbital simulation will be reset. Are you sure you want to exit?");
    harness.get_by_label("Cancel").click();
    harness.run();
    harness.get_by_label("Exit Sim").click();
    harness.run();
    assert!(matches!(
        harness.state().events.as_slice(),
        [UiEvent::CancelExitConfirm, UiEvent::BackToHome]
    ));
}

#[test]
fn restart_prompt_buttons_emit_their_events() {
    let mut harness = harness_for((), [500.0, 240.0], restart_prompt_ui);
    harness.get_by_label("Plan Changed");
    harness.get_by_label("Continue").click();
    harness.run();
    harness.get_by_label("Apply Live").click();
    harness.run();
    harness.get_by_label("Restart Sim").click();
    harness.run();
    assert!(matches!(
        harness.state().events.as_slice(),
        [
            UiEvent::DismissRestartPrompt,
            UiEvent::ApplyPlanLive,
            UiEvent::RestartSim
        ]
    ));
}

/// An app with what `sim_screen_events` and the egui systems read.
pub(crate) fn sim_screen_app() -> App {
    let mut app = App::new();
    app.add_plugins(StatesPlugin)
        .init_state::<UiScreen>()
        .init_state::<SimState>()
        .init_state::<PauseState>()
        .add_message::<UiEvent>()
        .add_message::<LogEvent>()
        .init_resource::<NewCapturePlanForm>()
        .init_resource::<SimPlanSyncState>()
        .init_resource::<CapturePlanLibrary>()
        .init_resource::<SpaceCatalogUiState>()
        .init_resource::<ExitConfirmOpen>()
        .init_resource::<HudReadouts>()
        .init_resource::<SelectedProject>()
        .init_resource::<WorkingDirectory>()
        .init_resource::<OrbitalSelectionState>()
        .init_resource::<OrbitalCache>()
        .init_resource::<bevy_egui::EguiUserTextures>()
        .insert_resource(SimConfig {
            start_epoch: ssg_sim::test_support::fixed_epoch(),
        });
    app
}

pub(crate) fn send(app: &mut App, event: UiEvent) {
    app.world_mut().write_message(event);
    app.world_mut().run_system_once(sim_screen_events).unwrap();
}

fn logged(app: &App) -> Vec<String> {
    app.world()
        .resource::<Messages<LogEvent>>()
        .iter_current_update_messages()
        .map(|event| event.message.clone())
        .collect()
}

#[test]
fn restart_event_goes_through_home_and_closes_the_prompt() {
    let mut app = sim_screen_app();
    app.world_mut()
        .resource_mut::<NewCapturePlanForm>()
        .show_restart_prompt = true;
    send(&mut app, UiEvent::RestartSim);
    let sync = app.world().resource::<SimPlanSyncState>();
    assert!(sync.restart_requested && sync.in_sync);
    assert!(
        !app.world()
            .resource::<NewCapturePlanForm>()
            .show_restart_prompt
    );
    assert_eq!(logged(&app), ["Simulation restart requested"]);
    assert!(matches!(
        app.world().resource::<NextState<UiScreen>>(),
        NextState::Pending(UiScreen::Home)
    ));
}

#[test]
fn apply_live_resyncs_without_a_restart() {
    let mut app = sim_screen_app();
    {
        let mut sync = app.world_mut().resource_mut::<SimPlanSyncState>();
        sync.in_sync = false;
    }
    send(&mut app, UiEvent::ApplyPlanLive);
    let sync = app.world().resource::<SimPlanSyncState>();
    assert!(sync.in_sync && !sync.restart_requested);
    assert_eq!(
        logged(&app),
        ["Capture plan updated in situ — no restart required"]
    );
}

#[test]
fn continue_leaves_the_sim_out_of_sync() {
    let mut app = sim_screen_app();
    app.world_mut()
        .resource_mut::<NewCapturePlanForm>()
        .show_restart_prompt = true;
    send(&mut app, UiEvent::DismissRestartPrompt);
    assert!(!app.world().resource::<SimPlanSyncState>().in_sync);
    assert!(
        !app.world()
            .resource::<NewCapturePlanForm>()
            .show_restart_prompt
    );
}

#[test]
fn indicator_event_toggles_the_overlay_setting() {
    let mut app = sim_screen_app();
    assert!(
        app.world()
            .resource::<SpaceCatalogUiState>()
            .show_satellite_indicator
    );
    send(&mut app, UiEvent::ToggleSatelliteIndicator);
    assert!(
        !app.world()
            .resource::<SpaceCatalogUiState>()
            .show_satellite_indicator
    );
}

/// The egui systems' parameters are valid together (no context: they return early).
#[test]
fn sim_screen_systems_run_without_a_context() {
    let mut app = sim_screen_app();
    let world = app.world_mut();
    world.run_system_once(sim_screen_panels).unwrap();
    world.resource_mut::<ExitConfirmOpen>().0 = true;
    world.run_system_once(sim_screen_modals).unwrap();
}

#[test]
fn time_warp_readout_is_written_to_the_resource() {
    let mut app = sim_screen_app();
    let mut time = WorldTime::starting_at(ssg_sim::test_support::fixed_epoch());
    time.multiplier = 64;
    time.effective_multiplier = 64;
    app.world_mut().insert_resource(time);
    app.update();
    app.world_mut()
        .run_system_once(update_time_warp_readout)
        .unwrap();
    assert_eq!(app.world().resource::<HudReadouts>().time_warp, "64x");
}

#[test]
fn readouts_reset_with_the_session() {
    let mut app = sim_screen_app();
    app.world_mut().resource_mut::<HudReadouts>().telemetry = "old".to_string();
    app.world_mut().run_system_once(init_hud_readouts).unwrap();
    let readouts = app.world().resource::<HudReadouts>();
    assert_eq!(readouts.telemetry, "Waiting for live capture telemetry...");
    assert_eq!(readouts.target_label, "Satellite1");
}

fn tab_event() -> egui::Event {
    egui::Event::Key {
        key: egui::Key::Tab,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }
}

/// One egui pass of the sidebar over `input`, returning whether a widget has focus.
fn focused_after(ctx: &egui::Context, input: egui::RawInput) -> bool {
    let _ = ctx.run(input, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            sim_sidebar_ui(ui, &mut running_view(), &mut Vec::new());
        });
    });
    ctx.memory(|memory| memory.focused().is_some())
}

/// P0-006: unfiltered, Tab gives a sidebar button the focus (and the hotkeys
/// would then be ignored); filtered, nothing takes it.
#[test]
fn tab_cycles_camera_not_focus() {
    let raw = || egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, SIZE.into())),
        events: vec![tab_event()],
        ..Default::default()
    };

    let unfiltered = egui::Context::default();
    let _ = focused_after(&unfiltered, egui::RawInput::default());
    let mut focused = false;
    for _ in 0..3 {
        focused |= focused_after(&unfiltered, raw());
    }
    assert!(focused, "the test bites: egui takes Tab when it sees it");

    let filtered = egui::Context::default();
    let _ = focused_after(&filtered, egui::RawInput::default());
    for _ in 0..3 {
        let mut input = raw();
        strip_tab(&mut input);
        assert!(!focused_after(&filtered, input));
    }
}

#[test]
fn tab_is_removed_from_the_egui_input() {
    let mut world = World::new();
    let mut input = EguiInput::default();
    input.0.events.push(tab_event());
    input.0.events.push(egui::Event::Text("m".to_string()));
    let entity = world.spawn(input).id();
    world.run_system_once(keep_tab_from_egui).unwrap();
    let events = &world.get::<EguiInput>(entity).unwrap().0.events;
    assert_eq!(events, &[egui::Event::Text("m".to_string())]);
}

/// A click on a sidebar button leaves no widget focused, so the hotkeys keep working.
#[test]
fn clicking_a_button_keeps_the_hotkeys() {
    let mut harness = harness_for(running_view(), SIZE, sim_sidebar_ui);
    harness.get_by_label("Map View (M)").click();
    harness.run();
    assert!(!harness.ctx.wants_keyboard_input());
}

#[test]
fn p7_windows_open_beside_the_panels() {
    // Catalog closed: below the header, right of the toggle (12 + 156 px).
    let closed = egui::Rect::from_min_max(egui::pos2(0.0, 72.0), egui::pos2(860.0, 600.0));
    assert_eq!(window_pos(closed), egui::pos2(180.0, 84.0));
    // Catalog open: the panel's 364 px are not in the free space.
    let open = egui::Rect::from_min_max(egui::pos2(364.0, 72.0), egui::pos2(860.0, 600.0));
    assert!(open.contains(window_pos(open)));
}
