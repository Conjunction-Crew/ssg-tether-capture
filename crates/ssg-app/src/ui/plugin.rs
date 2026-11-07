use bevy::camera::CameraOutputMode;
use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy::render::render_resource::BlendState;
use bevy::transform::TransformSystems;
use bevy_egui::{EguiPreUpdateSet, EguiPrimaryContextPass};

use crate::render::camera::setup_camera;
use crate::render::layers::UI_LAYER;
use crate::ui::catalog_state::{FilteredSpaceCatalogResults, SpaceCatalogUiState};
use crate::ui::egui::{CsvExportState, begin_plot_session, egui_plots};
use crate::ui::egui_terminal::egui_terminal_panel;
use crate::ui::egui_theme::apply_egui_theme;
use crate::ui::events::UiEvent;
use crate::ui::events_handler::{
    FileDialogTask, UserPlansDirty, handle_ui_events, poll_file_dialog_task, poll_home_plan_refresh,
};
use crate::ui::form::{NewCapturePlanForm, SimPlanSyncState};
use crate::ui::hud::{
    HudReadouts, init_hud_readouts, update_capture_guidance, update_capture_telemetry,
    update_time_warp_readout,
};
use crate::ui::input::{
    change_time_warp, egui_wants_keyboard, keep_tab_from_egui, toggle_capture_gizmos,
    toggle_map_view, toggle_origin,
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
use crate::ui::screens::capture_plan::capture_plan_form_window;
use crate::ui::screens::home::{home_screen, load_home_plans};
use crate::ui::screens::project_detail::{
    ExitConfirmOpen, cleanup_project_detail_screen, refresh_space_catalog_results,
    reset_space_catalog_ui_state, restore_catalog_sources, sim_screen_events, sim_screen_modals,
    sim_screen_panels, space_catalog_panel, spawn_project_detail_screen,
};
use crate::ui::screens::working_directory_setup::{
    WorkingDirectorySetupError, clear_working_directory_setup_error, working_directory_setup_screen,
};
use crate::ui::settings::Settings;
use crate::ui::theme::UiTheme;
use ssg_sim::SimSet;
use ssg_sim::project::WorkingDirectory;
use ssg_sim::session::SimState;

pub struct UiPlugin;

#[derive(Component)]
pub(crate) struct UiCamera;

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
