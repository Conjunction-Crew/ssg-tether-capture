//! The display items (P8 S7): the units toggle (OQ3), the epoch and sim time, the
//! running warp and the lowered request (F-46, OQ2), the plots' fresh view (N8)
//! and the export failures (F-33).

use std::path::{Path, PathBuf};
use std::time::Duration;

use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use bevy_egui::egui;
use egui_kittest::kittest::Queryable;
use egui_plot::PlotMemory;
use ssg_sim::capture::plan::{
    CapturePlanLibrary, CompiledCapturePlan, CompiledCaptureState, CompiledCaptureStateParameters,
    CompiledCaptureTransition,
};
use ssg_sim::capture::{CaptureComponent, CaptureSphereRadius};
use ssg_sim::orbit::cache::OrbitalCache;
use ssg_sim::orbit::selection::OrbitalSelectionState;
use ssg_sim::project::{WorkingDirectory, load_plans_from_dir_with_errors};
use ssg_sim::session::PauseState;
use ssg_sim::telemetry::LogLevel;
use ssg_sim::test_support::{TempDir, fixed_epoch};
use ssg_sim::time::{StepCap, WorldTime};

use crate::tests::app_support::{LogTap, enter_sim, start_running, test_app};
use crate::tests::egui_support::{harness_for, snapshot};
use crate::tests::plan_form::{filled_form, new_form};
use crate::tests::sim_screen::{running_view, setup_view, sim_screen_app};
use crate::ui::egui::{
    CsvExportState, PlotsView, begin_plot_session, egui_plots, export_csv, export_error_ui,
    export_oem, plot_id, plot_line_names, plots_ui, record_export,
};
use crate::ui::events::UiEvent;
use crate::ui::form::{NewCapturePlanForm, UnitSystem, show};
use crate::ui::hud::{
    CaptureMetrics, HudReadouts, append_state_parameters, append_transitions, sim_time_text,
    telemetry_text, update_capture_guidance, update_capture_telemetry, update_time_warp_readout,
    utc_seconds,
};
use crate::ui::input::change_time_warp;
use crate::ui::plugin::load_plan_into_form;
use crate::ui::recording::adapt_step_cap;
use crate::ui::screens::capture_plan::build_capture_plan_json;
use crate::ui::screens::project_detail::sidebar::{SimScreenView, sim_sidebar_ui};
use crate::ui::settings::{Settings, set_unit_system};

const SIZE: [f32; 2] = [420.0, 1100.0];

fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-6, "{actual} != {expected}");
}

fn metrics() -> CaptureMetrics {
    CaptureMetrics {
        range_m: 100.0,
        relative_speed_m_s: 2.0,
        closing_speed_m_s: 1.0,
        target_speed_m_s: 7660.0,
        target_altitude_m: 400_000.0,
    }
}

fn phase() -> CompiledCaptureState {
    CompiledCaptureState {
        id: "terminal".to_string(),
        parameters: CompiledCaptureStateParameters {
            max_velocity: 1.0,
            max_force: 4.44822,
            shrink_rate: Some(0.3048),
            ..Default::default()
        },
        transitions: vec![CompiledCaptureTransition {
            to: "capture".to_string(),
            distance_less_than: Some(50.0),
            relative_velocity_greater_than: Some(3.0),
            ..Default::default()
        }],
        ..Default::default()
    }
}

