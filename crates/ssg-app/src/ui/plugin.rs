use std::path::{Path, PathBuf};

use avian3d::prelude::RigidBody;
use bevy::camera::CameraOutputMode;
use bevy::camera::visibility::RenderLayers;
use bevy::ecs::system::SystemParam;
use bevy::pbr::{Atmosphere, AtmosphereSettings};
use bevy::prelude::*;
use bevy::render::render_resource::BlendState;
use bevy::tasks::{AsyncComputeTaskPool, Task, block_on, futures_lite::future};
use bevy::transform::TransformSystems;
use bevy_egui::{EguiPreUpdateSet, EguiPrimaryContextPass};

use crate::render::camera::{CameraTarget, cycle_camera_target, set_map_view, setup_camera};
use crate::render::layers::UI_LAYER;
use crate::ui::catalog_state::{FilteredSpaceCatalogResults, SpaceCatalogUiState};
use crate::ui::egui::{CsvExportState, begin_plot_session, egui_plots};
use crate::ui::egui_terminal::egui_terminal_panel;
use crate::ui::egui_theme::apply_egui_theme;
use crate::ui::events::UiEvent;
use crate::ui::form::{NewCapturePlanForm, SimPlanSyncState, TransitionForm, UnitSystem};
use crate::ui::hud::{
    HudReadouts, init_hud_readouts, update_capture_guidance, update_capture_telemetry,
    update_time_warp_readout,
};
use crate::ui::input::{
    change_time_warp, egui_wants_keyboard, keep_tab_from_egui, step_time_warp,
    toggle_capture_gizmos, toggle_map_view, toggle_origin,
};
use crate::ui::log_ui::{CaptureLogUiState, CapturePlanLoadErrors};
use crate::ui::overlays::{
    map_orbitals, update_satellite_indicator_overlay, update_selected_catalog_overlay,
};
use crate::ui::recording::{
    RecordingPanel, adapt_step_cap, collect_recorder_events, install_recording_sink,
    recording_window, reset_recording_panel, reset_step_cap,
};
use crate::ui::replay::{
    RecordingsList, ReturnToSim, advance_replay, poll_open_recording, recordings_window,
    remove_replay_session, reset_recordings_list, return_to_sim, scan_recordings_system,
    show_replay_frame, show_replay_in_scene_view, spawn_replay_bodies, transport_window,
};
use crate::ui::screen::{ScreenPlugin, UiScreen};
use crate::ui::screens::capture_plan::{
    build_capture_plan_json, capture_plan_form_window, generate_filename, validate_form,
};
use crate::ui::screens::home::{home_screen, load_home_plans};
use crate::ui::screens::project_detail::{
    ExitConfirmOpen, cleanup_project_detail_screen, refresh_space_catalog_results,
    reset_space_catalog_ui_state, restore_catalog_sources, sim_screen_events, sim_screen_modals,
    sim_screen_panels, space_catalog_panel, spawn_project_detail_screen,
};
use crate::ui::screens::working_directory_setup::{
    WorkingDirectorySetupError, clear_working_directory_setup_error, working_directory_setup_screen,
};
use crate::ui::settings::{Settings, set_unit_system};
use crate::ui::theme::UiTheme;
use ssg_sim::SimSet;
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
struct FileDialogTask(Option<Task<Option<PathBuf>>>);

#[derive(Resource, Default)]
struct UserPlansDirty(bool);

#[derive(SystemParam)]
struct UiFlowState<'w> {
    plans_dirty: ResMut<'w, UserPlansDirty>,
    exit_confirm: ResMut<'w, ExitConfirmOpen>,
    setup_error: ResMut<'w, WorkingDirectorySetupError>,
}

pub struct UiPlugin;

