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
use bevy_egui::EguiPrimaryContextPass;

use crate::render::camera::{CameraTarget, cycle_camera_target, set_map_view, setup_camera};
use crate::render::layers::UI_LAYER;
use crate::ui::catalog_state::{FilteredSpaceCatalogResults, SpaceCatalogUiState};
use crate::ui::egui::{CsvExportState, egui_plots};
use crate::ui::egui_terminal::egui_terminal_panel;
use crate::ui::events::UiEvent;
use crate::ui::form::{NewCapturePlanForm, SimPlanSyncState, TransitionForm, UnitSystem};
use crate::ui::hud::{
    map_orbitals, update_capture_guidance, update_capture_telemetry, update_time_warp_readout,
};
use crate::ui::input::{
    change_time_warp, step_time_warp, toggle_capture_gizmos, toggle_map_view, toggle_origin,
};
use crate::ui::log_ui::{CaptureLogUiState, CapturePlanLoadErrors};
use crate::ui::screens::capture_plan::{
    CapturePlanModal, CapturePlanScrollBody, build_capture_plan_json, capture_plan_interactions,
    dropdown_interactions, generate_filename, spawn_capture_plan_modal, sync_dropdown_labels,
    sync_form_fields, tether_type_radio_interactions, validate_form,
};
use crate::ui::screens::home::{
    HomeScreen, cleanup_home_screen, home_interactions, spawn_home_screen, spawn_home_screen_inner,
    update_home_working_directory_label,
};
use crate::ui::screens::project_detail::{
    RestartPromptModal, SimScreen, catalog_interactions, catalog_keyboard_input,
    cleanup_project_detail_screen, collapsible_toggle_interaction, orbital_selection_interactions,
    project_detail_interactions, refresh_space_catalog_results, reset_space_catalog_ui_state,
    restart_prompt_interactions, spawn_exit_confirm_modal, spawn_project_detail_screen,
    spawn_restart_prompt_modal, sync_orbital_selection_ui, sync_space_catalog_ui,
    update_satellite_indicator_overlay, update_selected_catalog_overlay, update_sync_indicator,
    view_edit_plan_interactions,
};
use crate::ui::screens::working_directory_setup::{
    DirectoryPathText, cleanup_working_directory_setup_screen,
    spawn_working_directory_setup_screen, working_directory_setup_interactions,
};
use crate::ui::settings::Settings;
use crate::ui::theme::UiTheme;
use crate::ui::widgets::{
    ClipboardRes, input_field_display, input_field_interaction, input_field_keyboard,
};
use ssg_sim::SimSet;
use ssg_sim::capture::SimCommand;
use ssg_sim::capture::plan::CapturePlanLibrary;
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
use ssg_sim::session::{PauseState, SimState, UiScreen};
use ssg_sim::telemetry::{LogEvent, LogLevel};
use ssg_sim::time::{SimConfig, WorldTime};

#[derive(Resource, Default)]
struct FileDialogTask(Option<Task<Option<PathBuf>>>);

#[derive(Resource, Default)]
struct UserPlansDirty(bool);

#[derive(Resource, Default)]
struct ExitConfirmPending(bool);

#[derive(SystemParam)]
struct UiFlowState<'w> {
    plans_dirty: ResMut<'w, UserPlansDirty>,
    exit_confirm: ResMut<'w, ExitConfirmPending>,
}

pub struct UiPlugin;

