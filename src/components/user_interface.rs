use bevy::prelude::*;

#[derive(Component, Debug, Clone)]
pub struct TrackObject {
    pub entity: Option<Entity>
}

impl Default for TrackObject {
    fn default() -> Self {
        Self {
            entity: None,
        }
    }
}