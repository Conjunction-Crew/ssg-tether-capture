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
use crate::ui::events::UiEvent;
use crate::ui::form::{NewCapturePlanForm, SimPlanSyncState, TransitionForm, UnitSystem};
use crate::ui::input::step_time_warp;
use crate::ui::log_ui::CapturePlanLoadErrors;
use crate::ui::plugin::UiCamera;
use crate::ui::screen::UiScreen;
use crate::ui::screens::capture_plan::{build_capture_plan_json, generate_filename, validate_form};
use crate::ui::screens::project_detail::ExitConfirmOpen;
use crate::ui::screens::working_directory_setup::WorkingDirectorySetupError;
use crate::ui::settings::{Settings, set_unit_system};
use ssg_sim::capture::SimCommand;
use ssg_sim::capture::plan::{CapturePlan, CapturePlanLibrary};
use ssg_sim::orbit::Orbital;
use ssg_sim::orbit::selection::{OrbitalSelectionState, SelectedOrbitalObject};
use ssg_sim::project::manifest::{
    MANIFEST_FILE, ProjectManifest, read_manifest_file, read_sim_inputs, save_and_reload,
};
use ssg_sim::project::plan_files::{read_plan_value, save_plan};
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

/// Saves the plan form's JSON (v0-shaped) as a v1 plan file: migrated in memory,
/// validated, then written by the v1 writer. The error lists why it was not saved.
pub(crate) fn save_form_plan(
    dir: &std::path::Path,
    filename: &str,
    json: &serde_json::Value,
) -> Result<(), String> {
    let plan_id = filename.strip_suffix(".json").unwrap_or(filename);
    let report = read_plan_value(plan_id, json);
    let plan = report.plan.ok_or_else(|| report.errors.join("; "))?;
    save_plan(dir, filename, &plan).map_err(|e| e.to_string())
}

/// `UiEvent::EditCapturePlan`: fills the form from `plan` (read-only for an
/// example plan), shown in the toggle's units, and snapshots it, so an unchanged
/// Save writes nothing.
pub(crate) fn load_plan_into_form(
    form: &mut NewCapturePlanForm,
    plan_id: &str,
    plan: &CapturePlan,
    is_example: bool,
) {
    // The plan's numbers are metric; they are converted once they are all in.
    let unit = form.unit_system;
    form.unit_system = UnitSystem::Metric;
    form.reset();
    form.read_only = is_example;
    form.plan_name = plan.name.clone();
    form.tether_name = plan.tether.clone();
    if let Some(device) = &plan.device {
        form.tether_type = device.device_type.clone().unwrap_or_default();
        if let Some(length) = device.tether_length_m.filter(|l| *l > 0.0) {
            form.tether_length = number_text(length);
        }
    }
    for phase in &plan.phases {
        let transitions: Vec<TransitionForm> = phase
            .transitions
            .iter()
            .flatten()
            .filter_map(|t| {
                let dist = t.all_of.distance_m?;
                let (kind, val) = if let Some(v) = dist.less_than {
                    ("less_than".to_string(), number_text(v))
                } else {
                    ("greater_than".to_string(), number_text(dist.greater_than?))
                };
                Some(TransitionForm {
                    to: t.to.clone(),
                    distance_kind: kind,
                    distance_value: val,
                })
            })
            .collect();
        let max_velocity = number_text(phase.max_velocity_mps);
        let max_force = number_text(phase.max_force_n);
        let shrink_rate = phase.shrink_rate_mps.map(number_text).unwrap_or_default();

        match phase.id.as_str() {
            "approach" => {
                form.approach_max_velocity = max_velocity;
                form.approach_max_force = max_force;
                form.approach_transitions = transitions;
            }
            "terminal" => {
                form.terminal_max_velocity = max_velocity;
                form.terminal_max_force = max_force;
                form.terminal_shrink_rate = shrink_rate;
                form.terminal_transitions = transitions;
            }
            "capture" => {
                form.capture_max_velocity = max_velocity;
                form.capture_max_force = max_force;
                form.capture_shrink_rate = shrink_rate;
            }
            _ => {}
        }
    }
    form.set_unit_system(unit);
    form.editing_plan_id = Some(plan_id.to_string());
    // Snapshot the form state so we can detect unchanged edits on Save.
    form.original_json = Some(build_capture_plan_json(form));
}

