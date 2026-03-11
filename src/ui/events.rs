use bevy::prelude::*;

#[derive(Message, Debug, Clone)]
pub enum UiEvent {
	OpenProject(String),
	BackToHome,
	CaptureDebris(Option<Entity>),
}
