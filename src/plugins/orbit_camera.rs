use bevy::prelude::*;

use crate::systems::orbit_camera::*;
use crate::systems::propagation::floating_origin;

pub struct OrbitCameraPlugin;

impl Plugin for OrbitCameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (orbit_camera_input, orbit_camera_switch_target))
            .add_systems(PostUpdate, orbit_camera_track.after(floating_origin));
    }
}
