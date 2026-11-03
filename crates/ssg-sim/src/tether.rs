//! The tether and the debris: the two bodies a capture run spawns.

use avian3d::prelude::*;
use bevy::{
    math::{DQuat, DVec3},
    platform::collections::HashMap,
    prelude::*,
};
use brahe::Epoch;

use crate::SimSet;
use crate::anchor::SimAnchor;
use crate::capture::plan::{
    CapturePlanLibrary, DEFAULT_SEGMENT_SPACING_M, DEFAULT_TETHER_LENGTH_M, SpawnOrientation,
};
use crate::orbit::cache::OrbitalCache;
use crate::orbit::lvlh::LvlhFrame;
use crate::orbit::selection::{OrbitalSelectionState, SelectedOrbitalObject};
use crate::orbit::{Orbit, start_state_eci};
use crate::physics_bubble::PHYSICS_DISABLE_RADIUS;
use crate::project::SelectedProject;
use crate::session::{SimState, UiScreen};
use crate::telemetry::{LogEvent, LogLevel};
use crate::time::SimConfig;

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

/// The collision layer of every tether body (root, nodes, tail).
pub const TETHER_LAYER: LayerMask = LayerMask(0b01);
/// The collision layer of the debris (its body and its array children).
pub const DEBRIS_LAYER: LayerMask = LayerMask(0b10);
/// What every tether body filters: the rope touches itself and the debris (design
/// D13, O2). Jointed neighbours never pair (`JointCollisionDisabled`); the rope
/// still passes between its own nodes (0.0625 m spheres 0.25 m apart).
pub const TETHER_FILTERS: LayerMask = LayerMask(TETHER_LAYER.0 | DEBRIS_LAYER.0);

/// One box of the debris collider in the debris body's axes: its half-extents, its
/// centre and its rotation about the body's X.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DebrisBox {
    pub half_extents: DVec3,
    pub centre: DVec3,
    pub rotation_about_x_rad: f64,
}

impl DebrisBox {
    fn collider(&self) -> Collider {
        let size = 2.0 * self.half_extents;
        Collider::cuboid(size.x, size.y, size.z)
    }

    fn rotation(&self) -> DQuat {
        DQuat::from_rotation_x(self.rotation_about_x_rad)
    }
}

/// The debris collider's boxes, stated primitives fitted to `broken_satellite.glb`
/// (F-64, design D13), in the debris body's axes with the attach's 90° about Y
/// applied (glTF (x, y, z) → body (z, y, −x)). The model's nodes, in metres:
///
/// | node | body x | body y | body z | box |
/// |---|---|---|---|---|
/// | `Cube` (the bus) | −2.00…1.00 | −1.00…1.00 | −1.01…1.00 | bus |
/// | `NurbsCurve` (the aft ring) | −2.66…−2.00 | −1.50…1.50 | −1.50…1.50 | bus |
/// | `Cylinder` | −2.43…−1.99 | −0.01…0.01 | −0.01…0.01 | bus |
/// | `NurbsCurve.001`, `Cylinder.003` (the nozzle) | 0.75…1.49 | −0.75…0.75 | −0.75…0.75 | bus |
/// | `Plane.001` (the flat array) | −1.00…1.00 | −0.01…0.00 | 1.50…9.50 | array A |
/// | `Cylinder.002` (its boom) | −0.10…0.10 | −0.10…0.10 | 1.00…9.00 | array A, bus |
/// | `Plane` (the broken array, tilted 14.9° toward −Y) | −1.61…1.25 | −1.99…0.08 | −9.32…−1.47 | array B |
/// | `Cylinder.001` (its boom) | −0.10…0.10 | −1.60…0.13 | −7.50…−1.00 | array B, bus |
///
/// The axis the controller circles about (`CAPTURE_CIRCLING_AXIS`, the body's Z)
/// is the one the arrays span; the boxes and the axis are fitted to this one model
/// and apply to every target in P7.5 (a per-target key is P9's, OQ3).
pub const DEBRIS_BUS: DebrisBox = DebrisBox {
    half_extents: DVec3::new(2.075, 1.5, 1.5),
    centre: DVec3::new(-0.585, 0.0, 0.0),
    rotation_about_x_rad: 0.0,
};
/// The flat solar array (array A); see [`DEBRIS_BUS`].
pub const DEBRIS_ARRAY_A: DebrisBox = DebrisBox {
    half_extents: DVec3::new(1.0, 0.05, 4.0),
    centre: DVec3::new(0.0, 0.0, 5.5),
    rotation_about_x_rad: 0.0,
};
/// The broken solar array (array B), rotated −0.26 rad about X; see [`DEBRIS_BUS`].
pub const DEBRIS_ARRAY_B: DebrisBox = DebrisBox {
    half_extents: DVec3::new(1.4, 0.3, 4.0),
    centre: DVec3::new(-0.18, -0.95, -5.4),
    rotation_about_x_rad: -0.26,
};

