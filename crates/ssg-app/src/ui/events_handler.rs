//! `handle_ui_events` and what it calls: the app's response to the screens'
//! `UiEvent`s. Split from `ui/plugin.rs` (Q34), behaviour unchanged.

use std::path::{Path, PathBuf};

use avian3d::prelude::RigidBody;
use bevy::camera::visibility::RenderLayers;
use bevy::ecs::system::SystemParam;
use bevy::pbr::{Atmosphere, AtmosphereSettings};
use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, block_on, futures_lite::future};

use crate::render::camera::{CameraTarget, cycle_camera_target, set_map_view};
use crate::render::layers::MAP_LAYER;
use crate::ui::events::UiEvent;
use crate::ui::form::{NewCapturePlanForm, SimPlanSyncState};
use crate::ui::input::step_time_warp;
use crate::ui::log_ui::CapturePlanLoadErrors;
use crate::ui::plugin::UiCamera;
use crate::ui::screen::UiScreen;
use crate::ui::screens::capture_plan::{
    NEW_PLAN_TEMPLATE, load_plan_into_form, new_plan_into_form, save_plan_from_form,
};
use crate::ui::screens::project_detail::ExitConfirmOpen;
use crate::ui::screens::project_detail::orbital_selection::place_chaser_relative;
use crate::ui::screens::working_directory_setup::WorkingDirectorySetupError;
use crate::ui::settings::{Settings, set_unit_system};
use ssg_sim::capture::SimCommand;
use ssg_sim::capture::plan::CapturePlanLibrary;
use ssg_sim::orbit::Orbital;
use ssg_sim::orbit::selection::{OrbitalSelectionState, SelectedOrbitalObject};
use ssg_sim::project::manifest::{
    MANIFEST_FILE, ProjectManifest, read_manifest_file, read_sim_inputs, save_and_reload,
};
use ssg_sim::project::{
    SelectedProject, WorkingDirectory, load_plans_from_dir_with_errors, now_utc,
    set_working_directory,
};
use ssg_sim::session::{PauseState, SimState};
use ssg_sim::telemetry::{LogEvent, LogLevel};
use ssg_sim::time::{SimConfig, StepCap, WorldTime};

#[derive(Resource, Default)]
pub(crate) struct FileDialogTask(Option<Task<Option<PathBuf>>>);

#[derive(Resource, Default)]
pub(crate) struct UserPlansDirty(bool);

#[derive(SystemParam)]
pub(crate) struct UiFlowState<'w> {
    plans_dirty: ResMut<'w, UserPlansDirty>,
    exit_confirm: ResMut<'w, ExitConfirmOpen>,
    setup_error: ResMut<'w, WorkingDirectorySetupError>,
}

/// What `handle_ui_events` reads and writes of the session: the screens, the
/// project, the plans and the form.
#[derive(SystemParam)]
pub(crate) struct UiEventSession<'w> {
    next_screen: ResMut<'w, NextState<UiScreen>>,
    next_sim_state: ResMut<'w, NextState<SimState>>,
    selected_project: ResMut<'w, SelectedProject>,
    working_directory: ResMut<'w, WorkingDirectory>,
    file_dialog_task: ResMut<'w, FileDialogTask>,
    capture_plan_lib: ResMut<'w, CapturePlanLibrary>,
    orbital_selection: ResMut<'w, OrbitalSelectionState>,
    sim_config: ResMut<'w, SimConfig>,
    form: ResMut<'w, NewCapturePlanForm>,
    settings: ResMut<'w, Settings>,
}

type SceneCameraData = (
    &'static mut RenderLayers,
    &'static mut Atmosphere,
    &'static mut AtmosphereSettings,
);
type CameraBodyFilter = (With<RigidBody>, With<Orbital>);

/// What `handle_ui_events` reads and writes of the running scene: the camera, the
/// bodies it can follow, and the clock.
#[derive(SystemParam)]
pub(crate) struct UiEventScene<'w, 's> {
    scene_camera: Query<'w, 's, SceneCameraData, Without<UiCamera>>,
    bodies: Query<'w, 's, (Entity, Has<CameraTarget>), CameraBodyFilter>,
    world_time: Option<ResMut<'w, WorldTime>>,
    pause: Res<'w, State<PauseState>>,
    next_pause: ResMut<'w, NextState<PauseState>>,
    step_cap: Res<'w, StepCap>,
}

