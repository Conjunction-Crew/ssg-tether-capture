//! The tether and the debris: the two bodies a capture run spawns.

use avian3d::prelude::*;
use bevy::{platform::collections::HashMap, prelude::*};

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

/// The collision layer of every tether body (root, nodes, tail); tether bodies
/// filter [`DEBRIS_LAYER`] only, so the rope never touches itself (design D10).
pub const TETHER_LAYER: LayerMask = LayerMask(0b01);
/// The collision layer of the debris.
pub const DEBRIS_LAYER: LayerMask = LayerMask(0b10);

/// The tether's bodies in chain order: from `root` along the distance joints
/// (`body1` → `body2`, spawn order) to the tail. A cycle ends the walk.
pub fn chain_from_joints<'a>(
    root: Entity,
    joints: impl Iterator<Item = &'a DistanceJoint>,
) -> Vec<Entity> {
    let next: HashMap<Entity, Entity> = joints.map(|j| (j.body1, j.body2)).collect();
    let mut chain = vec![root];
    while let Some(&node) = next.get(chain.last().expect("the root is in the chain")) {
        if chain.contains(&node) {
            break;
        }
        chain.push(node);
    }
    chain
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
    // Q10 row 11: the manifest's tensor replaces the collider's scaled inertia
    // (avian recomputes the mass properties in the first step's Prepare set).
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
    // C1: no angular damping on the chain. avian 0.6.1's joint damping
    // (`dynamics/solver/plugin.rs:785-792`) writes the angular term into both
    // joined bodies' angular velocity without a `LockedAxes` check, so a plan's
    // angular damping would leak spin into the rotation-locked nodes after any
    // contact. The linear term is the rope's internal viscosity when slack.
    let joint_damping = JointDamping {
        linear: device.joint_damping_linear(),
        angular: 0.0,
    };

    // F-66 (design D1, D2): a rope of point masses. The rope between the two end
    // spheres' surfaces is cut into `interval_count` equal intervals, as close to
    // the spacing as its length allows; the interior nodes sit between them. With
    // one interval the root and the tail are joined directly at the tether length
    // (they overlap when it is shorter than the two end spheres).
    let rope_length = tether_length - 2.0 * root_tail_radius;
    let interval_count = (rope_length / segment_spacing).round().max(1.0) as u32;
    let interval = rope_length / interval_count as f64;

    let sphere_shape = TetherShape::Sphere {
        radius: root_tail_radius,
    };
    let sphere_collider = Collider::sphere(root_tail_radius);

    let tether_node_shape = TetherShape::Sphere {
        radius: rope_radius / 8.0,
    };
    let tether_node_collider = Collider::sphere(rope_radius / 8.0);
    // D10: tether bodies touch the debris only (no tether–tether contact).
    let tether_layers = CollisionLayers::new(TETHER_LAYER, DEBRIS_LAYER);

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
        tether_layers,
        Mass::from(root_mass),
        Transform::from_xyz(0.0, 0.0, 0.0),
        initial_orbit(selected),
    ));
    // Avian recomputes the mass properties in the first step's Prepare set.
    if let Some(tensor) = &selected.inertia_kg_m2 {
        root.insert(angular_inertia(tensor));
    }
    let tether_root = root.id();

    orbital_entities
        .tethers
        .insert(tether_name.clone(), vec![tether_root]);

    let mut prev_sphere = tether_root;
    let mut prev_y = 0.0;

    for i in 1..=interval_count {
        let is_tail = i == interval_count;
        let y = if is_tail {
            tether_length
        } else {
            root_tail_radius + interval * i as f64
        };

        let mut body = commands.spawn((
            DespawnOnExit(UiScreen::Sim),
            TetherNode { root: tether_root },
            RigidBody::Dynamic,
            tether_layers,
            Transform::from_xyz(0.0, y as f32, 0.0),
        ));
        if is_tail {
            body.insert((sphere_shape, sphere_collider.clone(), Mass::from(end_mass)));
        } else {
            // A point mass: the node never rotates (D1).
            body.insert((
                tether_node_shape,
                tether_node_collider.clone(),
                Mass::from(segment_mass),
                LockedAxes::ROTATION_LOCKED,
            ));
        }
        let sphere = body.id();

        // Centre to centre, slack below the spawn distance and a hard stop at it.
        commands.spawn((
            DespawnOnExit(UiScreen::Sim),
            DistanceJoint::new(prev_sphere, sphere).with_limits(0.0, y - prev_y),
            joint_damping,
            JointCollisionDisabled,
        ));

        prev_sphere = sphere;
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
