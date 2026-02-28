use bevy::{platform::collections::HashMap, prelude::*};

#[derive(Resource, Debug, Default)]
pub struct OrbitalEntities {
    pub tethers: HashMap<String, Vec<Entity>>,
    pub debris: HashMap<String, Entity>,
}