pub(crate) fn poll_file_dialog_task(
    mut file_dialog_task: ResMut<FileDialogTask>,
    mut working_directory: ResMut<WorkingDirectory>,
) {
    if let Some(ref mut task) = file_dialog_task.0
        && let Some(result) = block_on(future::poll_once(task))
    {
        if let Some(path) = result {
            working_directory.pending_path = path.to_string_lossy().to_string();
        }
        file_dialog_task.0 = None;
    }
}

/// `UiEvent::OpenProject`: the project's own scenario, when it has a `project.json`
/// (the start epoch and the two bodies; a restored body starts from SGP4 as a
/// catalog pick does, and the map highlight of a restored catalog object is P8's).
/// Otherwise a new scenario starts at the wall clock, rounded as the manifest writes
/// it (F-06). A manifest that cannot be used is logged and the project opens as new.
pub(crate) fn restore_project_scenario(
    dir: &Path,
    sim_config: &mut SimConfig,
    selection: &mut OrbitalSelectionState,
    log: &mut MessageWriter<LogEvent>,
) {
    match read_sim_inputs(dir) {
        Some(Ok(inputs)) => {
            sim_config.start_epoch = inputs.start_epoch;
            selection.target = Some(inputs.target);
            selection.chaser = Some(inputs.chaser);
            selection.chaser_relative = inputs.chaser_relative;
            return;
        }
        Some(Err(errors)) => {
            for message in errors {
                log.write(LogEvent {
                    level: LogLevel::Error,
                    source: "project",
                    message: format!("{MANIFEST_FILE} not used: {message}"),
                });
            }
        }
        None => {}
    }
    sim_config.start_epoch = now_utc();
}

