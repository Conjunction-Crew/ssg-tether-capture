use bevy::{platform::collections::HashMap, prelude::*};

#[derive(Resource, Debug, Default)]
pub struct Devices {
    pub tethers: HashMap<String, Entity>
}