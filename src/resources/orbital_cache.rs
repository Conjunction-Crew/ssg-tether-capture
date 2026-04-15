use bevy::{platform::collections::HashMap, prelude::*};
use brahe::KeplerianPropagator;
use nalgebra::Vector6;

#[derive(Resource, Debug, Default)]
pub struct OrbitalCache {
    pub tethers: HashMap<String, Vec<Entity>>,
    pub debris: HashMap<String, Entity>,
    pub eci_states: HashMap<Entity, Vector6<f64>>,
}
