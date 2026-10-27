//! Rendering: meshes, materials, cameras, lights, gizmos and the GPU catalog.

pub mod attach;
pub mod camera;
pub mod celestial;
pub mod floating_origin_visuals;
pub mod gizmos;
pub mod gpu_compute;
pub mod layers;

use bevy::app::PluginGroupBuilder;
use bevy::prelude::*;

/// The render plugins. Add them after [`crate::sim::SimPlugins`].
pub struct RenderPlugins;

impl PluginGroup for RenderPlugins {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add(celestial::CelestialPlugin)
            .add(camera::OrbitCameraPlugin)
            .add(attach::AttachPlugin)
            .add(gizmos::GizmoPlugin)
            .add(gpu_compute::GpuComputePlugin)
    }
}
