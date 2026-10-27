//! The tether and the debris: the two bodies a capture run spawns.

use avian3d::prelude::*;
use bevy::{math::DVec3, prelude::*};
use nalgebra::Vector6;

use crate::sim::SimSet;
use crate::sim::anchor::CameraTarget;
use crate::sim::capture::plan::CapturePlanLibrary;
use crate::sim::orbit::Orbit;
use crate::sim::orbit::cache::OrbitalCache;
use crate::sim::orbit::selection::OrbitalSelectionState;
use crate::sim::physics_bubble::PHYSICS_DISABLE_RADIUS;
use crate::sim::project::SelectedProject;
use crate::sim::session::{SimState, UiScreen};

// A root tether node
#[derive(Component, Debug, Clone)]
pub struct TetherRoot;

// Component for identifying a root node of a tether system.
#[derive(Component, Debug, Clone)]
pub struct TetherNode {
    pub root: Entity,
}

/// The visual shape of a tether body; the renderer attaches the mesh (D2).
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub enum TetherShape {
    Sphere { radius: f64 },
    Cylinder { radius: f64, length: f64 },
}

/// A debris body; the renderer attaches the scene at `model_path` (D2).
#[derive(Component, Debug, Clone)]
pub struct DebrisBody {
    pub model_path: String,
}

pub fn spawn_debris(
    commands: &mut Commands,
    orbital_cache: &mut ResMut<OrbitalCache>,
    elements: Vector6<f64>,
) -> Result<(), BevyError> {
    orbital_cache.debris.insert(
        "Satellite1".to_string(),
        commands
            .spawn((
                DespawnOnExit(UiScreen::Sim),
                DebrisBody {
                    model_path: "models/broken_satellite.glb".to_string(),
                },
                RigidBody::Dynamic,
                RigidBodyDisabled,
                Orbit::FromElements(elements),
                ColliderConstructorHierarchy::new(ColliderConstructor::ConvexHullFromMesh),
                CenterOfMass(Vec3::ZERO),
                Mass::from(2500.0),
                Transform::from_xyz(
                    PHYSICS_DISABLE_RADIUS as f32 + 10.0,
                    PHYSICS_DISABLE_RADIUS as f32 + 10.0,
                    PHYSICS_DISABLE_RADIUS as f32 + 10.0,
                ),
            ))
            .id(),
    );

    Ok(())
}

