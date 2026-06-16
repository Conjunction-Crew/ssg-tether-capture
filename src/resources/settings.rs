use bevy::prelude::*;

#[derive(Resource, Debug)]
pub struct Settings {
    pub dev_gizmos: bool,
    pub capture_gizmos: bool,
    pub start_sim: bool,
    /// Disables atmosphere and catalog dot rendering for propagation sims.
    pub performance_mode: bool,
    pub prop_viz: PropVizSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            dev_gizmos: false,
            capture_gizmos: false,
            start_sim: false,
            performance_mode: false,
            prop_viz: PropVizSettings::default(),
        }
    }
}

/// Toggles for the propagation sim's Clohessy-Wiltshire/Hill frame
/// visualization (egui plots + 3D gizmos), all off by default.
#[derive(Debug, Default)]
pub struct PropVizSettings {
    pub show_cw_ellipse: bool,
    pub show_cw_time_series: bool,
    pub show_hill_gizmos: bool,
    pub show_node_trails: bool,
}
