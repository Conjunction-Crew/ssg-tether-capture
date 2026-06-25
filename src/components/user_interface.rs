use bevy::prelude::*;

#[derive(Component, Debug, Clone)]
pub struct TimeWarpReadout;

#[derive(Component, Debug, Clone)]
pub struct CaptureTelemetryReadout {
    pub rso_entity: Option<Entity>,
    pub reference_entity: Option<Entity>,
    pub rso_label: String,
}

impl Default for CaptureTelemetryReadout {
    fn default() -> Self {
        Self {
            rso_entity: None,
            reference_entity: None,
            rso_label: String::new(),
        }
    }
}

#[derive(Component, Debug, Clone)]
pub struct CaptureGuidanceReadout {
    pub rso_entity: Option<Entity>,
    pub reference_entity: Option<Entity>,
    pub rso_label: String,
    pub plan_id: String,
}

impl Default for CaptureGuidanceReadout {
    fn default() -> Self {
        Self {
            rso_entity: None,
            reference_entity: None,
            rso_label: String::new(),
            plan_id: String::new(),
        }
    }
}

#[derive(Component, Debug, Clone)]
pub struct OrbitLabel {
    pub entity: Option<Entity>,
}

impl Default for OrbitLabel {
    fn default() -> Self {
        Self { entity: None }
    }
}