pub fn spawn_tether(
    commands: &mut Commands,
    orbital_entities: &mut ResMut<OrbitalCache>,
    selected_project: &Res<SelectedProject>,
    capture_plan_lib: &Res<CapturePlanLibrary>,
    elements: Vector6<f64>,
) -> Result<(), BevyError> {
    let root_tail_radius: f64 = 0.50;
    let rope_radius: f64 = 0.25;

    // Resolve tether parameters from the active capture plan's device block.
    // Fall back to the legacy constant-derived values when unspecified.
    const DEFAULT_TETHER_LENGTH: f64 = 20.0;
    const DEFAULT_DIST_BETWEEN_JOINTS: f64 = 0.1;

    let active_plan = selected_project
        .project_id
        .as_deref()
        .and_then(|id| capture_plan_lib.plans.get(id));

    let tether_name = active_plan
        .map(|plan| plan.tether.clone())
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| "Tether1".to_string());

    let device = active_plan.and_then(|plan| plan.device.as_ref());

    let tether_length = device
        .filter(|d| d.tether_length > 0.0)
        .map(|d| d.tether_length)
        .unwrap_or(DEFAULT_TETHER_LENGTH);

    let interior_node_count: u32 = device
        .filter(|d| d.tether_length > 0.0)
        .map(|d| {
            ((d.tether_length - 2.0 * root_tail_radius) / DEFAULT_DIST_BETWEEN_JOINTS).max(0.0)
                as u32
        })
        .unwrap_or_else(|| {
            ((tether_length - 2.0 * root_tail_radius) / DEFAULT_DIST_BETWEEN_JOINTS).max(0.0) as u32
        });

    // Segment length is derived: distribute the interior length evenly across joints.
    let tether_node_length = if interior_node_count > 0 {
        (tether_length - 2.0 * root_tail_radius) / interior_node_count as f64
    } else {
        DEFAULT_DIST_BETWEEN_JOINTS
    };
    let tether_node_half_length = tether_node_length * 0.5;

    let sphere_shape = TetherShape::Sphere {
        radius: root_tail_radius,
    };
    let sphere_collider = Collider::sphere(root_tail_radius);

    let tether_node_shape = TetherShape::Cylinder {
        radius: rope_radius / 8.0,
        length: tether_node_length,
    };
    let tether_node_collider = Collider::cylinder(rope_radius / 8.0, tether_node_length);

    // The root tether node
    let tether_root = commands
        .spawn((
            DespawnOnExit(UiScreen::Sim),
            CameraTarget,
            TetherRoot,
            sphere_shape,
            RigidBody::Dynamic,
            sphere_collider.clone(),
            Mass::from(2.0),
            Transform::from_xyz(0.0, 0.0, 0.0),
            Orbit::FromElements(elements),
        ))
        .id();

    orbital_entities
        .tethers
        .insert(tether_name.clone(), vec![tether_root]);

    let mut prev_sphere = tether_root;
    let mut prev_half_extent = root_tail_radius;
    let mut prev_y = 0.0;
    let interval_count = interior_node_count + 1;
    let surface_gap = if interval_count > 0 {
        (tether_length - 2.0 * root_tail_radius - interior_node_count as f64 * tether_node_length)
            / interval_count as f64
    } else {
        0.0
    };
    let tail_index = interior_node_count + 1;

    for i in 1..=tail_index {
        let (shape, collider, mass, curr_half_extent) = if i == tail_index {
            (sphere_shape, sphere_collider.clone(), 2.0, root_tail_radius)
        } else {
            (
                tether_node_shape,
                tether_node_collider.clone(),
                0.1,
                tether_node_half_length,
            )
        };

        let link_spacing = prev_half_extent + curr_half_extent + surface_gap;
        let y = prev_y + link_spacing;

        let sphere = commands
            .spawn((
                DespawnOnExit(UiScreen::Sim),
                TetherNode { root: tether_root },
                shape,
                RigidBody::Dynamic,
                collider,
                Mass::from(mass),
                Transform::from_xyz(0.0, y as f32, 0.0),
            ))
            .id();

        let anchor = DVec3::new(0.0, prev_y + prev_half_extent + surface_gap * 0.5, 0.0);

        commands.spawn((
            DespawnOnExit(UiScreen::Sim),
            // SphericalJoint::new(prev_sphere, sphere).with_anchor(anchor),
            DistanceJoint::new(prev_sphere, sphere).with_anchor(anchor),
            JointDamping {
                linear: 1.0,  // Linear damping
                angular: 1.0, // Angular damping
            },
            JointCollisionDisabled,
        ));

        prev_sphere = sphere;
        prev_half_extent = curr_half_extent;
        prev_y = y;
    }

    // Add tail node to tether entity
    let Some(nodes) = orbital_entities.tethers.get_mut(&tether_name) else {
        return Err("tether not registered".into());
    };
    nodes.push(prev_sphere);

    Ok(())
}

pub fn spawn_selected_bodies(
    mut commands: Commands,
    selected_project: Res<SelectedProject>,
    capture_plan_lib: Res<CapturePlanLibrary>,
    selection: Res<OrbitalSelectionState>,
    mut orbital_cache: ResMut<OrbitalCache>,
) {
    let Some(chaser) = selection.chaser.as_ref() else {
        return;
    };

    let Some(target) = selection.target.as_ref() else {
        return;
    };

    if let Err(e) = spawn_tether(
        &mut commands,
        &mut orbital_cache,
        &selected_project,
        &capture_plan_lib,
        chaser.elements.to_vec6(),
    ) {
        error!("Error spawning tether: {}", e);
    };

    if let Err(e) = spawn_debris(&mut commands, &mut orbital_cache, target.elements.to_vec6()) {
        error!("Error spawning debris: {}", e);
    };
}

pub struct TetherPlugin;

impl Plugin for TetherPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(OnEnter(SimState::Running), SimSet::SpawnBodies)
            .add_systems(
                OnEnter(SimState::Running),
                spawn_selected_bodies.in_set(SimSet::SpawnBodies),
            );
    }
}