/// The debris body's own collider: the bus box as a one-shape compound (a body's
/// own collider sits at the body's `Position`, so only a compound shape carries the
/// box's offset; one shape has one contact manifold against a ball). The body keeps
/// the identity `Rotation` (C3).
fn debris_collider() -> Collider {
    Collider::compound(vec![(
        Position::from(DEBRIS_BUS.centre),
        Rotation::IDENTITY,
        DEBRIS_BUS.collider(),
    )])
}

/// The debris's angular inertia without a manifest tensor: the three-shape compound
/// (the bus and the two arrays at their offsets and rotations) built for its mass
/// properties only, scaled to `mass_kg` and re-expressed about the body origin with
/// the parallel-axis shift by the compound's natural centre `d`
/// (`+ m (|d|² E − d dᵀ)`, by hand: avian 0.6.1's `shifted_tensor` adds the outer
/// product instead of subtracting it), so the tensor and `CenterOfMass(ZERO)`
/// describe the same body (design D13, E21).
pub(crate) fn debris_angular_inertia(mass_kg: f64) -> AngularInertia {
    let compound = Collider::compound(
        [DEBRIS_BUS, DEBRIS_ARRAY_A, DEBRIS_ARRAY_B]
            .iter()
            .map(|b| {
                (
                    Position::from(b.centre),
                    Rotation::from(b.rotation()),
                    b.collider(),
                )
            })
            .collect(),
    );
    let mut props = compound.mass_properties(1.0);
    props.set_mass(mass_kg as f32, true);
    let frame = Mat3::from_quat(props.local_inertial_frame);
    let about_centre =
        frame * Mat3::from_diagonal(props.principal_angular_inertia) * frame.transpose();
    let d = props.center_of_mass;
    let shift = (Mat3::IDENTITY * d.length_squared() - Mat3::from_cols(d * d.x, d * d.y, d * d.z))
        * props.mass;
    AngularInertia::from_tensor(AngularInertiaTensor::from_mat3_unchecked(
        about_centre + shift,
    ))
}

/// The two solar arrays as child collider entities of the debris (design D13):
/// avian 0.6.1 (issue #1067, fixed in 0.8.0) loses a contact pair whose manifold
/// count changes while touching when one body was ever `RigidBodyDisabled`, which
/// only a multi-shape collider can trigger (design D12 F6). avian's
/// `ColliderHierarchyPlugin` attaches them to the body (`ColliderOf`); they despawn
/// with it.
fn spawn_debris_arrays(commands: &mut Commands, debris: Entity) {
    for array in [DEBRIS_ARRAY_A, DEBRIS_ARRAY_B] {
        commands.spawn((
            ChildOf(debris),
            array.collider(),
            CollisionLayers::new(DEBRIS_LAYER, TETHER_LAYER),
            Transform::from_translation(array.centre.as_vec3())
                .with_rotation(array.rotation().as_quat()),
        ));
    }
}

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
    let mass_kg = selected.mass_kg.unwrap_or(DEFAULT_TARGET_MASS_KG);
    // C1, E21: avian's `MassPropertyHelper::update_mass_properties` sums every
    // descendant collider's density-1 mass properties into `ComputedMass`,
    // `ComputedAngularInertia` and `ComputedCenterOfMass` unless told not to (the
    // array children would add 15.04 kg, a 2.5 cm centre offset and an unscaled
    // array inertia; a manifest tensor would have the children's inertia added to
    // it). The markers alone are not enough: `set_mass(2500, true)` would then
    // rescale the body-plus-children sum and leave the arrays at their density-1
    // mass. So the debris states all three: the mass, the centre at the origin, and
    // the manifest's tensor (Q10 row 11) or the compound's.
    let inertia = selected
        .inertia_kg_m2
        .as_ref()
        .map_or_else(|| debris_angular_inertia(mass_kg), angular_inertia);
    let debris = commands
        .spawn((
            DespawnOnExit(UiScreen::Sim),
            DebrisBody {
                model_path: "models/broken_satellite.glb".to_string(),
            },
            RigidBody::Dynamic,
            RigidBodyDisabled,
            initial_orbit(selected),
            debris_collider(),
            CollisionLayers::new(DEBRIS_LAYER, TETHER_LAYER),
            CenterOfMass(Vec3::ZERO),
            Mass::from(mass_kg as f32),
            (
                NoAutoMass,
                NoAutoCenterOfMass,
                NoAutoAngularInertia,
                inertia,
            ),
            Transform::from_xyz(
                PHYSICS_DISABLE_RADIUS as f32 + 10.0,
                PHYSICS_DISABLE_RADIUS as f32 + 10.0,
                PHYSICS_DISABLE_RADIUS as f32 + 10.0,
            ),
        ))
        .id();
    orbital_cache
        .debris
        .insert("Satellite1".to_string(), debris);
    spawn_debris_arrays(commands, debris);

    Ok(())
}