#[derive(Component)]
struct UiCamera;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ScreenPlugin)
            .init_resource::<UiTheme>()
            .init_resource::<WorkingDirectory>()
            .init_resource::<NewCapturePlanForm>()
            .init_resource::<FileDialogTask>()
            .init_resource::<UserPlansDirty>()
            .init_resource::<ExitConfirmOpen>()
            .init_resource::<HudReadouts>()
            .init_resource::<WorkingDirectorySetupError>()
            .init_resource::<SimPlanSyncState>()
            .init_resource::<CapturePlanLoadErrors>()
            .init_resource::<CaptureLogUiState>()
            .init_resource::<CsvExportState>()
            .init_resource::<RecordingPanel>()
            .init_resource::<RecordingsList>()
            .init_resource::<ReturnToSim>()
            .init_resource::<SpaceCatalogUiState>()
            .init_resource::<FilteredSpaceCatalogResults>()
            .init_resource::<Settings>()
            .add_message::<UiEvent>()
            .add_systems(Startup, setup_ui_camera)
            .add_systems(
                Update,
                (
                    (toggle_map_view, toggle_origin, toggle_capture_gizmos)
                        .run_if(not(egui_wants_keyboard)),
                    // A replay shows no sidebar: the readouts rest (rc.3).
                    (
                        update_time_warp_readout,
                        update_capture_telemetry,
                        update_capture_guidance,
                    )
                        .run_if(not(in_state(SimState::Replay))),
                )
                    .run_if(in_state(UiScreen::Sim)),
            )
            .add_systems(
                PreUpdate,
                keep_tab_from_egui
                    .after(EguiPreUpdateSet::ProcessInput)
                    .before(EguiPreUpdateSet::BeginPass)
                    .run_if(in_state(UiScreen::Sim)),
            )
            // The warp keys do nothing in a replay (the player has its own speeds).
            .add_systems(
                Update,
                change_time_warp
                    .run_if(not(egui_wants_keyboard))
                    .run_if(in_state(UiScreen::Sim))
                    .run_if(not(in_state(SimState::Replay))),
            )
            .add_systems(
                OnEnter(UiScreen::Sim),
                (
                    reset_recording_panel,
                    reset_recordings_list,
                    reset_step_cap,
                    begin_plot_session,
                ),
            )
            .add_systems(OnEnter(SimState::Running), install_recording_sink)
            .add_systems(
                Update,
                (
                    adapt_step_cap.run_if(in_state(SimState::Running)),
                    collect_recorder_events,
                    (scan_recordings_system, poll_open_recording).run_if(in_state(SimState::Setup)),
                    (advance_replay, show_replay_frame)
                        .chain()
                        .run_if(in_state(SimState::Replay)),
                )
                    .run_if(in_state(UiScreen::Sim)),
            )
            .add_systems(
                OnEnter(SimState::Replay),
                (
                    cleanup_project_detail_screen,
                    spawn_replay_bodies,
                    show_replay_in_scene_view,
                )
                    .chain(),
            )
            .add_systems(OnExit(SimState::Replay), remove_replay_session)
            .add_systems(Update, return_to_sim.run_if(in_state(UiScreen::Home)))
            .add_systems(
                PostUpdate,
                map_orbitals
                    .after(TransformSystems::Propagate)
                    .run_if(in_state(UiScreen::Sim)),
            )
            .add_systems(
                OnEnter(UiScreen::WorkingDirectorySetup),
                clear_working_directory_setup_error,
            )
            .add_systems(OnEnter(UiScreen::Home), load_home_plans)
            .add_systems(
                OnEnter(UiScreen::Sim),
                (
                    (init_hud_readouts, spawn_project_detail_screen)
                        .chain()
                        .after(setup_camera),
                    restore_catalog_sources,
                    reset_sync_state,
                    reset_terminal_log_ui_state,
                ),
            )
            .add_systems(
                OnEnter(SimState::Running),
                (
                    cleanup_project_detail_screen,
                    init_hud_readouts,
                    spawn_project_detail_screen,
                    reset_sync_state,
                )
                    .chain()
                    .after(SimSet::SpawnBodies),
            )
            .add_systems(
                OnExit(UiScreen::Sim),
                (cleanup_project_detail_screen, reset_space_catalog_ui_state),
            )
            .add_systems(
                Last,
                (
                    poll_file_dialog_task,
                    sim_screen_events,
                    poll_home_plan_refresh,
                    poll_sim_restart,
                    (
                        refresh_space_catalog_results,
                        update_selected_catalog_overlay,
                        update_satellite_indicator_overlay,
                    )
                        .chain()
                        .run_if(in_state(UiScreen::Sim)),
                    handle_ui_events.before(SimSet::ApplyCommands),
                ),
            )
            .add_systems(EguiPrimaryContextPass, apply_egui_theme)
            .add_systems(
                EguiPrimaryContextPass,
                (
                    working_directory_setup_screen
                        .run_if(in_state(UiScreen::WorkingDirectorySetup)),
                    home_screen.run_if(in_state(UiScreen::Home)),
                    capture_plan_form_window,
                    sim_screen_modals,
                )
                    .chain()
                    .after(apply_egui_theme),
            )
            .add_systems(
                EguiPrimaryContextPass,
                (
                    egui_terminal_panel,
                    (sim_screen_panels, space_catalog_panel)
                        .chain()
                        .run_if(not(in_state(SimState::Replay))),
                    egui_plots,
                )
                    .chain()
                    .run_if(in_state(UiScreen::Sim)),
            )
            .add_systems(
                EguiPrimaryContextPass,
                (
                    recording_window.run_if(in_state(SimState::Running)),
                    recordings_window.run_if(in_state(SimState::Setup)),
                    transport_window.run_if(in_state(SimState::Replay)),
                )
                    .after(space_catalog_panel)
                    .run_if(in_state(UiScreen::Sim)),
            );
    }
}

