use bevy::{platform::collections::HashMap, prelude::*};

#[derive(Resource, Debug, Default)]
pub struct Celestials {
    pub planets: HashMap<String, Entity>,
}