#[test]
fn units_convert_every_readout() {
    type Quantity = fn(UnitSystem, f64) -> (f64, &'static str);
    let table: [(Quantity, f64, &str, f64, &str); 4] = [
        (UnitSystem::length, 100.0, "m", 328.083_989_5, "ft"),
        (UnitSystem::speed, 1.0, "m/s", 3.280_839_895, "ft/s"),
        (UnitSystem::force, 4.44822, "N", 1.0, "lbf"),
        (UnitSystem::long_length, 1.609344, "km", 1.0, "mi"),
    ];
    for (quantity, metric, metric_unit, imperial, imperial_unit) in table {
        let (value, unit) = quantity(UnitSystem::Metric, metric);
        assert_eq!((value, unit), (metric, metric_unit));
        let (value, unit) = quantity(UnitSystem::Imperial, metric);
        close(value, imperial);
        assert_eq!(unit, imperial_unit);
    }

    // The telemetry: metric is rc.3's text, imperial converts every line.
    let telemetry = |units| telemetry_text("Satellite1", "Idle", &metrics(), 25.0, units);
    assert_eq!(
        telemetry(UnitSystem::Metric),
        "Target: Satellite1\nCapture status: Idle\nRange to tether root: 100.00 m\n\
         Relative speed: 2.00 m/s\nClosing rate: 1.00 m/s\nInside capture sphere: No\n\
         Capture sphere radius: 25.00 m\nTarget altitude: 400000.0 m\nTarget speed: 7660.00 m/s"
    );
    assert_eq!(
        telemetry(UnitSystem::Imperial),
        "Target: Satellite1\nCapture status: Idle\nRange to tether root: 328.08 ft\n\
         Relative speed: 6.56 ft/s\nClosing rate: 3.28 ft/s\nInside capture sphere: No\n\
         Capture sphere radius: 82.02 ft\nTarget altitude: 1312336.0 ft\n\
         Target speed: 25131.23 ft/s"
    );

    // The guidance's phase parameters.
    let parameters = |units| {
        let mut body = String::new();
        append_state_parameters(&mut body, &phase(), units);
        body
    };
    assert_eq!(
        parameters(UnitSystem::Metric),
        "Phase parameters\n- max_velocity: 1.0000\n- max_force_n (bound): 4.4482\n\
         - shrink_rate: 0.3048\n"
    );
    assert_eq!(
        parameters(UnitSystem::Imperial),
        "Phase parameters\n- max_velocity: 3.2808 ft/s\n- max_force_lbf (bound): 1.0000\n\
         - shrink_rate: 1.0000 ft/s\n"
    );

    // The guidance's transitions.
    let transitions = |units| {
        let mut body = String::new();
        append_transitions(&mut body, &phase(), Some(100.0), Some(2.0), true, units);
        body
    };
    assert_eq!(
        transitions(UnitSystem::Metric),
        "Possible transitions\n- capture when distance < 50.00 m (current 100.00 m, waiting) \
         and relative velocity > 3.00 m/s (current 2.00 m/s, waiting)\n"
    );
    assert_eq!(
        transitions(UnitSystem::Imperial),
        "Possible transitions\n- capture when distance < 164.04 ft (current 328.08 ft, waiting) \
         and relative velocity > 9.84 ft/s (current 6.56 ft/s, waiting)\n"
    );

    // The plots.
    assert_eq!(
        plot_line_names(UnitSystem::Metric),
        ("meters", "meters per second")
    );
    assert_eq!(
        plot_line_names(UnitSystem::Imperial),
        ("feet", "feet per second")
    );
}

/// Readiness is decided on the metric values, whatever the display says.
#[test]
fn readiness_does_not_depend_on_the_units() {
    // 100 m is under a 150 m limit; comparing the shown 328 ft with it would not be.
    let mut state = phase();
    state.transitions[0].relative_velocity_greater_than = None;
    state.transitions[0].distance_less_than = Some(150.0);
    let text = |units| {
        let mut body = String::new();
        append_transitions(&mut body, &state, Some(100.0), None, false, units);
        body
    };
    assert!(text(UnitSystem::Metric).contains("ready"));
    assert!(text(UnitSystem::Imperial).contains("ready"));
    assert!(text(UnitSystem::Imperial).contains("492.13 ft"));
}

#[test]
fn slider_label_shows_miles_and_the_event_stays_in_km() {
    use crate::ui::screens::project_detail::orbital_selection::{OrbitalView, custom_orbit};
    let view = |unit_system| SimScreenView {
        orbital: OrbitalView {
            target: Some(custom_orbit(
                crate::ui::catalog_state::OrbitalSelectionRole::Target,
                fixed_epoch(),
            )),
            unit_system,
            ..Default::default()
        },
        unit_system,
        ..setup_view()
    };
    let km = harness_for(view(UnitSystem::Metric), SIZE, sim_sidebar_ui);
    assert!(km.query_by_label_contains(" km").is_some());
    assert!(km.query_by_label_contains(" mi").is_none());
    let mi = harness_for(view(UnitSystem::Imperial), SIZE, sim_sidebar_ui);
    assert!(mi.query_by_label_contains(" mi").is_some());
    assert!(mi.query_by_label_contains(" km").is_none());
    assert!(
        mi.state().events.is_empty(),
        "changing the label moves no slider"
    );
}

#[test]
fn hud_toggle_emits_the_form_event() {
    let mut harness = harness_for(running_view(), SIZE, sim_sidebar_ui);
    harness.get_by_label("ft (imperial)").click();
    harness.run();
    assert!(matches!(
        harness.state().events.as_slice(),
        [UiEvent::SetUnitSystem(UnitSystem::Imperial)]
    ));
}

/// The one toggle, from either place, and what stays after the form closes.
#[test]
fn the_toggle_is_shared_with_the_form() {
    let mut settings = Settings::default();
    let mut form = new_form();
    assert_eq!(settings.unit_system, UnitSystem::Metric, "metric at launch");

    set_unit_system(&mut settings, &mut form, UnitSystem::Imperial);
    assert_eq!(settings.unit_system, UnitSystem::Imperial);
    assert_eq!(form.unit_system, UnitSystem::Imperial);

    form.reset();
    assert_eq!(
        form.unit_system,
        UnitSystem::Imperial,
        "the form reopens on it"
    );
}

/// A new plan's default tether is 20 m in either system.
#[test]
fn the_default_tether_is_twenty_metres_in_imperial() {
    let mut form = NewCapturePlanForm {
        unit_system: UnitSystem::Imperial,
        ..Default::default()
    };
    form.reset();
    assert_eq!(form.tether_length, "65.6167979003");
    let json = build_capture_plan_json(&form);
    assert_eq!(json["device"]["tether_length"].as_f64(), Some(20.0));
}

/// Toggling keeps every number's physical value: the same JSON comes out, and a
/// field that is not a number stays as typed for Save to report.
#[test]
fn files_stay_metric_whatever_the_toggle() {
    let mut form = filled_form();
    form.terminal_transitions = vec![crate::ui::form::TransitionForm {
        to: "capture".to_string(),
        distance_kind: "less_than".to_string(),
        distance_value: "12.5".to_string(),
    }];
    let metric = build_capture_plan_json(&form);

    form.set_unit_system(UnitSystem::Imperial);
    assert_eq!(form.approach_max_velocity, "6.56167979");
    assert_eq!(form.approach_max_force, "0.6744270742");
    assert_eq!(
        form.approach_transitions[0].distance_value,
        "164.0419947507"
    );
    assert_eq!(build_capture_plan_json(&form), metric);

    form.set_unit_system(UnitSystem::Metric);
    assert_eq!(form.approach_max_velocity, "2.0");
    assert_eq!(build_capture_plan_json(&form), metric);

    form.approach_max_velocity = "abc".to_string();
    form.terminal_max_force = String::new();
    form.set_unit_system(UnitSystem::Imperial);
    assert_eq!(form.approach_max_velocity, "abc");
    assert_eq!(form.terminal_max_force, "");
}

/// A plan opened in imperial shows converted numbers and writes the same file
/// as in metric, and an unchanged Save still writes nothing (no double conversion).
#[test]
fn a_plan_opened_in_imperial_saves_the_same_numbers() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/example_capture_plans");
    let (plans, errors) = load_plans_from_dir_with_errors(&dir);
    assert!(errors.is_empty(), "{errors:?}");
    assert!(!plans.is_empty());
    for (plan_id, plan) in &plans {
        let mut metric = NewCapturePlanForm::default();
        load_plan_into_form(&mut metric, plan_id, plan, false);
        let expected = build_capture_plan_json(&metric);

        let mut imperial = NewCapturePlanForm {
            unit_system: UnitSystem::Imperial,
            ..Default::default()
        };
        load_plan_into_form(&mut imperial, plan_id, plan, false);
        assert_eq!(imperial.unit_system, UnitSystem::Imperial, "{plan_id}");
        assert_ne!(
            imperial.approach_max_velocity, metric.approach_max_velocity,
            "{plan_id}: the form shows converted numbers"
        );
        assert_eq!(build_capture_plan_json(&imperial), expected, "{plan_id}");
        assert_eq!(
            imperial.original_json.as_ref(),
            Some(&expected),
            "{plan_id}: an unchanged Save writes nothing"
        );

        // Toggled after opening: still unchanged.
        imperial.set_unit_system(UnitSystem::Metric);
        assert_eq!(build_capture_plan_json(&imperial), expected, "{plan_id}");
    }
}

