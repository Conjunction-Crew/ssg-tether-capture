use bevy::prelude::*;

#[derive(Component, Debug, Clone)]
pub struct TimeWarpReadout;

#[derive(Component, Debug, Clone)]
pub struct CaptureTelemetryReadout {
    pub target_entity: Option<Entity>,
    pub reference_entity: Option<Entity>,
    pub target_label: String,
}

impl Default for CaptureTelemetryReadout {
    fn default() -> Self {
        Self {
            target_entity: None,
            reference_entity: None,
            target_label: String::new(),
        }
    }
}

#[derive(Component, Debug, Clone)]
pub struct CaptureGuidanceReadout {
    pub target_entity: Option<Entity>,
    pub reference_entity: Option<Entity>,
    pub target_label: String,
    pub plan_id: String,
}

impl Default for CaptureGuidanceReadout {
    fn default() -> Self {
        Self {
            target_entity: None,
            reference_entity: None,
            target_label: String::new(),
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