/// The unit axis the tether is laid along from the root (design D9), from both
/// bodies' states at `start_epoch`: away from the target (`toward_target`), or
/// along the chaser's LVLH −V-bar (`v_bar`, trailing) or −R-bar (`r_bar`, below).
/// `None` when a state is unavailable (SGP4 failure) or the bodies coincide.
pub(crate) fn spawn_axis(
    orientation: SpawnOrientation,
    chaser: &SelectedOrbitalObject,
    target: &SelectedOrbitalObject,
    start_epoch: Epoch,
) -> Option<DVec3> {
    let chaser_state = start_state_eci(&initial_orbit(chaser), start_epoch)?;
    match orientation {
        SpawnOrientation::TowardTarget => {
            let target_state = start_state_eci(&initial_orbit(target), start_epoch)?;
            let to_target = DVec3::new(
                target_state[0] - chaser_state[0],
                target_state[1] - chaser_state[1],
                target_state[2] - chaser_state[2],
            );
            (-to_target).try_normalize()
        }
        SpawnOrientation::VBar => {
            LvlhFrame::from_eci(&chaser_state).map(|frame| frame.to_eci(DVec3::new(0.0, -1.0, 0.0)))
        }
        SpawnOrientation::RBar => {
            LvlhFrame::from_eci(&chaser_state).map(|frame| frame.to_eci(DVec3::new(-1.0, 0.0, 0.0)))
        }
    }
}

/// Spawns the tether from the root at the origin along the unit `axis`. The
/// orientation lives in the positions only: every body keeps the identity
/// `Rotation` (C3), so a stated chaser inertia tensor stays in the sim's axes.
pub fn spawn_tether(
    commands: &mut Commands,
    orbital_entities: &mut ResMut<OrbitalCache>,
    selected_project: &Res<SelectedProject>,
    capture_plan_lib: &Res<CapturePlanLibrary>,
    selected: &SelectedOrbitalObject,
    axis: DVec3,
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
    // D13: tether bodies touch each other and the debris.
    let tether_layers = CollisionLayers::new(TETHER_LAYER, TETHER_FILTERS);

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
    // D13 (C9, E18): the chaser holds its attitude (C3) and a sphere's spin moves
    // nothing. The chaser's `inertia_kg_m2` is still loaded onto the root and is
    // inert while the rotation is locked, until P12's attitude model.
    root.insert(LockedAxes::ROTATION_LOCKED);
    let tether_root = root.id();

    orbital_entities
        .tethers
        .insert(tether_name.clone(), vec![tether_root]);

    let mut prev_sphere = tether_root;
    let mut prev_position = Vec3::ZERO;

    for i in 1..=interval_count {
        let is_tail = i == interval_count;
        let y = if is_tail {
            tether_length
        } else {
            root_tail_radius + interval * i as f64
        };

        let position = (axis * y).as_vec3();
        let mut body = commands.spawn((
            DespawnOnExit(UiScreen::Sim),
            TetherNode { root: tether_root },
            RigidBody::Dynamic,
            tether_layers,
            Transform::from_translation(position),
        ));
        if is_tail {
            body.insert((sphere_shape, sphere_collider.clone(), Mass::from(end_mass)));
            // D13: the tail is locked as the root is (its spin moves nothing).
            body.insert(LockedAxes::ROTATION_LOCKED);
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
            DistanceJoint::new(prev_sphere, sphere)
                .with_limits(0.0, f64::from(position.distance(prev_position))),
            joint_damping,
            JointCollisionDisabled,
        ));

        prev_sphere = sphere;
        prev_position = position;
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
    config: Res<SimConfig>,
    mut log: MessageWriter<LogEvent>,
) {
    let Some(chaser) = selection.chaser.as_ref() else {
        return;
    };

    let Some(target) = selection.target.as_ref() else {
        return;
    };

    let orientation = selected_project
        .project_id
        .as_deref()
        .and_then(|id| capture_plan_lib.plans.get(id))
        .and_then(|plan| plan.device.as_ref())
        .map(|device| device.spawn_orientation())
        .unwrap_or_default();
    let axis = spawn_axis(orientation, chaser, target, config.start_epoch).unwrap_or_else(|| {
        log.write(LogEvent {
            level: LogLevel::Warn,
            source: "tether",
            message: format!(
                "No start state for the {orientation:?} spawn orientation; the tether is laid along +Y"
            ),
        });
        DVec3::Y
    });

    if let Err(e) = spawn_tether(
        &mut commands,
        &mut orbital_cache,
        &selected_project,
        &capture_plan_lib,
        chaser,
        axis,
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