#[test]
fn epoch_and_sim_time_are_shown() {
    let start = fixed_epoch();
    let text = sim_time_text(start + 3661.5, start);
    assert!(text.starts_with("Sim time: "), "{text}");
    assert!(text.ends_with(" (T+1:01:01)"), "{text}");
    assert_eq!(utc_seconds(start).len(), "2026-11-04T01:52:08Z".len());
    assert!(utc_seconds(start).ends_with('Z'));
    assert!(sim_time_text(start, start).ends_with(" (T+0:00:00)"));
    // Hours do not wrap.
    assert!(sim_time_text(start + 360_000.0, start).contains("(T+100:00:00)"));

    let view = SimScreenView {
        start_epoch: "2026-11-04T01:52:08Z".to_string(),
        sim_time: "Sim time: 2026-11-04T02:53:09Z (T+1:01:01)".to_string(),
        ..running_view()
    };
    let harness = harness_for(view, SIZE, sim_sidebar_ui);
    harness.get_by_label("Sim start epoch: 2026-11-04T01:52:08Z");
    harness.get_by_label("Sim time: 2026-11-04T02:53:09Z (T+1:01:01)");
}

#[test]
fn hud_and_reference_say_phase() {
    let harness = harness_for(running_view(), SIZE, sim_sidebar_ui);
    assert!(
        harness
            .query_by_label_contains("currently active phase")
            .is_some()
    );

    let mut app = sim_screen_app();
    let plan_id = "p".to_string();
    let plan = CompiledCapturePlan {
        states: vec![phase()],
        state_indices: [("terminal".to_string(), 0)].into_iter().collect(),
        ..Default::default()
    };
    app.init_resource::<OrbitalCache>()
        .insert_resource(CaptureSphereRadius { radius: 25.0 });
    app.world_mut()
        .resource_mut::<CapturePlanLibrary>()
        .compiled_plans
        .insert(plan_id.clone(), plan);
    app.world_mut().resource_mut::<HudReadouts>().plan_id = plan_id.clone();
    app.world_mut()
        .run_system_once(update_capture_guidance)
        .unwrap();
    let idle = app.world().resource::<HudReadouts>().guidance.clone();
    assert!(idle.contains("Initial phase: terminal"), "{idle}");
    assert!(!idle.to_lowercase().contains("state"), "{idle}");

    let target = app
        .world_mut()
        .spawn(CaptureComponent {
            plan_id,
            current_state: "terminal".to_string(),
            state_enter_time_s: 0.0,
            state_elapsed_time_s: 2.0,
            sphere_radius_at_entry_m: 25.0,
        })
        .id();
    app.world_mut().resource_mut::<HudReadouts>().target_entity = Some(target);
    app.world_mut()
        .run_system_once(update_capture_guidance)
        .unwrap();
    let active = app.world().resource::<HudReadouts>().guidance.clone();
    assert!(active.contains("Current phase: terminal"), "{active}");
    assert!(active.contains("Time in phase: 2.0 s"), "{active}");
    assert!(active.contains("Phase parameters"), "{active}");
    assert!(!active.to_lowercase().contains("state"), "{active}");
}

