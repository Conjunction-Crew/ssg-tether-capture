//! The tether and the debris: the two bodies a capture run spawns.

use avian3d::prelude::*;
use bevy::{math::DVec3, prelude::*};

use crate::SimSet;
use crate::anchor::SimAnchor;
use crate::capture::plan::{
    CapturePlanLibrary, DEFAULT_SEGMENT_SPACING_M, DEFAULT_TETHER_LENGTH_M,
};
use crate::orbit::Orbit;
use crate::orbit::cache::OrbitalCache;
use crate::orbit::selection::{OrbitalSelectionState, SelectedOrbitalObject};
use crate::physics_bubble::PHYSICS_DISABLE_RADIUS;
use crate::project::SelectedProject;
use crate::session::{SimState, UiScreen};

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

/// How a selected body starts: SGP4 for an unedited catalog pick, otherwise its
/// elements at their epoch (F-10, F-27).
fn initial_orbit(selected: &SelectedOrbitalObject) -> Orbit {
    match &selected.gp {
        Some(gp) => Orbit::FromGp(gp.clone()),
        None => Orbit::FromElementsAt {
            elements: selected.elements.to_vec6(),
            epoch: selected.elements.epoch,
        },
    }
}

/// The target's mass when `project.json` gives none, in kg.
const DEFAULT_TARGET_MASS_KG: f64 = 2500.0;

/// The `project.json` inertia tensor (kg m², validated symmetric and positive
/// definite) as avian's [`AngularInertia`]. Every body keeps the identity
/// `Rotation` at spawn, so the tensor is in the sim's ECI-aligned axes (C3).
fn angular_inertia(tensor: &[[f64; 3]; 3]) -> AngularInertia {
    let rows = tensor.map(|row| row.map(|v| v as f32));
    AngularInertia::from_tensor(AngularInertiaTensor::from_mat3_unchecked(
        Mat3::from_cols_array_2d(&rows).transpose(),
    ))
}

pub fn spawn_debris(
    commands: &mut Commands,
    orbital_cache: &mut ResMut<OrbitalCache>,
    selected: &SelectedOrbitalObject,
) -> Result<(), BevyError> {
    let mut debris = commands.spawn((
        DespawnOnExit(UiScreen::Sim),
        DebrisBody {
            model_path: "models/broken_satellite.glb".to_string(),
        },
        RigidBody::Dynamic,
        RigidBodyDisabled,
        initial_orbit(selected),
        ColliderConstructorHierarchy::new(ColliderConstructor::ConvexHullFromMesh),
        CenterOfMass(Vec3::ZERO),
        Mass::from(selected.mass_kg.unwrap_or(DEFAULT_TARGET_MASS_KG) as f32),
        Transform::from_xyz(
            PHYSICS_DISABLE_RADIUS as f32 + 10.0,
            PHYSICS_DISABLE_RADIUS as f32 + 10.0,
            PHYSICS_DISABLE_RADIUS as f32 + 10.0,
        ),
    ));
    // Q10 row 11: the manifest's tensor replaces the collider's scaled inertia.
    if let Some(tensor) = &selected.inertia_kg_m2 {
        debris.insert(angular_inertia(tensor));
    }
    orbital_cache
        .debris
        .insert("Satellite1".to_string(), debris.id());

    Ok(())
}

pub fn spawn_tether(
    commands: &mut Commands,
    orbital_entities: &mut ResMut<OrbitalCache>,
    selected_project: &Res<SelectedProject>,
    capture_plan_lib: &Res<CapturePlanLibrary>,
    selected: &SelectedOrbitalObject,
) -> Result<(), BevyError> {
    let root_tail_radius: f64 = 0.50;
    let rope_radius: f64 = 0.25;

    // Resolve tether parameters from the active capture plan's device block.
    // Anything the plan leaves out takes the plan's own default (D3), which are
    // the values this spawner used as literals before P6.

    let active_plan = selected_project
        .project_id
        .as_deref()
        .and_then(|id| capture_plan_lib.plans.get(id));

    // F-43 (P6): the plan's tether id is validated non-blank; "Tether1" only without an active plan
    let tether_name = active_plan
        .map(|plan| plan.tether.clone())
        .unwrap_or_else(|| "Tether1".to_string());

    let device = active_plan
        .and_then(|plan| plan.device.clone())
        .unwrap_or_default();

    // A plan the validator rejects (`insert_plan` does not validate) falls back
    // to the default length and spacing instead of building nothing or dividing by zero.
    let tether_length = Some(device.tether_length_m())
        .filter(|l| *l > 0.0)
        .unwrap_or(DEFAULT_TETHER_LENGTH_M);
    let segment_spacing = Some(device.segment_spacing_m())
        .filter(|s| *s > 0.0)
        .unwrap_or(DEFAULT_SEGMENT_SPACING_M);
    let segment_mass = device.segment_mass_kg() as f32;
    let end_mass = device.end_mass_kg() as f32;
    let joint_damping = JointDamping {
        linear: device.joint_damping_linear(),
        angular: device.joint_damping_angular(),
    };

    let interior_node_count: u32 =
        ((tether_length - 2.0 * root_tail_radius) / segment_spacing).max(0.0) as u32;

    // Segment length is derived: distribute the interior length evenly across joints.
    let tether_node_length = if interior_node_count > 0 {
        (tether_length - 2.0 * root_tail_radius) / interior_node_count as f64
    } else {
        segment_spacing
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

    // The root tether node. The root is the chaser (design D8): it takes the
    // chaser's `mass_kg` and `inertia_kg_m2` from `project.json` when given (Q10 row 11).
    let root_mass = selected.mass_kg.map_or(end_mass, |m| m as f32);
    let mut root = commands.spawn((
        DespawnOnExit(UiScreen::Sim),
        SimAnchor,
        TetherRoot,
        sphere_shape,
        RigidBody::Dynamic,
        sphere_collider.clone(),
        Mass::from(root_mass),
        Transform::from_xyz(0.0, 0.0, 0.0),
        initial_orbit(selected),
    ));
    if let Some(tensor) = &selected.inertia_kg_m2 {
        root.insert(angular_inertia(tensor));
    }
    let tether_root = root.id();

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
            (
                sphere_shape,
                sphere_collider.clone(),
                end_mass,
                root_tail_radius,
            )
        } else {
            (
                tether_node_shape,
                tether_node_collider.clone(),
                segment_mass,
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
            joint_damping,
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
        chaser,
    ) {
        error!("Error spawning tether: {}", e);
    };

    if let Err(e) = spawn_debris(&mut commands, &mut orbital_cache, target) {
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
