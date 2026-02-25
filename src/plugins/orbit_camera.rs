use avian3d::prelude::PhysicsSystems;
use bevy::prelude::*;

use crate::systems::orbit_camera::*;

pub struct OrbitCameraPlugin;

impl Plugin for OrbitCameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                orbit_camera_input,
                orbit_camera_switch_target,
                orbit_camera_control_target,
            ),
        )
        .add_systems(FixedPostUpdate, orbit_camera_track);
    }
}