fn setup_ui_camera(mut commands: Commands, camera_query: Query<Entity, With<UiCamera>>) {
    if !camera_query.is_empty() {
        return;
    }

    commands.spawn((
        UiCamera,
        Camera2d,
        RenderLayers::layer(UI_LAYER),
        Camera {
            order: 1,
            clear_color: ClearColorConfig::None,
            output_mode: CameraOutputMode::Write {
                blend_state: Some(BlendState::ALPHA_BLENDING),
                clear_color: ClearColorConfig::None,
            },
            ..default()
        },
    ));
}

fn poll_file_dialog_task(
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

fn poll_sim_restart(
    mut sync_state: ResMut<SimPlanSyncState>,
    screen: Res<State<UiScreen>>,
    sim_state: Res<State<SimState>>,
    mut next_screen: ResMut<NextState<UiScreen>>,
    mut next_sim_state: ResMut<NextState<SimState>>,
) {
    if !sync_state.restart_requested {
        return;
    }

    if *screen.get() == UiScreen::Home {
        next_screen.set(UiScreen::Sim);
    } else if *screen.get() == UiScreen::Sim && *sim_state.get() == SimState::Setup {
        sync_state.restart_requested = false;
        next_sim_state.set(SimState::Running);
    }
}

fn reset_sync_state(mut sync_state: ResMut<SimPlanSyncState>) {
    let restart_requested = sync_state.restart_requested;
    *sync_state = SimPlanSyncState {
        restart_requested,
        ..Default::default()
    };
}

fn reset_terminal_log_ui_state(mut log_ui: ResMut<CaptureLogUiState>) {
    // Preserve active_filters and existing log entries across sim screen re-entries,
    // but reset per-frame UI state (scroll position, selection).
    log_ui.is_user_scrolled = false;
    log_ui.selected_rows = None;
    log_ui.selection_anchor = None;
    // Force a display rebuild so newly-spawned rows reflect the current log.
    log_ui.last_rendered_count = usize::MAX;
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
fn restore_project_scenario(
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
fn start_from_project_file(
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

fn handle_ui_events(
    mut commands: Commands,
    mut ui_events: MessageReader<UiEvent>,
    mut next_screen: ResMut<NextState<UiScreen>>,
    mut selected_project: ResMut<SelectedProject>,
    mut working_directory: ResMut<WorkingDirectory>,
    mut file_dialog_task: ResMut<FileDialogTask>,

    mut capture_plan_lib: ResMut<CapturePlanLibrary>,
    mut orbital_selection: ResMut<OrbitalSelectionState>,
    mut world_time: Option<ResMut<WorldTime>>,
    mut scene_camera: Query<
        (&mut RenderLayers, &mut Atmosphere, &mut AtmosphereSettings),
        Without<UiCamera>,
    >,
    bodies: Query<(Entity, Has<CameraTarget>), (With<RigidBody>, With<Orbital>)>,
    ui_runtime: (
        ResMut<Settings>,
        ResMut<NextState<SimState>>,
        MessageWriter<SimCommand>,
        ResMut<NewCapturePlanForm>,
        ResMut<SimConfig>,
    ),
    mut flow: UiFlowState,
    mut log: MessageWriter<LogEvent>,
    (pause, mut next_pause, step_cap): (
        Res<State<PauseState>>,
        ResMut<NextState<PauseState>>,
        Res<StepCap>,
    ),
) {
    let (mut settings, mut next_sim_state, mut sim_commands, mut form, mut sim_config) = ui_runtime;

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

fn poll_home_plan_refresh(
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

#[cfg(test)]
mod start_from_project_file_tests {
    use bevy::ecs::message::Messages;
    use bevy::ecs::system::RunSystemOnce;
    use ssg_sim::orbit::catalog::gp_record_to_elements;
    use ssg_sim::orbit::selection::EditableOrbitalElements;
    use ssg_sim::test_support::{TempDir, fixed_epoch, q13};

    use super::*;

    fn catalog_pick(norad_id: u64, label: &str) -> SelectedOrbitalObject {
        let (_, gp, _) =
            gp_record_to_elements(&q13::fixture_record(norad_id), fixed_epoch()).unwrap();
        SelectedOrbitalObject {
            source: ssg_sim::orbit::selection::OrbitalSelectionSource::Catalog {
                catalog_index: 7,
                gpu_index: 7,
                label: label.to_string(),
            },
            elements: EditableOrbitalElements::from_gp(&gp),
            gp: Some(gp),
            mass_kg: None,
            inertia_kg_m2: None,
        }
    }

    /// Runs [`start_from_project_file`] on `dir`; returns what it left and the
    /// messages it logged.
    fn start(
        dir: &Path,
        mut config: SimConfig,
        mut selection: OrbitalSelectionState,
    ) -> (SimConfig, OrbitalSelectionState, Vec<String>) {
        let mut app = App::new();
        app.add_message::<LogEvent>();
        let dir = dir.to_path_buf();
        let (config, selection) = app
            .world_mut()
            .run_system_once(move |mut log: MessageWriter<LogEvent>| {
                start_from_project_file(
                    &dir,
                    Some("example_plan"),
                    &mut config,
                    &mut selection,
                    &mut log,
                );
                (config.clone(), selection.clone())
            })
            .unwrap();
        let logged = app
            .world()
            .resource::<Messages<LogEvent>>()
            .iter_current_update_messages()
            .map(|event| event.message.clone())
            .collect();
        (config, selection, logged)
    }

    /// Runs [`restore_project_scenario`] on `dir`.
    fn open(dir: &Path) -> (SimConfig, OrbitalSelectionState, Vec<String>) {
        let mut app = App::new();
        app.add_message::<LogEvent>();
        let dir = dir.to_path_buf();
        let (config, selection) = app
            .world_mut()
            .run_system_once(move |mut log: MessageWriter<LogEvent>| {
                let mut config = SimConfig {
                    start_epoch: fixed_epoch(),
                };
                let mut selection = OrbitalSelectionState::default();
                restore_project_scenario(&dir, &mut config, &mut selection, &mut log);
                (config, selection)
            })
            .unwrap();
        let logged = app
            .world()
            .resource::<Messages<LogEvent>>()
            .iter_current_update_messages()
            .map(|event| event.message.clone())
            .collect();
        (config, selection, logged)
    }

    #[test]
    fn open_restores_the_epoch_and_the_bodies_from_project_json() {
        let dir = TempDir::new("app-open");
        let selection = both_bodies();
        let epoch = ssg_sim::project::parse_utc("2026-04-03T16:03:47.002464Z").unwrap();
        start(
            dir.path(),
            SimConfig { start_epoch: epoch },
            selection.clone(),
        );

        let (config, restored, logged) = open(dir.path());

        assert!(logged.is_empty(), "{logged:?}");
        assert_eq!(config.start_epoch, epoch);
        assert_eq!(restored.target.unwrap().gp, selection.target.unwrap().gp);
        assert_eq!(restored.chaser.unwrap().gp, selection.chaser.unwrap().gp);
    }

    #[test]
    fn open_without_project_json_starts_a_new_scenario_at_a_rounded_now() {
        let dir = TempDir::new("app-open-new");

        let (config, restored, logged) = open(dir.path());

        assert!(logged.is_empty());
        assert!(restored.target.is_none() && restored.chaser.is_none());
        assert_ne!(
            config.start_epoch,
            fixed_epoch(),
            "the wall clock, not the old epoch"
        );
        assert_eq!(
            ssg_sim::project::parse_utc(&ssg_sim::project::format_utc(config.start_epoch)),
            Some(config.start_epoch),
            "a whole number of microseconds"
        );
    }

    #[test]
    fn open_with_an_unusable_project_json_logs_and_opens_as_new() {
        let dir = TempDir::new("app-open-bad");
        dir.write("project.json", "{ not json");

        let (config, restored, logged) = open(dir.path());

        assert_eq!(logged.len(), 1, "{logged:?}");
        assert!(
            logged[0].starts_with("project.json not used: [project.json] Invalid JSON: "),
            "{logged:?}"
        );
        assert!(restored.target.is_none());
        assert_ne!(config.start_epoch, fixed_epoch());
    }

    fn both_bodies() -> OrbitalSelectionState {
        OrbitalSelectionState {
            target: Some(catalog_pick(q13::HST_NORAD_ID, "HST")),
            chaser: Some(catalog_pick(q13::CHASER_NORAD_ID, "Chaser")),
        }
    }

    #[test]
    fn start_keeps_the_recording_settings_of_project_json() {
        let dir = TempDir::new("app-start-recording");
        let selection = both_bodies();
        start(
            dir.path(),
            SimConfig {
                start_epoch: fixed_epoch(),
            },
            selection.clone(),
        );
        // The Recording window's Save put a block into the file.
        let mut manifest = read_manifest_file(dir.path()).unwrap().manifest.unwrap();
        let block = ssg_sim::project::manifest::RecordingBlock {
            auto_start_phase: Some("capture".to_string()),
            keyframe_interval_steps: Some(16),
            ..Default::default()
        };
        manifest.recording = Some(block.clone());
        ssg_sim::project::manifest::save_manifest(dir.path(), &manifest).unwrap();

        let (_, _, logged) = start(
            dir.path(),
            SimConfig {
                start_epoch: fixed_epoch(),
            },
            selection,
        );

        assert!(logged.is_empty(), "{logged:?}");
        let kept = read_manifest_file(dir.path()).unwrap().manifest.unwrap();
        assert_eq!(kept.recording, Some(block));
    }

    #[test]
    fn start_writes_project_json_and_starts_from_what_it_read() {
        let dir = TempDir::new("app-start");
        let selection = both_bodies();

        let (config, after, logged) = start(
            dir.path(),
            SimConfig {
                start_epoch: fixed_epoch(),
            },
            selection.clone(),
        );

        assert!(logged.is_empty(), "{logged:?}");
        let written = std::fs::read_to_string(dir.path().join("project.json")).unwrap();
        assert!(
            written.contains("\"active_plan\": \"example_plan\""),
            "{written}"
        );
        assert_eq!(config.start_epoch, fixed_epoch());
        let target = after.target.unwrap();
        assert_eq!(target.gp, selection.target.as_ref().unwrap().gp);
        assert!(
            matches!(
                target.source,
                ssg_sim::orbit::selection::OrbitalSelectionSource::Catalog { gpu_index: 7, .. }
            ),
            "the selection keeps its source for the map highlight"
        );
    }

    #[test]
    fn start_keeps_the_name_of_an_existing_project_json() {
        let dir = TempDir::new("app-start-name");
        let first = start(
            dir.path(),
            SimConfig {
                start_epoch: fixed_epoch(),
            },
            both_bodies(),
        );
        let text = std::fs::read_to_string(dir.path().join("project.json")).unwrap();
        let renamed = text.replacen(
            &format!(
                "\"name\": \"{}\"",
                dir.path().file_name().unwrap().to_string_lossy()
            ),
            "\"name\": \"Hubble study\"",
            1,
        );
        assert_ne!(
            text, renamed,
            "the folder name was the default project name"
        );
        std::fs::write(dir.path().join("project.json"), renamed).unwrap();
        drop(first);

        start(
            dir.path(),
            SimConfig {
                start_epoch: fixed_epoch(),
            },
            both_bodies(),
        );

        let text = std::fs::read_to_string(dir.path().join("project.json")).unwrap();
        assert!(text.contains("\"name\": \"Hubble study\""), "{text}");
    }

    #[test]
    fn start_without_both_bodies_writes_nothing() {
        let dir = TempDir::new("app-start-incomplete");
        let selection = OrbitalSelectionState {
            target: Some(catalog_pick(q13::HST_NORAD_ID, "HST")),
            chaser: None,
        };

        let (_, after, logged) = start(
            dir.path(),
            SimConfig {
                start_epoch: fixed_epoch(),
            },
            selection,
        );

        assert!(logged.is_empty());
        assert!(!dir.path().join("project.json").exists());
        assert!(after.target.is_some() && after.chaser.is_none());
    }

    #[test]
    fn start_in_a_folder_that_cannot_be_written_logs_and_keeps_the_selection() {
        let dir = TempDir::new("app-start-unwritable");
        let missing = dir.path().join("not").join("there");
        let selection = both_bodies();

        let (_, after, logged) = start(
            &missing,
            SimConfig {
                start_epoch: fixed_epoch(),
            },
            selection.clone(),
        );

        assert_eq!(logged.len(), 1, "{logged:?}");
        assert!(
            logged[0].starts_with("could not write project.json: "),
            "{logged:?}"
        );
        assert_eq!(after.target.unwrap().gp, selection.target.unwrap().gp);
    }
}
