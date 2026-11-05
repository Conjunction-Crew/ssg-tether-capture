use bevy::prelude::*;

use crate::ui::form::{NewCapturePlanForm, UnitSystem};

#[derive(Resource, Debug, Default)]
pub struct Settings {
    pub dev_gizmos: bool,
    pub capture_gizmos: bool,
    // unused today; the P3 component/resource audit decides
    #[allow(dead_code)]
    pub start_sim: bool,
    /// The units shown on the sim screen, the plots and the plan form, for this
    /// launch only (OQ3).
    pub unit_system: UnitSystem,
}

impl Settings {
    /// Shows or hides the capture gizmos (the C key and the UI button, F-29).
    pub fn toggle_capture_gizmos(&mut self) {
        self.capture_gizmos = !self.capture_gizmos;
    }
}

/// The one Metric/Imperial toggle, from the Simulation HUD or the plan form (OQ3).
pub fn set_unit_system(settings: &mut Settings, form: &mut NewCapturePlanForm, unit: UnitSystem) {
    settings.unit_system = unit;
    form.set_unit_system(unit);
}