/// A plan value as the form shows it (as JSON writes it, e.g. `2.0`).
fn number_text(value: f64) -> String {
    serde_json::Value::from(value).to_string()
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
                form.reset();
                form.open = true;
            }
            UiEvent::CloseNewCapturePlanForm => {
                form.open = false;
                form.reset();
            }
            UiEvent::AddApproachTransition => {
                form.approach_transitions.push(Default::default());
            }
            UiEvent::RemoveApproachTransition(i) => {
                if *i < form.approach_transitions.len() {
                    form.approach_transitions.remove(*i);
                }
            }
            UiEvent::AddTerminalTransition => {
                form.terminal_transitions.push(Default::default());
            }
            UiEvent::RemoveTerminalTransition(i) => {
                if *i < form.terminal_transitions.len() {
                    form.terminal_transitions.remove(*i);
                }
            }
            UiEvent::SaveCapturePlan => {
                // If editing and nothing has changed, close the form without
                // writing to disk or prompting for a sim restart.
                if form.editing_plan_id.is_some()
                    && form.original_json.as_ref() == Some(&build_capture_plan_json(&form))
                {
                    form.open = false;
                    form.reset();
                } else {
                    let errors = validate_form(&form);
                    if !errors.is_empty() {
                        form.validation_errors = errors;
                    } else {
                        form.validation_errors.clear();
                        // When editing, always write to the original file (keyed by
                        // editing_plan_id) so renaming a plan only updates the `name`
                        // field inside the JSON without creating a new file on disk.
                        let filename = if let Some(ref edit_id) = form.editing_plan_id {
                            generate_filename(edit_id)
                        } else {
                            generate_filename(&form.plan_name)
                        };
                        let dest = std::path::Path::new(&working_directory.path).join(&filename);
                        // In edit mode, skip overwrite dialog and always overwrite
                        let skip_overwrite_check = form.editing_plan_id.is_some();
                        let was_editing = form.editing_plan_id.is_some();
                        if dest.exists()
                            && form.overwrite_conflict_path.is_none()
                            && !skip_overwrite_check
                        {
                            form.overwrite_conflict_path = Some(dest.to_string_lossy().to_string());
                        } else {
                            let json = build_capture_plan_json(&form);
                            if let Err(e) = save_form_plan(
                                std::path::Path::new(&working_directory.path),
                                &filename,
                                &json,
                            ) {
                                log.write(LogEvent {
                                    level: LogLevel::Error,
                                    source: "ui",
                                    message: format!(
                                        "Failed to write capture plan '{}': {e}",
                                        form.plan_name
                                    ),
                                });
                            } else {
                                // Reload user plans and recompile every plan so
                                // compiled_plans stays in sync. When editing from the sim
                                // with sim-affecting changes, we still prompt the user to
                                // restart, but capture will not fail with stale data.
                                capture_plan_lib.reload_user_plans(std::path::Path::new(
                                    &working_directory.path,
                                ));
                                let editing_from_sim =
                                    was_editing && selected_project.project_id.is_some();
                                // `name` is not a sim parameter. If only the name
                                // changed, apply immediately without a restart prompt.
                                let only_name_changed = editing_from_sim
                                    && form.original_json.as_ref().is_some_and(|orig| {
                                        let mut a = orig.clone();
                                        let mut b = json.clone();
                                        if let (Some(ao), Some(bo)) =
                                            (a.as_object_mut(), b.as_object_mut())
                                        {
                                            ao.remove("name");
                                            bo.remove("name");
                                        }
                                        a == b
                                    });
                                flow.plans_dirty.0 = true;
                                log.write(LogEvent {
                                    level: LogLevel::Info,
                                    source: "ui",
                                    message: format!("Capture plan '{}' saved", form.plan_name),
                                });
                                // Prompt for restart only when sim-affecting params changed
                                let should_prompt = editing_from_sim && !only_name_changed;
                                form.open = false;
                                form.reset();
                                if should_prompt {
                                    form.show_restart_prompt = true;
                                }
                            }
                        }
                    }
                } // end else (form was changed)
            }
            UiEvent::ConfirmOverwriteCapturePlan => {
                let filename = generate_filename(&form.plan_name);
                let json = build_capture_plan_json(&form);
                if let Err(e) = save_form_plan(
                    std::path::Path::new(&working_directory.path),
                    &filename,
                    &json,
                ) {
                    log.write(LogEvent {
                        level: LogLevel::Error,
                        source: "ui",
                        message: format!("Failed to write capture plan '{}': {e}", form.plan_name),
                    });
                } else {
                    capture_plan_lib
                        .reload_user_plans(std::path::Path::new(&working_directory.path));
                    flow.plans_dirty.0 = true;
                    log.write(LogEvent {
                        level: LogLevel::Info,
                        source: "ui",
                        message: format!("Capture plan '{}' saved", form.plan_name),
                    });
                    form.open = false;
                    form.reset();
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
            // `sim_screen_events` owns these.
            UiEvent::RestartSim
            | UiEvent::ApplyPlanLive
            | UiEvent::DismissRestartPrompt
            | UiEvent::ToggleSatelliteIndicator
            | UiEvent::UseCatalogSelection(_)
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
