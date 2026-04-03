use avian3d::prelude::{Physics, RigidBody};
use bevy::camera::CameraOutputMode;
use bevy::camera::visibility::RenderLayers;
use bevy::pbr::{Atmosphere, AtmosphereSettings};
use bevy::prelude::*;
use bevy::render::render_resource::BlendState;

use avian3d::prelude::{Physics, PhysicsTime};

use crate::components::capture_components::CaptureComponent;
use crate::components::orbit::Orbital;
use crate::components::orbit_camera::CameraTarget;
use crate::constants::{MAP_LAYER, MAP_UNITS_TO_M, SCENE_LAYER, UI_LAYER};
use crate::resources::capture_plans::CapturePlanLibrary;
use crate::resources::settings::Settings;
use crate::resources::world_time::WorldTime;
use crate::systems::setup::setup_entities;
use crate::ui::events::UiEvent;
use crate::ui::screens::home::{
    cleanup_home_screen, home_interactions, spawn_home_screen, update_home_working_directory_label,
};
use crate::ui::screens::new_capture_plan::{
    build_capture_plan_json, cleanup_new_capture_plan_modal, generate_filename,
    new_capture_plan_interactions, spawn_new_capture_plan_modal, sync_form_fields, validate_form,
    NewCapturePlanModal,
};
use crate::ui::screens::project_detail::{
    cleanup_project_detail_screen, collapsible_toggle_interaction, project_detail_interactions,
    spawn_project_detail_screen,
};
use crate::ui::screens::working_directory_setup::{
    cleanup_working_directory_setup_screen, spawn_working_directory_setup_screen,
    working_directory_setup_interactions, DirectoryPathText,
};
use crate::ui::state::{ProjectCatalog, SelectedProject, UiScreen};
use crate::ui::theme::UiTheme;
use crate::ui::widgets::{input_field_display, input_field_interaction, input_field_keyboard};

#[derive(Resource, Default)]
struct FileDialogTask(Option<Task<Option<PathBuf>>>);

pub struct UiPlugin;

#[derive(Component)]
struct UiCamera;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<UiScreen>()
            .init_resource::<ProjectCatalog>()
            .init_resource::<SelectedProject>()
            .init_resource::<UiTheme>()
            .init_resource::<WorkingDirectory>()
            .init_resource::<NewCapturePlanForm>()
            .init_resource::<FileDialogTask>()
            .add_message::<UiEvent>()
            .add_systems(Startup, setup_ui_camera)
            .add_systems(
                OnEnter(UiScreen::WorkingDirectorySetup),
                spawn_working_directory_setup_screen,
            )
            .add_systems(
                OnExit(UiScreen::WorkingDirectorySetup),
                cleanup_working_directory_setup_screen,
            )
            .add_systems(Update, working_directory_setup_interactions)
            .add_systems(Update, poll_file_dialog_task)
            .add_systems(OnEnter(UiScreen::Home), spawn_home_screen)
            .add_systems(OnExit(UiScreen::Home), cleanup_home_screen)
            .add_systems(
                OnEnter(UiScreen::Sim),
                spawn_project_detail_screen.after(setup_entities),
            )
            .add_systems(Update, home_interactions)
            .add_systems(Update, update_home_working_directory_label)
            .add_systems(OnExit(UiScreen::Sim), cleanup_project_detail_screen)
            .add_systems(Update, project_detail_interactions)
            .add_systems(Update, collapsible_toggle_interaction)
            .add_systems(Update, input_field_interaction)
            .add_systems(Update, input_field_keyboard)
            .add_systems(Update, input_field_display)
            .add_systems(Update, sync_form_fields)
            .add_systems(Update, new_capture_plan_interactions)
            .add_systems(Update, poll_new_plan_modal)
            .add_systems(Update, handle_ui_events);
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
    modals: Query<Entity, With<NewCapturePlanModal>>,
) {
    if !form.is_changed() {
        return;
    }
    let modal_exists = !modals.is_empty();
    if form.open && !modal_exists {
        spawn_new_capture_plan_modal(&mut commands, &asset_server, &theme, &form, UI_LAYER);
    } else if !form.open && modal_exists {
        for e in &modals {
            commands.entity(e).despawn();
        }
    } else if form.open && modal_exists {
        // Re-render the modal when form state changes (e.g. transitions added/removed)
        for e in &modals {
            commands.entity(e).despawn();
        }
        spawn_new_capture_plan_modal(&mut commands, &asset_server, &theme, &form, UI_LAYER);
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
    project_catalog: Res<ProjectCatalog>,
    capture_entities: Query<Entity, With<CaptureComponent>>,
    mut scene_camera: Query<
        (&mut RenderLayers, &mut Atmosphere, &mut AtmosphereSettings),
        Without<UiCamera>,
    >,
    bodies: Query<(Entity, Has<CameraTarget>), (With<RigidBody>, With<Orbital>)>,
    mut settings: ResMut<Settings>,
) {
    for event in ui_events.read() {
        match event {
            UiEvent::OpenProject(project_id) => {
                if project_catalog
                    .projects
                    .iter()
                    .any(|project| project.id == *project_id)
                {
                    selected_project.project_id = Some(project_id.clone());
                    next_screen.set(UiScreen::Sim);
                }
            }
            UiEvent::BackToHome => {
                next_screen.set(UiScreen::Home);
            }
            UiEvent::WorkingDirectorySelected(path) => {
                working_directory.path = path.clone();
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
                println!("Trying to capture");
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
                            commands.entity(*capture_entity).insert(CaptureComponent {
                                plan_id: plan.name.clone(),
                                current_state: plan
                                    .states
                                    .get(0)
                                    .expect("No states in the desired plan!")
                                    .id
                                    .clone(),
                                state_enter_time_s: physics_time.elapsed_secs_f64(),
                                state_elapsed_time_s: 0.0,
                            });
                        }
                    } else {
                        println!("entity already marked for capture!");
                    }
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
            UiEvent::ToggleOrigin => {
                settings.dev_gizmos = !settings.dev_gizmos;
            }
            UiEvent::ChangeTimeWarp { increase } => {
                if let Some(ref mut world_time) = world_time {
                    const MAX_TIME_WARP: u32 = 10000;
                    const MIN_TIME_WARP: u32 = 1;
                    if *increase && world_time.multiplier * 2 <= MAX_TIME_WARP {
                        world_time.multiplier *= 2;
                    } else if !increase && world_time.multiplier / 2 >= MIN_TIME_WARP {
                        world_time.multiplier /= 2;
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
                }
            }
            _ => {}
        }
    }
}