fn warp_readout(multiplier: u32, effective: u32, pause: PauseState) -> String {
    let mut app = sim_screen_app();
    app.insert_resource(WorldTime {
        multiplier,
        effective_multiplier: effective,
        ..WorldTime::starting_at(fixed_epoch())
    });
    app.world_mut()
        .resource_mut::<NextState<PauseState>>()
        .set(pause);
    app.update();
    app.world_mut()
        .run_system_once(update_time_warp_readout)
        .unwrap();
    app.world().resource::<HudReadouts>().time_warp.clone()
}

/// F-46, OQ2: one line, the warp that runs; no "effective" note.
#[test]
fn readout_shows_running_warp() {
    assert_eq!(warp_readout(128, 64, PauseState::Running), "64x");
    assert_eq!(warp_readout(64, 64, PauseState::Running), "64x");
    assert_eq!(warp_readout(128, 64, PauseState::Paused), "Paused");
    // Before the first fixed step, the request.
    assert_eq!(warp_readout(8, 0, PauseState::Running), "8x");
    // A request just lowered is not overstated by the last run's count.
    assert_eq!(warp_readout(16, 32, PauseState::Running), "16x");
}

/// `adapt_step_cap` over a run of `frame_ms` frames that run at the cap, in an app
/// with the given warp; returns the app and the Info lines logged by "ui".
fn adapt(multiplier: u32, frame_ms: u64, frames: usize) -> (App, Vec<String>) {
    let mut app = test_app();
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
        frame_ms,
    )))
    .insert_resource(WorldTime {
        multiplier,
        effective_multiplier: multiplier.min(StepCap::default().0),
        ..WorldTime::starting_at(fixed_epoch())
    })
    .add_systems(Update, adapt_step_cap);
    let mut tap = LogTap::new(app.world());
    app.update();
    let mut messages = Vec::new();
    for _ in 0..frames {
        let world = app.world_mut();
        let cap = world.resource::<StepCap>().0;
        let mut time = world.resource_mut::<WorldTime>();
        time.effective_multiplier = time.multiplier.min(cap);
        app.update();
        messages.extend(
            tap.drain(app.world())
                .into_iter()
                .filter(|(level, source, _)| *level == LogLevel::Info && *source == "ui")
                .map(|(_, _, message)| message),
        );
    }
    (app, messages)
}