#[derive(Component)]
struct UiCamera;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<UiTheme>()
            .init_resource::<WorkingDirectory>()
            .init_resource::<NewCapturePlanForm>()
            .init_resource::<FileDialogTask>()
            .init_resource::<UserPlansDirty>()
            .init_resource::<ExitConfirmPending>()
            .init_resource::<SimPlanSyncState>()
            .init_resource::<CapturePlanLoadErrors>()
            .init_resource::<CaptureLogUiState>()
            .init_resource::<CsvExportState>()
            .init_resource::<SpaceCatalogUiState>()
            .init_resource::<FilteredSpaceCatalogResults>()
            .init_resource::<Settings>()
            .init_non_send_resource::<ClipboardRes>()
            .add_message::<UiEvent>()
            .add_systems(Startup, setup_ui_camera)
            .add_systems(
                Update,
                (
                    toggle_map_view,
                    toggle_origin,
                    toggle_capture_gizmos,
                    change_time_warp,
                    update_time_warp_readout,
                    update_capture_telemetry,
                    update_capture_guidance,
                )
                    .run_if(in_state(UiScreen::Sim)),
            )
            .add_systems(
                PostUpdate,
                map_orbitals
                    .after(TransformSystems::Propagate)
                    .run_if(in_state(UiScreen::Sim)),
            )
            .add_systems(
                OnEnter(UiScreen::WorkingDirectorySetup),
                spawn_working_directory_setup_screen,
            )
            .add_systems(
                OnExit(UiScreen::WorkingDirectorySetup),
                cleanup_working_directory_setup_screen,
            )
            .add_systems(OnEnter(UiScreen::Home), spawn_home_screen)
            .add_systems(OnExit(UiScreen::Home), cleanup_home_screen)
            .add_systems(
                OnEnter(UiScreen::Sim),
                (
                    spawn_project_detail_screen.after(setup_camera),
                    reset_sync_state,
                    reset_terminal_log_ui_state,
                ),
            )
            .add_systems(
                OnEnter(SimState::Running),
                (
                    cleanup_project_detail_screen,
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
                    working_directory_setup_interactions,
                    poll_file_dialog_task,
                    home_interactions,
                    update_home_working_directory_label,
                    view_edit_plan_interactions,
                    restart_prompt_interactions,
                    update_sync_indicator,
                    input_field_interaction,
                    input_field_keyboard,
                    input_field_display,
                    (
                        sync_form_fields,
                        capture_plan_interactions,
                        tether_type_radio_interactions,
                        (dropdown_interactions, sync_dropdown_labels).chain(),
                    ),
                    poll_new_plan_modal,
                    poll_home_plan_refresh,
                    poll_exit_confirm_modal,
                    poll_restart_prompt_modal,
                    poll_sim_restart,
                    (
                        update_sim_screen_bottom_padding,
                        project_detail_interactions,
                        orbital_selection_interactions,
                        sync_orbital_selection_ui,
                        collapsible_toggle_interaction,
                        catalog_interactions,
                        catalog_keyboard_input,
                        refresh_space_catalog_results,
                        sync_space_catalog_ui,
                        update_selected_catalog_overlay,
                        update_satellite_indicator_overlay,
                    )
                        .chain()
                        .run_if(in_state(UiScreen::Sim)),
                    handle_ui_events.before(SimSet::ApplyCommands),
                ),
            )
            .add_systems(
                EguiPrimaryContextPass,
                (egui_terminal_panel, egui_plots)
                    .chain()
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
    mut path_text: Query<&mut Text, With<DirectoryPathText>>,
) {
    if let Some(ref mut task) = file_dialog_task.0
        && let Some(result) = block_on(future::poll_once(task))
    {
        if let Some(path) = result {
            let path_str = path.to_string_lossy().to_string();
            working_directory.pending_path = path_str.clone();
            for mut text in &mut path_text {
                text.0 = path_str.clone();
            }
        }
        file_dialog_task.0 = None;
    }
}

fn poll_new_plan_modal(
    mut commands: Commands,
    form: Res<NewCapturePlanForm>,
    asset_server: Res<AssetServer>,
    theme: Res<UiTheme>,
    modals: Query<Entity, With<CapturePlanModal>>,
    scroll_body: Query<&ScrollPosition, With<CapturePlanScrollBody>>,
    // (approach_count, terminal_count, has_overwrite, error_count, scroll_y, unit_system)
    mut last: Local<(usize, usize, bool, usize, f32, UnitSystem)>,
) {
    if !form.is_changed() {
        return;
    }
    let modal_exists = !modals.is_empty();
    if form.open && !modal_exists {
        spawn_capture_plan_modal(&mut commands, &asset_server, &theme, &form, UI_LAYER, 0.0);
        *last = (
            form.approach_transitions.len(),
            form.terminal_transitions.len(),
            form.overwrite_conflict_path.is_some(),
            form.validation_errors.len(),
            0.0,
            form.unit_system,
        );
    } else if !form.open && modal_exists {
        for e in &modals {
            commands.entity(e).despawn();
        }
    } else if form.open && modal_exists {
        let needs_rerender = form.approach_transitions.len() != last.0
            || form.terminal_transitions.len() != last.1
            || form.overwrite_conflict_path.is_some() != last.2
            || form.validation_errors.len() != last.3
            || form.unit_system != last.5;
        if needs_rerender {
            let scroll_y = scroll_body.single().map(|sp| sp.0.y).unwrap_or(last.4);
            for e in &modals {
                commands.entity(e).despawn();
            }
            spawn_capture_plan_modal(
                &mut commands,
                &asset_server,
                &theme,
                &form,
                UI_LAYER,
                scroll_y,
            );
            *last = (
                form.approach_transitions.len(),
                form.terminal_transitions.len(),
                form.overwrite_conflict_path.is_some(),
                form.validation_errors.len(),
                scroll_y,
                form.unit_system,
            );
        }
    }
}

