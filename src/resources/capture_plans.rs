use bevy::{platform::collections::HashMap, prelude::*};

use crate::components::capture_components::CapturePlan;

#[derive(Resource, Debug, Default)]
pub struct CapturePlanLibrary {
    pub plans: HashMap<String, CapturePlan>,
}

#[derive(Resource, Debug, Default)]
pub struct CaptureSphereRadius {
    pub radius: f64,
}