/// F-46, OQ2: when the cap falls below the request, the request follows and says so.
#[test]
fn warp_lowered_to_cap() {
    // 60 ms frames: the cap halves after half a second (nine frames).
    let (app, messages) = adapt(64, 60, 9);
    assert_eq!(app.world().resource::<StepCap>().0, 32);
    assert_eq!(app.world().resource::<WorldTime>().multiplier, 32);
    assert_eq!(messages, ["Time warp → 32x (machine limit)"]);

    // A request of 2 × the cap comes down to the new cap, not the old one.
    let (app, messages) = adapt(128, 60, 9);
    assert_eq!(app.world().resource::<StepCap>().0, 32);
    assert_eq!(app.world().resource::<WorldTime>().multiplier, 32);
    assert_eq!(messages, ["Time warp → 32x (machine limit)"]);
}

#[test]
fn warp_below_the_cap_is_left_alone() {
    // Below the cap the warp does not run at the cap, so the cap never halves.
    let (app, messages) = adapt(8, 60, 30);
    assert_eq!(app.world().resource::<StepCap>().0, 64);
    assert_eq!(app.world().resource::<WorldTime>().multiplier, 8);
    assert!(messages.is_empty(), "{messages:?}");
}

/// A request of 2 × the cap stays, and lets the cap grow on a fast machine.
#[test]
fn asking_for_twice_the_cap_lets_the_cap_grow() {
    let (app, messages) = adapt(128, 10, 60);
    assert_eq!(app.world().resource::<StepCap>().0, 128);
    assert_eq!(app.world().resource::<WorldTime>().multiplier, 128);
    assert!(messages.is_empty(), "{messages:?}");
}

/// F-46, OQ2: `.` asks for at most 2 × the cap; the top of the ladder needs a cap
/// that high. A press past the limit changes nothing and logs nothing.
#[test]
fn period_asks_for_at_most_twice_the_cap() {
    let mut app = test_app();
    enter_sim(&mut app);
    start_running(&mut app);
    let mut press = |key| {
        let mut tap = LogTap::new(app.world());
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(key);
        app.world_mut().run_system_once(change_time_warp).unwrap();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .reset_all();
        (
            app.world().resource::<WorldTime>().multiplier,
            tap.drain(app.world()).len(),
        )
    };
    let mut up = Vec::new();
    for _ in 0..7 {
        up.push(press(KeyCode::Period).0);
    }
    assert_eq!(up, [2, 4, 8, 16, 32, 64, 128]);
    assert_eq!(press(KeyCode::Period), (128, 0), "past 2 × the default cap");
    assert_eq!(press(KeyCode::Comma).0, 64);
}

