use bevy::prelude::*;

#[derive(Component, Debug, Clone)]
pub struct TimeWarpReadout;

#[derive(Component, Debug, Clone, Default)]
pub struct CaptureTelemetryReadout {
    pub target_entity: Option<Entity>,
    pub reference_entity: Option<Entity>,
    pub target_label: String,
}

#[derive(Component, Debug, Clone, Default)]
pub struct CaptureGuidanceReadout {
    pub target_entity: Option<Entity>,
    pub reference_entity: Option<Entity>,
    pub target_label: String,
    pub plan_id: String,
}

#[derive(Component, Debug, Clone, Default)]
pub struct OrbitLabel {
    pub entity: Option<Entity>,
}
