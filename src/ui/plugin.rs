use avian3d::prelude::{Physics, RigidBody};
use bevy::camera::CameraOutputMode;
use bevy::camera::visibility::RenderLayers;
use bevy::pbr::{Atmosphere, AtmosphereSettings};
use bevy::prelude::*;
use bevy::render::render_resource::BlendState;
use bevy::transform::TransformSystems;

use crate::components::capture_components::CaptureComponent;
use crate::components::orbit::Orbital;
use crate::components::orbit_camera::CameraTarget;
use crate::constants::{MAP_LAYER, MAP_UNITS_TO_M, SCENE_LAYER, UI_LAYER};
use crate::resources::capture_plans::CapturePlanLibrary;
use crate::resources::settings::Settings;
use crate::resources::world_time::WorldTime;
use crate::systems::setup::setup_entities;
use crate::ui::events::UiEvent;
use crate::ui::screens::home::{cleanup_home_screen, home_interactions, spawn_home_screen};
use crate::ui::screens::project_detail::{
    catalog_interactions, catalog_keyboard_input, cleanup_project_detail_screen,
    collapsible_toggle_interaction, project_detail_interactions,
    refresh_space_catalog_results, reset_space_catalog_ui_state, spawn_project_detail_screen,
    sync_space_catalog_ui, update_satellite_indicator_overlay, update_selected_catalog_overlay,
};
use crate::ui::state::{ProjectCatalog, SelectedProject, UiScreen};
use crate::ui::theme::UiTheme;

pub struct UiPlugin;

#[derive(Component)]
struct UiCamera;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<UiScreen>()
            .init_resource::<ProjectCatalog>()
            .init_resource::<SelectedProject>()
            .init_resource::<UiTheme>()
            .add_message::<UiEvent>()
            .add_systems(Startup, setup_ui_camera)
            .add_systems(OnEnter(UiScreen::Home), spawn_home_screen)
            .add_systems(OnExit(UiScreen::Home), cleanup_home_screen)
            .add_systems(
                OnEnter(UiScreen::Sim),
                spawn_project_detail_screen.after(setup_entities),
            )
            .add_systems(Update, home_interactions)
            .add_systems(OnExit(UiScreen::Sim), cleanup_project_detail_screen)
            .add_systems(OnExit(UiScreen::Sim), reset_space_catalog_ui_state)
            .add_systems(
                Update,
                (
                    project_detail_interactions,
                    collapsible_toggle_interaction,
                    catalog_interactions,
                    catalog_keyboard_input,
                    refresh_space_catalog_results,
                    sync_space_catalog_ui,
                )
                    .chain()
                    .run_if(in_state(UiScreen::Sim)),
            )
            .add_systems(
                PostUpdate,
                (update_selected_catalog_overlay, update_satellite_indicator_overlay)
                    .chain()
                    .after(sync_space_catalog_ui)
                    .after(TransformSystems::Propagate)
                    .run_if(in_state(UiScreen::Sim)),
            )
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

fn handle_ui_events(
    mut commands: Commands,
    mut ui_events: MessageReader<UiEvent>,
    mut next_screen: ResMut<NextState<UiScreen>>,
    mut selected_project: ResMut<SelectedProject>,
    mut world_time: Option<ResMut<WorldTime>>,
    physics_time: Res<Time<Physics>>,
    project_catalog: Res<ProjectCatalog>,
    capture_plans: Res<CapturePlanLibrary>,
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
                        if let Some(plan) = capture_plans.plans.get(plan_id) {
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
        }
    }
}
