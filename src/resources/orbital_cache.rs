use bevy::{math::DVec3, platform::collections::HashMap, prelude::*};
use nalgebra::Vector6;

#[derive(Resource, Debug, Default)]
pub struct OrbitalCache {
    pub tethers: HashMap<String, Vec<Entity>>,
    pub debris: HashMap<String, Entity>,
    pub eci_states: HashMap<Entity, Vector6<f64>>,
    pub com_rv: HashMap<Entity, (DVec3, DVec3)>,
}
