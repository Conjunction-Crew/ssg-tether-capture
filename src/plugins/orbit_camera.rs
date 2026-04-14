use bevy::prelude::*;
use bevy::transform::TransformSystems;

use crate::systems::orbit_camera::*;
use crate::ui::state::UiScreen;

pub struct OrbitCameraPlugin;

impl Plugin for OrbitCameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (orbit_camera_input, orbit_camera_switch_target).run_if(in_state(UiScreen::Sim)),
        )
        .add_systems(
            PostUpdate,
            orbit_camera_track
                .before(TransformSystems::Propagate)
                .run_if(in_state(UiScreen::Sim)),
        );
    }
}
