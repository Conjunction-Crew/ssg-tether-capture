use bevy::{platform::collections::HashMap, prelude::*};
use brahe::{KeplerianPropagator};

#[derive(Resource, Debug, Default)]
pub struct OrbitalEntities {
    pub tethers: HashMap<String, Vec<Entity>>,
    pub debris: HashMap<String, Entity>,
    pub propagators: Vec<KeplerianPropagator>,
}