const PLOT_SIZE: [f32; 2] = [500.0, 700.0];
const SAMPLES: [(f64, f64); 3] = [(0.0, 100.0), (1.0, 90.0), (2.0, 80.0)];

/// N8: the view of the plots belongs to the session. A panned plot stays panned in
/// its session; the next session's plot starts on automatic bounds.
#[test]
fn plot_resets_on_reopen() {
    let screen = |ui: &mut egui::Ui, session: &mut u32, _: &mut Vec<UiEvent>| {
        plots_ui(
            ui,
            &PlotsView {
                position: &SAMPLES,
                velocity: &SAMPLES,
                units: UnitSystem::Metric,
                session: *session,
            },
        );
    };
    let id = |session| plot_id("Relative Distance", session, UnitSystem::Metric);
    let mut harness = harness_for(1_u32, PLOT_SIZE, screen);
    let ctx = harness.ctx.clone();

    // The operator scrolls the plot (egui_plot records it as auto bounds off).
    let mut memory = PlotMemory::load(&ctx, id(1)).expect("the plot remembers its view");
    assert!(memory.auto_bounds.x, "a new plot fits its data");
    memory.auto_bounds = false.into();
    memory.store(&ctx, id(1));
    harness.run();
    assert!(
        !PlotMemory::load(&ctx, id(1)).unwrap().auto_bounds.x,
        "the same session keeps the scroll (what N8 saw across sessions)"
    );

    // The project is re-opened.
    harness.state_mut().view = 2;
    harness.run();
    assert!(
        PlotMemory::load(&ctx, id(2)).unwrap().auto_bounds.x,
        "the new session starts from a fresh view"
    );

    // A units change does too: the old view does not fit the converted numbers.
    assert_ne!(
        plot_id("Relative Distance", 2, UnitSystem::Metric),
        plot_id("Relative Distance", 2, UnitSystem::Imperial)
    );
}

/// The changed systems' parameters are valid together: no test boots `UiPlugin`.
#[test]
fn display_systems_run_without_a_capture_or_a_context() {
    let mut app = sim_screen_app();
    app.init_resource::<CsvExportState>()
        .init_resource::<ssg_sim::telemetry::DataCollection>()
        .insert_resource(CaptureSphereRadius { radius: 25.0 });
    let world = app.world_mut();
    world.run_system_once(egui_plots).unwrap();
    world.run_system_once(update_capture_telemetry).unwrap();
    let telemetry = &world.resource::<HudReadouts>().telemetry;
    assert!(telemetry.contains("Waiting for live capture telemetry"));
}

#[test]
fn entering_the_sim_begins_a_plot_session() {
    let mut app = sim_screen_app();
    app.init_resource::<CsvExportState>();
    app.world_mut()
        .resource_mut::<CsvExportState>()
        .export_error = Some("old".to_string());
    app.world_mut().run_system_once(begin_plot_session).unwrap();
    let first = app.world().resource::<CsvExportState>().plot_session;
    app.world_mut().run_system_once(begin_plot_session).unwrap();
    let state = app.world().resource::<CsvExportState>();
    assert_ne!(state.plot_session, first);
    assert_eq!(state.export_error, None);
}

/// A working directory whose `exports` is a file, so every export fails.
fn broken_exports(name: &str) -> (TempDir, WorkingDirectory) {
    let dir = TempDir::new(name);
    std::fs::write(dir.path().join("exports"), "in the way").unwrap();
    let path = dir.path().to_string_lossy().into_owned();
    let working_directory = WorkingDirectory {
        path: path.clone(),
        pending_path: path,
    };
    (dir, working_directory)
}

