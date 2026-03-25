use bevy::camera::CameraOutputMode;
use bevy::camera::visibility::RenderLayers;
use bevy::pbr::{Atmosphere, AtmosphereSettings};
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::render::render_resource::BlendState;
use bevy::ui::InteractionDisabled;
use bevy::ui_widgets::{CoreSliderDragState, SliderRange, SliderValue};

use avian3d::prelude::{Physics, PhysicsTime};

use crate::components::capture_components::CaptureComponent;
use crate::components::dev_components::Origin;
use crate::constants::{MAP_LAYER, MAP_UNITS_TO_M, SCENE_LAYER, UI_LAYER};
use crate::resources::capture_plans::CapturePlanLibrary;
use crate::resources::world_time::WorldTime;
use crate::ui::events::UiEvent;
use crate::ui::screens::home::{cleanup_home_screen, home_interactions, spawn_home_screen};
use crate::ui::screens::project_detail::{
    RadiusSlider, RadiusSliderThumb, TimeWarpLabel, cleanup_project_detail_screen,
    project_detail_interactions, spawn_project_detail_screen,
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
            .add_systems(Update, home_interactions)
            .add_systems(
                OnEnter(UiScreen::ProjectDetail),
                spawn_project_detail_screen,
            )
            .add_systems(
                OnExit(UiScreen::ProjectDetail),
                cleanup_project_detail_screen,
            )
            .add_systems(Update, project_detail_interactions)
            .add_systems(Update, handle_ui_events)
            .add_systems(Update, handle_simulation_control_events)
            .add_systems(Update, update_slider_style)
            .add_systems(Update, update_time_warp_label);
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
    time: Res<Time>,
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
                    next_screen.set(UiScreen::ProjectDetail);
                }
            }
            UiEvent::BackToHome => {
                next_screen.set(UiScreen::Home);
            }
            UiEvent::CaptureDebris(entity) => {
                println!("Trying to capture");
                if let Some(capture_entity) = entity {
                    // Check if the entity is not already marked for capture
                    if !capture_entities.contains(*capture_entity) {
                        // Remove CaptureComponent from entities (if any)
                        for marked_entity in capture_entities {
                            commands.entity(marked_entity).remove::<CaptureComponent>();
                        }
                        // Get plan information
                        if let Some(plan) = capture_plans.plans.get("example_plan") {
                            // Now, mark the entity for capture
                            commands.entity(*capture_entity).insert(CaptureComponent {
                                plan_id: plan.name.clone(),
                                current_state: plan
                                    .states
                                    .get(0)
                                    .expect("No states in the desired plan!")
                                    .id
                                    .clone(),
                                state_enter_time_s: time.elapsed_secs_f64(),
                                state_elapsed_time_s: time.elapsed_secs_f64(),
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
            _ => {}
        }
    }
}

fn handle_simulation_control_events(
    mut commands: Commands,
    mut ui_events: MessageReader<UiEvent>,
    mut scene_camera: Single<(&mut RenderLayers, &mut Atmosphere, &mut AtmosphereSettings)>,
    mut world_time: ResMut<WorldTime>,
    mut physics_time: ResMut<Time<Physics>>,
    origin: Single<(Entity, &Visibility), With<Origin>>,
) {
    let (origin_entity, origin_vis) = origin.into_inner();
    let (ref mut render_layers, ref mut atmosphere, ref mut atmosphere_settings) =
        *scene_camera;

    for event in ui_events.read() {
        match event {
            UiEvent::ToggleMapView => {
                if render_layers.intersects(&RenderLayers::layer(SCENE_LAYER)) {
                    **render_layers = RenderLayers::layer(MAP_LAYER);
                    atmosphere.world_position = Vec3::ZERO;
                    atmosphere_settings.scene_units_to_m = MAP_UNITS_TO_M;
                } else if render_layers.intersects(&RenderLayers::layer(MAP_LAYER)) {
                    **render_layers = RenderLayers::layer(SCENE_LAYER);
                    atmosphere_settings.scene_units_to_m = 1.0;
                }
            }
            UiEvent::TimeWarpIncrease => {
                if world_time.multiplier * 2.0 <= MAX_TIME_WARP {
                    world_time.multiplier *= 2.0;
                }
                sync_physics_time(&world_time, &mut physics_time);
            }
            UiEvent::TimeWarpDecrease => {
                if world_time.multiplier / 2.0 >= MIN_TIME_WARP {
                    world_time.multiplier /= 2.0;
                }
                sync_physics_time(&world_time, &mut physics_time);
            }
            UiEvent::ToggleOrigin => {
                match origin_vis {
                    Visibility::Visible => {
                        commands.entity(origin_entity).insert(Visibility::Hidden);
                    }
                    Visibility::Hidden => {
                        commands.entity(origin_entity).insert(Visibility::Visible);
                    }
                    Visibility::Inherited => {}
                }
            }
            _ => {}
        }
    }
}

fn sync_physics_time(world_time: &WorldTime, physics_time: &mut Time<Physics>) {
    if world_time.multiplier > 4.0 {
        physics_time.pause();
    } else {
        physics_time.unpause();
        physics_time.set_relative_speed_f64(world_time.multiplier as f64);
    }
}

const MAX_TIME_WARP: f64 = 1000.0;
const MIN_TIME_WARP: f64 = 0.001;

fn update_time_warp_label(
    mut labels: Query<&mut Text, With<TimeWarpLabel>>,
    world_time: Res<WorldTime>,
) {
    for mut text in &mut labels {
        **text = format!("{}x", world_time.multiplier);
    }
}

const SLIDER_TRACK: Color = Color::srgb(0.059, 0.078, 0.133);
const SLIDER_THUMB: Color = Color::srgb(0.137, 0.286, 0.914);
const ELEMENT_FILL_DISABLED: Color = Color::srgb(0.5019608, 0.5019608, 0.5019608);


fn thumb_color(disabled: bool, hovered: bool) -> Color {
    match (disabled, hovered) {
        (true, _) => ELEMENT_FILL_DISABLED,

        (false, true) => SLIDER_THUMB.lighter(0.3),

        _ => SLIDER_THUMB,
    }
}

fn update_slider_style(
    sliders: Query<
        (
            Entity,
            &SliderValue,
            &SliderRange,
            &Hovered,
            &CoreSliderDragState,
            Has<InteractionDisabled>,
        ),
        (
            Or<(
                Changed<SliderValue>,
                Changed<SliderRange>,
                Changed<Hovered>,
                Changed<CoreSliderDragState>,
                Added<InteractionDisabled>,
            )>,
            With<RadiusSlider>,
        ),
    >,
    children: Query<&Children>,
    mut thumbs: Query<
        (&mut Node, &mut BackgroundColor, Has<RadiusSliderThumb>),
        Without<RadiusSlider>,
    >,
) {
    for (slider_ent, value, range, hovered, drag_state, disabled) in sliders.iter() {
        for child in children.iter_descendants(slider_ent) {
            if let Ok((mut thumb_node, mut thumb_bg, is_thumb)) = thumbs.get_mut(child)
                && is_thumb
            {
                thumb_node.left = percent(range.thumb_position(value.0) * 100.0);
                thumb_bg.0 = thumb_color(disabled, hovered.0 | drag_state.dragging);
            }
        }
    }
}
