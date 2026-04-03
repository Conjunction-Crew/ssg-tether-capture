use bevy::prelude::*;

#[derive(Message, Debug, Clone)]
pub enum UiEvent {
    OpenProject(String),
    BackToHome,
    CaptureDebris {
        entity: Option<Entity>,
        plan_id: String,
    },
    ToggleMapView,
    ToggleOrigin,
    ChangeTimeWarp { increase: bool },
    CycleCameraTarget,
}
