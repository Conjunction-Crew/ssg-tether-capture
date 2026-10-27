use bevy::prelude::*;

#[derive(Resource, Debug, Default)]
pub struct Settings {
    pub dev_gizmos: bool,
    pub capture_gizmos: bool,
    // unused today; the P3 component/resource audit decides
    #[allow(dead_code)]
    pub start_sim: bool,
}

impl Settings {
    /// Shows or hides the capture gizmos (the C key and the UI button, F-29).
    pub fn toggle_capture_gizmos(&mut self) {
        self.capture_gizmos = !self.capture_gizmos;
    }
}
