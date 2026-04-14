use std::path::PathBuf;

use avian3d::prelude::{Physics, RigidBody};
use bevy::camera::CameraOutputMode;
use bevy::camera::visibility::RenderLayers;
use bevy::ecs::system::SystemParam;
use bevy::pbr::{Atmosphere, AtmosphereSettings};
use bevy::prelude::*;
use bevy::render::render_resource::BlendState;
use bevy::tasks::{AsyncComputeTaskPool, Task, block_on, futures_lite::future};
use bevy_egui::EguiPrimaryContextPass;

use crate::components::capture_components::{CaptureComponent, CapturePlan};
use crate::components::orbit::Orbital;
use crate::components::orbit_camera::CameraTarget;
use crate::constants::{MAP_LAYER, MAP_UNITS_TO_M, SCENE_LAYER, UI_LAYER};
use crate::resources::capture_log::{CaptureLog, CaptureLogUiState, LogEntry, LogEvent, LogLevel};
use crate::resources::capture_plan_form::{
    NewCapturePlanForm, SimPlanSyncState, TransitionForm, UnitSystem,
};
use crate::resources::capture_plans::{
    CapturePlanLibrary, CapturePlanLoadErrors, build_capture_component, load_plans_from_dir,
    load_plans_from_dir_with_errors,
};
use crate::resources::data_collection::DataCollection;
use crate::resources::settings::Settings;
use crate::resources::working_directory::{WorkingDirectory, save_to_config};
use crate::resources::world_time::WorldTime;
use crate::systems::setup::setup_entities;
use crate::ui::egui::egui_plots;
use crate::ui::egui_terminal::egui_terminal_panel;
use crate::ui::events::UiEvent;
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
    RestartPromptModal, catalog_interactions, catalog_keyboard_input,
    cleanup_project_detail_screen, collapsible_toggle_interaction, project_detail_interactions,
    refresh_space_catalog_results, reset_space_catalog_ui_state, restart_prompt_interactions,
    spawn_exit_confirm_modal, spawn_project_detail_screen, spawn_restart_prompt_modal,
    sync_space_catalog_ui, update_satellite_indicator_overlay, update_selected_catalog_overlay,
    update_sync_indicator, view_edit_plan_interactions,
};
use crate::ui::screens::working_directory_setup::{
    DirectoryPathText, cleanup_working_directory_setup_screen,
    spawn_working_directory_setup_screen, working_directory_setup_interactions,
};
use crate::ui::state::{SelectedProject, UiScreen};
use crate::ui::theme::UiTheme;
use crate::ui::widgets::{
    ClipboardRes, input_field_display, input_field_interaction, input_field_keyboard,
};

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
        app.init_state::<UiScreen>()
            .init_resource::<SelectedProject>()
            .init_resource::<UiTheme>()
            .init_resource::<WorkingDirectory>()
            .init_resource::<NewCapturePlanForm>()
            .init_resource::<FileDialogTask>()
            .init_resource::<UserPlansDirty>()
            .init_resource::<ExitConfirmPending>()
            .init_resource::<SimPlanSyncState>()
            .init_resource::<CapturePlanLoadErrors>()
            .init_resource::<DataCollection>()
            .init_resource::<CaptureLog>()
            .init_resource::<CaptureLogUiState>()
            .init_non_send_resource::<ClipboardRes>()
            .add_message::<UiEvent>()
            .add_message::<LogEvent>()
            .add_systems(Startup, setup_ui_camera)
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
                    spawn_project_detail_screen.after(setup_entities),
                    reset_sync_state,
                    reset_terminal_log_ui_state,
                ),
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
                    collect_log_events.run_if(in_state(UiScreen::Sim)),
                    (
                        project_detail_interactions,
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
                    handle_ui_events,
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
        Camera2d::default(),
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
    if let Some(ref mut task) = file_dialog_task.0 {
        if let Some(result) = block_on(future::poll_once(task)) {
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
    mut next_screen: ResMut<NextState<UiScreen>>,
) {
    if sync_state.restart_requested && *screen.get() == UiScreen::Home {
        sync_state.restart_requested = false;
        next_screen.set(UiScreen::Sim);
    }
}

fn reset_sync_state(mut sync_state: ResMut<SimPlanSyncState>) {
    *sync_state = SimPlanSyncState::default();
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

/// Collects [`LogEvent`]s emitted this frame and appends them to [`CaptureLog`].
pub fn collect_log_events(
    mut events: MessageReader<LogEvent>,
    mut capture_log: ResMut<CaptureLog>,
    world_time: Res<WorldTime>,
) {
    for event in events.read() {
        let full_ts = format!("{}", world_time.epoch);
        // Extract HH:MM:SS from ISO timestamp (chars 11–19 of "YYYY-MM-DDTHH:MM:SS…").
        let timestamp = full_ts
            .get(11..19)
            .map(|s| s.to_string())
            .unwrap_or_else(|| full_ts.clone());
        capture_log.push(LogEntry {
            timestamp,
            level: event.level,
            source: event.source,
            message: event.message.clone(),
        });
    }
}

fn handle_ui_events(
    mut commands: Commands,
    mut ui_events: MessageReader<UiEvent>,
    mut next_screen: ResMut<NextState<UiScreen>>,
    mut selected_project: ResMut<SelectedProject>,
    mut working_directory: ResMut<WorkingDirectory>,
    mut file_dialog_task: ResMut<FileDialogTask>,
    mut form: ResMut<NewCapturePlanForm>,
    mut capture_plan_lib: ResMut<CapturePlanLibrary>,
    mut world_time: Option<ResMut<WorldTime>>,
    physics_time: Res<Time<Physics>>,
    capture_entities: Query<Entity, With<CaptureComponent>>,
    mut scene_camera: Query<
        (&mut RenderLayers, &mut Atmosphere, &mut AtmosphereSettings),
        Without<UiCamera>,
    >,
    bodies: Query<(Entity, Has<CameraTarget>), (With<RigidBody>, With<Orbital>)>,
    ui_runtime: (
        ResMut<Settings>,
        ResMut<DataCollection>,
    ),
    mut flow: UiFlowState,
    mut log: MessageWriter<LogEvent>,
) {
    let (mut settings, mut data_collection) = ui_runtime;

    for event in ui_events.read() {
        match event {
            UiEvent::OpenProject(project_id) => {
                if capture_plan_lib.plans.contains_key(project_id.as_str()) {
                    selected_project.project_id = Some(project_id.clone());
                    next_screen.set(UiScreen::Sim);
                }
            }
            UiEvent::BackToHome => {
                next_screen.set(UiScreen::Home);
            }
            UiEvent::ShowExitConfirm => {
                flow.exit_confirm.0 = true;
            }
            UiEvent::CancelExitConfirm => {
                flow.exit_confirm.0 = false;
            }
            UiEvent::WorkingDirectorySelected(path) => {
                working_directory.path = path.clone();
                save_to_config(&path);
                if let Err(e) = std::fs::create_dir_all(path) {
                    eprintln!("Failed to create working directory: {e}");
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
                if let Some(capture_entity) = entity {
                    // Check if the entity is not already marked for capture
                    if !capture_entities.contains(*capture_entity) {
                        // Remove CaptureComponent from entities (if any)
                        for marked_entity in capture_entities {
                            commands.entity(marked_entity).remove::<CaptureComponent>();
                        }
                        // Get plan information
                        if let Some(plan) = capture_plan_lib.plans.get(plan_id) {
                            // Now, mark the entity for capture
                            if let Some(capture_component) = build_capture_component(
                                plan_id.as_str(),
                                plan,
                                physics_time.elapsed_secs_f64(),
                            ) {
                                commands.entity(*capture_entity).insert(capture_component);
                                log.write(LogEvent {
                                    level: LogLevel::Info,
                                    source: "ui",
                                    message: format!(
                                        "Capture initiated for entity {capture_entity:?} with plan '{plan_id}'"
                                    ),
                                });
                            } else {
                                log.write(LogEvent {
                                    level: LogLevel::Error,
                                    source: "ui",
                                    message: format!(
                                        "Failed to initiate capture for entity {capture_entity:?} with plan '{plan_id}'"
                                    ),
                                });
                                continue;
                            }

                            // Begin data collection on entity
                            data_collection
                                .position
                                .insert(*capture_entity, Vec::<(f64, f64)>::new());
                            data_collection
                                .velocity
                                .insert(*capture_entity, Vec::<(f64, f64)>::new());
                        } else {
                            log.write(LogEvent {
                                level: LogLevel::Error,
                                source: "ui",
                                message: format!("Capture plan '{plan_id}' not found"),
                            });
                        }
                    } else {
                        log.write(LogEvent {
                            level: LogLevel::Warn,
                            source: "ui",
                            message: format!(
                                "Entity {capture_entity:?} already marked for capture — ignoring"
                            ),
                        });
                    }
                } else {
                    log.write(LogEvent {
                        level: LogLevel::Warn,
                        source: "ui",
                        message: format!(
                            "Capture initiated with no target entity for plan '{plan_id}'"
                        ),
                    });
                }
            }
            UiEvent::ToggleMapView => {
                if let Ok((mut render_layers, mut atmosphere, mut atmosphere_settings)) =
                    scene_camera.single_mut()
                {
                    if render_layers.intersects(&RenderLayers::layer(SCENE_LAYER)) {
                        *render_layers = RenderLayers::layer(MAP_LAYER);
                        atmosphere.world_position = Vec3::ZERO;
                        atmosphere_settings.scene_units_to_m = MAP_UNITS_TO_M as f32;
                    } else if render_layers.intersects(&RenderLayers::layer(MAP_LAYER)) {
                        *render_layers = RenderLayers::layer(SCENE_LAYER);
                        atmosphere_settings.scene_units_to_m = 1.0;
                    }
                }
            }
            UiEvent::ChangeTimeWarp { increase } => {
                if let Some(ref mut world_time) = world_time {
                    const MAX_TIME_WARP: u32 = 10000;
                    const MIN_TIME_WARP: u32 = 1;
                    let prev = world_time.multiplier;
                    if *increase && world_time.multiplier * 2 <= MAX_TIME_WARP {
                        world_time.multiplier *= 2;
                    } else if !increase && world_time.multiplier / 2 >= MIN_TIME_WARP {
                        world_time.multiplier /= 2;
                    }
                    if world_time.multiplier != prev {
                        log.write(LogEvent {
                            level: LogLevel::Info,
                            source: "ui",
                            message: format!("Time warp → {}x", world_time.multiplier),
                        });
                    }
                }
            }
            UiEvent::CycleCameraTarget => {
                let mut entities: Vec<(Entity, bool)> = bodies.iter().collect();
                if !entities.is_empty() {
                    entities.sort_by_key(|(entity, _)| entity.index());
                    let current_index = entities
                        .iter()
                        .position(|(_, is_target)| *is_target)
                        .unwrap_or(0);
                    let next_target = entities[(current_index + 1) % entities.len()].0;
                    for (entity, is_target) in &entities {
                        if *is_target {
                            commands.entity(*entity).remove::<CameraTarget>();
                        }
                    }
                    commands.entity(next_target).insert(CameraTarget);
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
                            match serde_json::to_string_pretty(&json) {
                                Ok(content) => {
                                    if let Err(e) = std::fs::write(&dest, content) {
                                        log.write(LogEvent {
                                            level: LogLevel::Error,
                                            source: "ui",
                                            message: format!(
                                                "Failed to write capture plan '{}': {e}",
                                                form.plan_name
                                            ),
                                        });
                                    } else {
                                        // Reload user plans
                                        let new_user_plans = load_plans_from_dir(
                                            std::path::Path::new(&working_directory.path),
                                        );
                                        capture_plan_lib.user_plans = new_user_plans;
                                        capture_plan_lib.plans = capture_plan_lib
                                            .example_plans
                                            .iter()
                                            .chain(capture_plan_lib.user_plans.iter())
                                            .map(|(k, v)| (k.clone(), v.clone()))
                                            .collect();
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
                                        // Always recompile after save so compiled_plans
                                        // stays in sync. When editing from the sim with
                                        // sim-affecting changes, we still prompt the user
                                        // to restart, but capture will not fail with stale data.
                                        let refreshed: Vec<(String, CapturePlan)> =
                                            capture_plan_lib
                                                .plans
                                                .iter()
                                                .map(|(k, v)| (k.clone(), v.clone()))
                                                .collect();
                                        for (id, plan) in refreshed {
                                            capture_plan_lib.insert_plan(id, plan);
                                        }
                                        flow.plans_dirty.0 = true;
                                        log.write(LogEvent {
                                            level: LogLevel::Info,
                                            source: "ui",
                                            message: format!(
                                                "Capture plan '{}' saved",
                                                form.plan_name
                                            ),
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
                                Err(e) => {
                                    log.write(LogEvent {
                                        level: LogLevel::Error,
                                        source: "ui",
                                        message: format!(
                                            "Failed to serialize capture plan '{}': {e}",
                                            form.plan_name
                                        ),
                                    });
                                }
                            }
                        }
                    }
                } // end else (form was changed)
            }
            UiEvent::ConfirmOverwriteCapturePlan => {
                let filename = generate_filename(&form.plan_name);
                let dest = std::path::Path::new(&working_directory.path).join(&filename);
                let json = build_capture_plan_json(&form);
                match serde_json::to_string_pretty(&json) {
                    Ok(content) => {
                        if let Err(e) = std::fs::write(&dest, content) {
                            log.write(LogEvent {
                                level: LogLevel::Error,
                                source: "ui",
                                message: format!(
                                    "Failed to write capture plan '{}': {e}",
                                    form.plan_name
                                ),
                            });
                        } else {
                            let new_user_plans =
                                load_plans_from_dir(std::path::Path::new(&working_directory.path));
                            capture_plan_lib.user_plans = new_user_plans;
                            capture_plan_lib.plans = capture_plan_lib
                                .example_plans
                                .iter()
                                .chain(capture_plan_lib.user_plans.iter())
                                .map(|(k, v)| (k.clone(), v.clone()))
                                .collect();
                            let refreshed: Vec<(String, CapturePlan)> = capture_plan_lib
                                .plans
                                .iter()
                                .map(|(k, v)| (k.clone(), v.clone()))
                                .collect();
                            for (id, plan) in refreshed {
                                capture_plan_lib.insert_plan(id, plan);
                            }
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
                    Err(e) => {
                        log.write(LogEvent {
                            level: LogLevel::Error,
                            source: "ui",
                            message: format!(
                                "Failed to serialize capture plan '{}': {e}",
                                form.plan_name
                            ),
                        });
                    }
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
                        form.tether_type = device.device_type.clone();
                        if device.tether_length > 0.0 {
                            form.tether_length = device.tether_length.to_string();
                        }
                    }
                    for state in &plan.states {
                        let transitions: Vec<TransitionForm> = state
                            .transitions
                            .as_ref()
                            .map(|trans| {
                                trans
                                    .iter()
                                    .filter_map(|t| {
                                        let to = t.get("to")?.as_str()?.to_string();
                                        let dist = t.get("distance")?;
                                        let (kind, val) = if let Some(v) = dist.get("less_than") {
                                            ("less_than".to_string(), v.to_string())
                                        } else if let Some(v) = dist.get("greater_than") {
                                            ("greater_than".to_string(), v.to_string())
                                        } else {
                                            return None;
                                        };
                                        Some(TransitionForm {
                                            to,
                                            distance_kind: kind,
                                            distance_value: val,
                                        })
                                    })
                                    .collect()
                            })
                            .unwrap_or_default();

                        match state.id.as_str() {
                            "approach" => {
                                if let Some(params) = &state.parameters {
                                    form.approach_max_velocity = params
                                        .get("max_velocity")
                                        .map(|v| v.to_string())
                                        .unwrap_or_default();
                                    form.approach_max_force = params
                                        .get("max_force")
                                        .map(|v| v.to_string())
                                        .unwrap_or_default();
                                }
                                form.approach_transitions = transitions;
                            }
                            "terminal" => {
                                if let Some(params) = &state.parameters {
                                    form.terminal_max_velocity = params
                                        .get("max_velocity")
                                        .map(|v| v.to_string())
                                        .unwrap_or_default();
                                    form.terminal_max_force = params
                                        .get("max_force")
                                        .map(|v| v.to_string())
                                        .unwrap_or_default();
                                    form.terminal_shrink_rate = params
                                        .get("shrink_rate")
                                        .map(|v| v.to_string())
                                        .unwrap_or_default();
                                }
                                form.terminal_transitions = transitions;
                            }
                            "capture" => {
                                if let Some(params) = &state.parameters {
                                    form.capture_max_velocity = params
                                        .get("max_velocity")
                                        .map(|v| v.to_string())
                                        .unwrap_or_default();
                                    form.capture_max_force = params
                                        .get("max_force")
                                        .map(|v| v.to_string())
                                        .unwrap_or_default();
                                    form.capture_shrink_rate = params
                                        .get("shrink_rate")
                                        .map(|v| v.to_string())
                                        .unwrap_or_default();
                                }
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
                settings.capture_gizmos = !settings.capture_gizmos;
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
