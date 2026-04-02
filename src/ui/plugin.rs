use avian3d::prelude::Physics;
use bevy::camera::CameraOutputMode;
use bevy::camera::visibility::RenderLayers;
use bevy::pbr::{Atmosphere, AtmosphereSettings};
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::render::render_resource::BlendState;

use crate::components::capture_components::CaptureComponent;
use crate::constants::UI_LAYER;
use crate::resources::capture_plans::CapturePlanLibrary;
use crate::systems::setup::setup_entities;
use crate::ui::events::UiEvent;
use crate::ui::screens::home::{cleanup_home_screen, home_interactions, spawn_home_screen};
use crate::ui::screens::project_detail::{
    cleanup_project_detail_screen, project_detail_interactions, spawn_project_detail_screen,
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
            .add_systems(Update, project_detail_interactions)
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
    physics_time: Res<Time<Physics>>,
    project_catalog: Res<ProjectCatalog>,
    capture_plans: Res<CapturePlanLibrary>,
    capture_entities: Query<Entity, With<CaptureComponent>>,
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
                    // We will do nothing if the selected entity is already marked for capture
                    // TODO: button should probably be removed to prevent being able to make this call to
                    // the same entity?
                }
            }
        }
    }
}
