use bevy::prelude::*;

#[derive(Message, Debug, Clone)]
pub enum UiEvent {
	OpenProject(String),
	BackToHome,
	CaptureDebris(Option<Entity>),
	ToggleMapView,
	TimeWarpIncrease,
	TimeWarpDecrease,
	ToggleOrigin,
}
