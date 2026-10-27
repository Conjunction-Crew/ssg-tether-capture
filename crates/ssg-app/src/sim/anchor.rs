//! The body the floating origin and the physics bubble are centred on.

use bevy::prelude::*;

use crate::sim::SimStep;
use crate::sim::time::ManualPhysics;

// Component to query the camera target
#[derive(Component, Debug, Clone)]
pub struct CameraTarget;

/// The body the floating origin and the physics bubble are centred on (Q15).
#[derive(Component, Debug, Clone, Copy)]
pub struct SimAnchor;

/// Mirrors [`CameraTarget`] into [`SimAnchor`].
///
/// P3 scaffolding (Q15/F-25): P4 removes this system and moves `CameraTarget` to
/// the app. It is the first step of every `ManualPhysics` run, so the steps after
/// it see the same anchor set the old code read from `CameraTarget`.
pub fn sync_anchor_from_camera_target(
    mut commands: Commands,
    added: Query<Entity, (With<CameraTarget>, Without<SimAnchor>)>,
    stale: Query<Entity, (With<SimAnchor>, Without<CameraTarget>)>,
) {
    for entity in &added {
        commands.entity(entity).insert(SimAnchor);
    }
    for entity in &stale {
        commands.entity(entity).remove::<SimAnchor>();
    }
}

pub struct AnchorPlugin;

impl Plugin for AnchorPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            ManualPhysics,
            sync_anchor_from_camera_target.in_set(SimStep::SyncAnchor),
        );
    }
}
