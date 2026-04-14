use bevy::prelude::*;

use crate::systems::orbit_camera::*;
use crate::systems::propagation::floating_origin_update_visuals;
use crate::ui::state::UiScreen;

pub struct OrbitCameraPlugin;

impl Plugin for OrbitCameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                orbit_camera_input,
                orbit_camera_switch_target,
                orbit_camera_ui_controls,
            )
                .run_if(in_state(UiScreen::Sim)),
        )
        .add_systems(
            PostUpdate,
            orbit_camera_track
                .after(floating_origin_update_visuals)
                .run_if(in_state(UiScreen::Sim)),
        );
    }
}