/// F-33: a failed export is shown in the window and logged, and the next good
/// export clears it.
#[test]
fn export_failures_are_shown_and_logged() {
    let (_dir, working_directory) = broken_exports("p8_export_failure");
    let csv = export_csv(&working_directory, "a.csv", &SAMPLES, &SAMPLES).map(|_| ());
    assert!(csv.is_err());
    let world_time = WorldTime::starting_at(fixed_epoch());
    let oem = export_oem(
        &working_directory,
        "a.oem",
        &OrbitalSelectionState::default(),
        Some(&world_time),
        None,
    );
    assert!(oem.is_err());

    let mut app = test_app();
    app.init_resource::<CsvExportState>();
    let mut tap = LogTap::new(app.world());
    for (kind, outcome) in [("CSV", csv), ("OEM", oem)] {
        let shown = app
            .world_mut()
            .run_system_once(
                move |mut state: ResMut<CsvExportState>,
                      mut log: MessageWriter<ssg_sim::telemetry::LogEvent>| {
                    record_export(&mut state, &mut log, kind, outcome.clone());
                    state.export_error.clone()
                },
            )
            .unwrap()
            .expect("the failure is kept for the window");
        assert!(
            shown.starts_with(&format!("{kind} export failed: ")),
            "{shown}"
        );
        assert!(shown.contains("exports"), "{shown}");

        let harness = harness_for(shown.clone(), PLOT_SIZE, |ui, error, _| {
            export_error_ui(ui, Some(error.as_str()));
        });
        harness.get_by_label(&shown);

        let logged = tap.drain(app.world());
        assert!(
            logged
                .iter()
                .any(|(level, _, message)| *level == LogLevel::Error && *message == shown),
            "{logged:?}"
        );
    }

    app.world_mut()
        .run_system_once(
            |mut state: ResMut<CsvExportState>,
             mut log: MessageWriter<ssg_sim::telemetry::LogEvent>| {
                record_export(&mut state, &mut log, "CSV", Ok(()));
            },
        )
        .unwrap();
    assert_eq!(app.world().resource::<CsvExportState>().export_error, None);
}

#[test]
fn exports_still_work() {
    let dir = TempDir::new("p8_export_ok");
    let path = dir.path().to_string_lossy().into_owned();
    let working_directory = WorkingDirectory {
        path: path.clone(),
        pending_path: path,
    };
    let path = export_csv(&working_directory, "a.csv", &SAMPLES, &SAMPLES).unwrap();
    assert!(path.is_file());
    assert_eq!(path.parent().unwrap().file_name().unwrap(), "exports");
}

/// The S7 PNGs for `p8_screen_snapshots`.
pub(crate) fn s7_snapshots() -> Vec<PathBuf> {
    use crate::ui::screens::capture_plan::window::capture_plan_form_ui;
    let hud = |unit_system| SimScreenView {
        start_epoch: "2026-11-04T01:52:08Z".to_string(),
        unit_system,
        time_warp: "64x".to_string(),
        sim_time: "Sim time: 2026-11-04T02:53:09Z (T+1:01:01)".to_string(),
        telemetry: telemetry_text("Satellite1", "Idle", &metrics(), 25.0, unit_system),
        guidance: {
            let mut body = String::from("Target: Satellite1\nCurrent phase: terminal\n");
            body.push_str(&format!(
                "Time in phase: 2.0 s\nCapture sphere radius: {}\n\n",
                show(unit_system.length(25.0), 2)
            ));
            append_state_parameters(&mut body, &phase(), unit_system);
            body.push('\n');
            append_transitions(
                &mut body,
                &phase(),
                Some(100.0),
                Some(2.0),
                true,
                unit_system,
            );
            body
        },
        ..running_view()
    };
    let mut imperial_form = filled_form();
    imperial_form.set_unit_system(UnitSystem::Imperial);
    vec![
        snapshot(
            "s7_sidebar_metric",
            hud(UnitSystem::Metric),
            [420.0, 1300.0],
            sim_sidebar_ui,
        ),
        snapshot(
            "s7_sidebar_imperial",
            hud(UnitSystem::Imperial),
            [420.0, 1300.0],
            sim_sidebar_ui,
        ),
        snapshot(
            "s7_form_imperial",
            imperial_form,
            [720.0, 1100.0],
            capture_plan_form_ui,
        ),
        snapshot(
            "s7_plots_export_error",
            "CSV export failed: cannot create /Users/walk/wd/exports: Read-only file system"
                .to_string(),
            PLOT_SIZE,
            |ui, error, _| {
                plots_ui(
                    ui,
                    &PlotsView {
                        position: &SAMPLES,
                        velocity: &SAMPLES,
                        units: UnitSystem::Imperial,
                        session: 1,
                    },
                );
                export_error_ui(ui, Some(error.as_str()));
            },
        ),
    ]
}