fn poll_exit_confirm_modal(
    mut commands: Commands,
    mut pending: ResMut<ExitConfirmPending>,
    asset_server: Res<AssetServer>,
    theme: Res<UiTheme>,
) {
    if pending.0 {
        let font = asset_server.load("fonts/FiraMono-Medium.ttf");
        spawn_exit_confirm_modal(&mut commands, &font, &theme);
        pending.0 = false;
    }
}

fn poll_restart_prompt_modal(
    mut commands: Commands,
    mut form: ResMut<NewCapturePlanForm>,
    existing: Query<Entity, With<RestartPromptModal>>,
    asset_server: Res<AssetServer>,
    theme: Res<UiTheme>,
) {
    if form.show_restart_prompt && existing.is_empty() {
        let font = asset_server.load("fonts/FiraMono-Medium.ttf");
        spawn_restart_prompt_modal(&mut commands, &font, &theme);
        form.show_restart_prompt = false;
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

/// Pads the sim screen root so Bevy UI content stops above the egui terminal panel.
fn update_sim_screen_bottom_padding(
    log_ui: Res<CaptureLogUiState>,
    mut screen: Query<&mut Node, With<SimScreen>>,
) {
    for mut node in &mut screen {
        node.padding.bottom = Val::Px(log_ui.panel_height);
    }
}

/// Saves the plan form's JSON (v0-shaped) as a v1 plan file: migrated in memory,
/// validated, then written by the v1 writer. The error lists why it was not saved.
fn save_form_plan(
    dir: &std::path::Path,
    filename: &str,
    json: &serde_json::Value,
) -> Result<(), String> {
    let plan_id = filename.strip_suffix(".json").unwrap_or(filename);
    let report = read_plan_value(plan_id, json);
    let plan = report.plan.ok_or_else(|| report.errors.join("; "))?;
    save_plan(dir, filename, &plan).map_err(|e| e.to_string())
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
    let name = read_manifest_file(dir)
        .and_then(|report| report.manifest)
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
        Ok(manifest) => manifest,
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
    (pause, mut next_pause): (Res<State<PauseState>>, ResMut<NextState<PauseState>>),
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
                if let Err(e) = set_working_directory(&mut working_directory, path) {
                    warn!("Failed to create working directory: {e}");
                    log.write(LogEvent {
                        level: LogLevel::Error,
                        source: "ui",
                        message: format!("Failed to create working directory: {e}"),
                    });
                }
                next_screen.set(UiScreen::Home);
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
                    step_time_warp(*increase, world_time, &pause, &mut next_pause, &mut log);
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
                        let shrink_rate =
                            phase.shrink_rate_mps.map(number_text).unwrap_or_default();

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
                    form.editing_plan_id = Some(plan_id.clone());
                    // Snapshot the form state so we can detect unchanged edits on Save.
                    form.original_json = Some(build_capture_plan_json(&form));
                    log.write(LogEvent {
                        level: LogLevel::Info,
                        source: "ui",
                        message: format!("Opened '{}' for editing", plan_id),
                    });
                    form.open = true;
                }
            }
            UiEvent::SetUnitSystem(unit) => {
                form.unit_system = *unit;
            }
            UiEvent::ToggleCaptureGizmos => {
                settings.toggle_capture_gizmos();
            }
        }
    }
}

fn poll_home_plan_refresh(
    mut commands: Commands,
    mut user_plans_dirty: ResMut<UserPlansDirty>,
    capture_plan_lib: Res<CapturePlanLibrary>,
    mut capture_plan_load_errors: ResMut<CapturePlanLoadErrors>,
    home_screens: Query<Entity, With<HomeScreen>>,
    asset_server: Res<AssetServer>,
    theme: Res<UiTheme>,
    working_directory: Res<WorkingDirectory>,
) {
    if !user_plans_dirty.0 || home_screens.is_empty() {
        return;
    }
    // Re-scan the user plans directory for any newly broken or fixed files.
    let (_, user_load_errors) =
        load_plans_from_dir_with_errors(std::path::Path::new(&working_directory.path));
    capture_plan_load_errors.errors = user_load_errors;
    for entity in &home_screens {
        commands.entity(entity).despawn();
    }
    spawn_home_screen_inner(
        &mut commands,
        &asset_server,
        &theme,
        &capture_plan_lib,
        &working_directory.path,
        &capture_plan_load_errors,
    );
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
