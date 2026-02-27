use bevy::camera::CameraOutputMode;
use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy::render::render_resource::BlendState;

use crate::constants::UI_LAYER;
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
			.add_systems(Update, home_interactions)
			.add_systems(
				OnEnter(UiScreen::ProjectDetail),
				spawn_project_detail_screen,
			)
			.add_systems(OnExit(UiScreen::ProjectDetail), cleanup_project_detail_screen)
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
	mut ui_events: MessageReader<UiEvent>,
	mut next_screen: ResMut<NextState<UiScreen>>,
	mut selected_project: ResMut<SelectedProject>,
	project_catalog: Res<ProjectCatalog>,
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
		}
	}
}
