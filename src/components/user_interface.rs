use bevy::prelude::*;

#[derive(Component, Debug, Clone)]
pub struct TrackObject {
    pub entity: Option<Entity>,
}

impl Default for TrackObject {
    fn default() -> Self {
        Self { entity: None }
    }
}

#[derive(Component, Debug, Clone)]
pub struct OrbitLabel {
    pub entity: Option<Entity>,
}

impl Default for OrbitLabel {
    fn default() -> Self {
        Self { entity: None }
    }
}