/// `UiEvent::StartSim`: writes `project.json` from the selection, reads it back and
/// starts from what was read, so the app and the CLI take their inputs from the
/// same file. A body keeps its selection source (the map highlight is the UI's). A
/// file that cannot be written is logged and the run starts from the selection in
/// memory; a selection that is not complete or not valid is logged too.
pub(crate) fn start_from_project_file(
    dir: &Path,
    active_plan: Option<&str>,
    sim_config: &mut SimConfig,
    selection: &mut OrbitalSelectionState,
    log: &mut MessageWriter<LogEvent>,
) {
    let mut error = |message: String| {
        log.write(LogEvent {
            level: LogLevel::Error,
            source: "project",
            message,
        });
    };
    // The file is rewritten from the selection; the recording settings are not part
    // of it and carry over.
    let previous = read_manifest_file(dir).and_then(|report| report.manifest);
    let recording = previous
        .as_ref()
        .and_then(|manifest| manifest.recording.clone());
    let name = previous
        .map(|manifest| manifest.name)
        .or_else(|| {
            dir.file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| "project".to_string());
    let manifest = match ProjectManifest::from_selection(
        &name,
        sim_config.start_epoch,
        active_plan,
        selection,
    ) {
        Ok(manifest) => ProjectManifest {
            recording,
            ..manifest
        },
        // Nothing to write; the sim reports the missing body as before.
        Err(_) => return,
    };
    match save_and_reload(dir, &manifest) {
        Ok((inputs, write_error)) => {
            if let Some(message) = write_error {
                error(message);
            }
            sim_config.start_epoch = inputs.start_epoch;
            selection.chaser_relative = inputs.chaser_relative;
            for (slot, read) in [
                (&mut selection.target, inputs.target),
                (&mut selection.chaser, inputs.chaser),
            ] {
                if let Some(previous) = slot.as_ref() {
                    *slot = Some(SelectedOrbitalObject {
                        source: previous.source.clone(),
                        ..read
                    });
                }
            }
        }
        Err(errors) => {
            for message in errors {
                error(format!("{MANIFEST_FILE} not used: {message}"));
            }
        }
    }
}

pub(crate) fn handle_ui_events(
    mut commands: Commands,
    mut ui_events: MessageReader<UiEvent>,
    session: UiEventSession,
    scene: UiEventScene,
    mut flow: UiFlowState,
    mut sim_commands: MessageWriter<SimCommand>,
    mut log: MessageWriter<LogEvent>,
) {
    let UiEventSession {
        mut next_screen,
        mut next_sim_state,
        mut selected_project,
        mut working_directory,
        mut file_dialog_task,
        mut capture_plan_lib,
        mut orbital_selection,
        mut sim_config,
        mut form,
        mut settings,
    } = session;
    let UiEventScene {
        mut scene_camera,
        bodies,
        mut world_time,
        pause,
        mut next_pause,
        step_cap,
    } = scene;

    for event in ui_events.read() {
        match event {
            UiEvent::OpenProject(project_id) => {
                if capture_plan_lib.plans.contains_key(project_id.as_str()) {
                    selected_project.project_id = Some(project_id.clone());
                    *orbital_selection = OrbitalSelectionState::default();
                    restore_project_scenario(
                        Path::new(&working_directory.path),
                        &mut sim_config,
                        &mut orbital_selection,
                        &mut log,
                    );
                    next_sim_state.set(SimState::Setup);
                    next_screen.set(UiScreen::Sim);
                }
            }
            UiEvent::StartSim => {
                start_from_project_file(
                    Path::new(&working_directory.path),
                    selected_project.project_id.as_deref(),
                    &mut sim_config,
                    &mut orbital_selection,
                    &mut log,
                );
                next_sim_state.set(SimState::Running);
            }
            UiEvent::BackToHome => {
                flow.exit_confirm.0 = false;
                next_sim_state.set(SimState::Setup);
                next_screen.set(UiScreen::Home);
            }
            UiEvent::ShowExitConfirm => {
                flow.exit_confirm.0 = true;
            }
            UiEvent::CancelExitConfirm => {
                flow.exit_confirm.0 = false;
            }
            UiEvent::WorkingDirectorySelected(path) => {
                match select_working_directory(&mut working_directory, path) {
                    Ok(()) => {
                        flow.setup_error.0 = None;
                        next_screen.set(UiScreen::Home);
                    }
                    Err(message) => {
                        warn!("{message}");
                        log.write(LogEvent {
                            level: LogLevel::Error,
                            source: "ui",
                            message: message.clone(),
                        });
                        flow.setup_error.0 = Some(message);
                    }
                }
            }
            UiEvent::BrowseForWorkingDirectory => {
                let pool = AsyncComputeTaskPool::get();
                let task = pool.spawn(async {
                    rfd::AsyncFileDialog::new()
                        .pick_folder()
                        .await
                        .map(|handle| handle.path().to_owned())
                });
                file_dialog_task.0 = Some(task);
            }
            UiEvent::ChangeWorkingDirectory => {
                working_directory.pending_path = working_directory.path.clone();
                next_screen.set(UiScreen::WorkingDirectorySetup);
            }
            UiEvent::CaptureDebris { entity, plan_id } => {
                sim_commands.write(SimCommand::StartCapture {
                    target: *entity,
                    plan_id: plan_id.clone(),
                });
            }
            UiEvent::ToggleMapView => {
                if let Ok((render_layers, atmosphere, atmosphere_settings)) =
                    scene_camera.single_mut()
                    && let Some(message) =
                        set_map_view(render_layers, atmosphere, atmosphere_settings)
                {
                    log.write(LogEvent {
                        level: LogLevel::Debug,
                        source: "ui",
                        message: message.to_string(),
                    });
                }
            }
            // F-45: Select opens the catalog, which only the map view shows;
            // `sim_screen_events` shows the catalog panel.
            UiEvent::SelectFromCatalog => {
                if let Ok((render_layers, atmosphere, atmosphere_settings)) =
                    scene_camera.single_mut()
                    && !render_layers.intersects(&RenderLayers::layer(MAP_LAYER))
                    && let Some(message) =
                        set_map_view(render_layers, atmosphere, atmosphere_settings)
                {
                    log.write(LogEvent {
                        level: LogLevel::Debug,
                        source: "ui",
                        message: message.to_string(),
                    });
                }
            }
            UiEvent::ChangeTimeWarp { increase } => {
                if let Some(ref mut world_time) = world_time {
                    step_time_warp(
                        *increase,
                        world_time,
                        *step_cap,
                        &pause,
                        &mut next_pause,
                        &mut log,
                    );
                }
            }
            UiEvent::CycleCameraTarget => {
                if let Some(next_target) = cycle_camera_target(&mut commands, bodies.iter()) {
                    log.write(LogEvent {
                        level: LogLevel::Info,
                        source: "ui",
                        message: format!("Camera target → entity {next_target:?}"),
                    });
                }
            }
            UiEvent::OpenNewCapturePlanForm => {
                let template = capture_plan_lib.example_plans.get(NEW_PLAN_TEMPLATE);
                new_plan_into_form(&mut form, template);
                form.open = true;
            }
            UiEvent::CloseNewCapturePlanForm => {
                form.open = false;
                form.reset();
            }
            UiEvent::AddTransition(phase) => {
                if let Some(phase) = form.phases.get_mut(*phase) {
                    phase.transitions.push(Default::default());
                }
            }
            UiEvent::RemoveTransition { phase, index } => {
                if let Some(phase) = form.phases.get_mut(*phase)
                    && *index < phase.transitions.len()
                {
                    phase.transitions.remove(*index);
                }
            }
            UiEvent::SaveCapturePlan | UiEvent::ConfirmOverwriteCapturePlan => {
                let overwrite = matches!(event, UiEvent::ConfirmOverwriteCapturePlan);
                let saved = save_plan_from_form(
                    &mut form,
                    Path::new(&working_directory.path),
                    overwrite,
                    selected_project.project_id.is_some(),
                    &mut capture_plan_lib,
                    &mut |level, message| {
                        log.write(LogEvent {
                            level,
                            source: "ui",
                            message,
                        });
                    },
                );
                if saved {
                    flow.plans_dirty.0 = true;
                }
            }
            UiEvent::CancelOverwriteCapturePlan => {
                form.overwrite_conflict_path = None;
            }
            UiEvent::EditCapturePlan(plan_id) => {
                if let Some(plan) = capture_plan_lib.plans.get(plan_id.as_str()).cloned() {
                    let is_example = !capture_plan_lib.user_plans.contains_key(plan_id.as_str());
                    load_plan_into_form(&mut form, plan_id, &plan, is_example);
                    log.write(LogEvent {
                        level: LogLevel::Info,
                        source: "ui",
                        message: format!("Opened '{}' for editing", plan_id),
                    });
                    form.open = true;
                }
            }
            UiEvent::SetUnitSystem(unit) => {
                set_unit_system(&mut settings, &mut form, *unit);
            }
            UiEvent::ToggleCaptureGizmos => {
                settings.toggle_capture_gizmos();
            }
            UiEvent::PlaceChaserRelative { preset, offset_m } => {
                if let Err(message) = place_chaser_relative(
                    &mut orbital_selection,
                    sim_config.start_epoch,
                    *preset,
                    *offset_m,
                ) {
                    log.write(LogEvent {
                        level: LogLevel::Error,
                        source: "project",
                        message,
                    });
                }
            }
            // `sim_screen_events` owns these.
            UiEvent::RestartSim
            | UiEvent::ApplyPlanLive
            | UiEvent::DismissRestartPrompt
            | UiEvent::ToggleSatelliteIndicator
            | UiEvent::UseCatalogSelection(_)
            | UiEvent::ClearOrbitalSelection(_)
            | UiEvent::CreateCustomOrbit(_)
            | UiEvent::SetOrbitalElement { .. } => {}
        }
    }
}

/// Creates `path` and makes it the working directory (path and config). The
/// folder is created first, so on a failure the working directory and the
/// config are unchanged and the error is returned for the setup screen (F-33).
pub(crate) fn select_working_directory(
    working_directory: &mut WorkingDirectory,
    path: &str,
) -> Result<(), String> {
    std::fs::create_dir_all(path)
        .and_then(|()| set_working_directory(working_directory, path))
        .map_err(|e| format!("Failed to create working directory: {e}"))
}

pub(crate) fn poll_home_plan_refresh(
    mut user_plans_dirty: ResMut<UserPlansDirty>,
    mut capture_plan_load_errors: ResMut<CapturePlanLoadErrors>,
    screen: Res<State<UiScreen>>,
    sync_state: Res<SimPlanSyncState>,
    working_directory: Res<WorkingDirectory>,
) {
    if !user_plans_dirty.0 || *screen.get() != UiScreen::Home || sync_state.restart_requested {
        return;
    }
    // Re-scan the user plans directory for any newly broken or fixed files.
    let (_, user_load_errors) =
        load_plans_from_dir_with_errors(std::path::Path::new(&working_directory.path));
    capture_plan_load_errors.errors = user_load_errors;
    user_plans_dirty.0 = false;
}
