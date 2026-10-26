use bevy::prelude::*;

#[derive(Resource, Debug, Default)]
pub struct Settings {
    pub dev_gizmos: bool,
    pub capture_gizmos: bool,
    // unused today; the P3 component/resource audit decides
    #[allow(dead_code)]
    pub start_sim: bool,
}
