//! Meshes, materials and scenes for the sim bodies (D2).
//!
//! The sim spawns bodies with [`TetherShape`] / [`DebrisBody`] markers; these
//! systems attach the visuals in the same `OnEnter(SimState::Running)` schedule,
//! after the spawn set. Not in `Update`: avian's
//! `init_collider_constructor_hierarchies` runs there and strips
//! `ColliderConstructorHierarchy` from a body that has no `SceneRoot` yet.

use bevy::{camera::visibility::RenderLayers, platform::collections::HashMap, prelude::*};

use crate::render::layers::SCENE_LAYER;
use ssg_sim::SimSet;
use ssg_sim::session::SimState;
use ssg_sim::tether::{DebrisBody, TetherShape};

/// The shared tether material and one mesh per distinct shape.
#[derive(Default)]
pub struct TetherVisualCache {
    material: Option<Handle<StandardMaterial>>,
    meshes: HashMap<(u8, u64, u64), Handle<Mesh>>,
}

fn shape_key(shape: &TetherShape) -> (u8, u64, u64) {
    match *shape {
        TetherShape::Sphere { radius } => (0, radius.to_bits(), 0),
        TetherShape::Cylinder { radius, length } => (1, radius.to_bits(), length.to_bits()),
    }
}

pub fn attach_tether_visuals(
    mut commands: Commands,
    shapes: Query<(Entity, &TetherShape), Added<TetherShape>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut cache: Local<TetherVisualCache>,
) {
    for (entity, shape) in &shapes {
        let material = cache
            .material
            .get_or_insert_with(|| {
                materials.add(StandardMaterial {
                    base_color: Color::Srgba(Srgba {
                        red: 1.0,
                        green: 0.0,
                        blue: 0.0,
                        alpha: 1.0,
                    }),
                    perceptual_roughness: 1.0,
                    ..default()
                })
            })
            .clone();
        let mesh = cache
            .meshes
            .entry(shape_key(shape))
            .or_insert_with(|| {
                meshes.add(match *shape {
                    TetherShape::Sphere { radius } => Mesh::from(Sphere::new(radius as f32)),
                    TetherShape::Cylinder { radius, length } => {
                        Mesh::from(Cylinder::new(radius as f32, length as f32))
                    }
                })
            })
            .clone();
        commands.entity(entity).insert((
            Mesh3d(mesh),
            MeshMaterial3d(material),
            RenderLayers::layer(SCENE_LAYER),
        ));
    }
}

pub fn attach_debris_scene(
    mut commands: Commands,
    debris: Query<(Entity, &DebrisBody), Added<DebrisBody>>,
    asset_server: Res<AssetServer>,
) {
    for (entity, body) in &debris {
        let scene: Handle<Scene> =
            asset_server.load(GltfAssetLabel::Scene(0).from_asset(body.model_path.clone()));
        commands.entity(entity).insert(SceneRoot(scene));
    }
}

pub struct AttachPlugin;

impl Plugin for AttachPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            OnEnter(SimState::Running),
            (attach_tether_visuals, attach_debris_scene).after(SimSet::SpawnBodies),
        );
    }
}
