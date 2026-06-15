use avian3d::prelude::*;
use bevy::{camera::visibility::RenderLayers, math::DVec3, prelude::*};
use brahe::utils::DOrbitStateProvider;
use brahe::{AngleFormat, Epoch, KeplerianPropagator};
use nalgebra::Vector6;

use crate::{
    components::{
        capture_components::{PropagationNodeMode, PropagationOrientation, SimType},
        orbit::{Orbit, Orbital, SeparateBody, TetherNode, TetherRoot},
        orbit_camera::CameraTarget,
    },
    constants::{ISS_ORBIT, PHYSICS_DISABLE_RADIUS, SCENE_LAYER},
    resources::{capture_plans::CapturePlanLibrary, orbital_cache::OrbitalCache},
    systems::hill_frame::HillBasis,
    ui::state::{SelectedProject, UiScreen},
};

/// Per-tether spawn context for a propagation sim: which orientation axis to lay
/// the chain along (raw ECI) and the rotation that maps the existing +Y layout
/// onto that axis, plus the Hill basis used to seed independently-propagated nodes.
struct PropSpawnCtx {
    node_mode: PropagationNodeMode,
    axis_eci: DVec3,
    layout_rot: Quat,
    basis: HillBasis,
    reference_rv: Vector6<f64>,
}

/// Convert Keplerian elements `[a,e,i,Ω,ω,M]` (radians) to an ECI state at `epoch`.
fn keplerian_to_eci(elements: Vector6<f64>, epoch: Epoch) -> Vector6<f64> {
    let propagator = KeplerianPropagator::from_keplerian(epoch, elements, AngleFormat::Radians, 1.0);
    propagator
        .state_eci(epoch)
        .unwrap_or_else(|_| Vector6::zeros())
}

pub fn spawn_debris(
    commands: &mut Commands,
    orbital_cache: &mut ResMut<OrbitalCache>,
    asset_server: &Res<AssetServer>,
    elements: Vector6<f64>,
) -> Result<(), BevyError> {
    let scene: Handle<Scene> =
        asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/broken_satellite.glb"));

    orbital_cache.debris.insert(
        "Satellite1".to_string(),
        commands
            .spawn((
                DespawnOnExit(UiScreen::Sim),
                SceneRoot(scene),
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
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    orbital_entities: &mut ResMut<OrbitalCache>,
    selected_project: &Res<SelectedProject>,
    capture_plan_lib: &Res<CapturePlanLibrary>,
    elements: Vector6<f64>,
    epoch: Epoch,
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

    // For a propagation plan, resolve the initial orientation axis (raw ECI) and
    // the rotation that maps the default +Y node layout onto that axis. The
    // physics frame near the tether uses ECI axes (matching how disabled debris
    // is positioned), so the CW axis is used directly without an orbit-frame rotation.
    let prop_ctx: Option<PropSpawnCtx> = active_plan
        .filter(|plan| plan.sim_type == SimType::Propagation)
        .and_then(|plan| plan.propagation)
        .map(|config| {
            let reference_rv = keplerian_to_eci(elements, epoch);
            let basis = HillBasis::from_reference(reference_rv, elements[0]);
            let radial = matches!(config.orientation, PropagationOrientation::CwRadial);
            let axis_eci = basis.axis(radial);
            let axis_world =
                Vec3::new(axis_eci.x as f32, axis_eci.y as f32, axis_eci.z as f32).normalize_or(Vec3::Y);
            let layout_rot = Quat::from_rotation_arc(Vec3::Y, axis_world);
            PropSpawnCtx {
                node_mode: config.node_mode,
                axis_eci,
                layout_rot,
                basis,
                reference_rv,
            }
        });

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

    let sphere_mesh = meshes.add(Mesh::from(Sphere::new(root_tail_radius as f32)));
    let sphere_collider = Collider::sphere(root_tail_radius);
    let sphere_material = materials.add(StandardMaterial {
        base_color: Color::Srgba(Srgba {
            red: 1.0,
            green: 0.0,
            blue: 0.0,
            alpha: 1.0,
        }),
        perceptual_roughness: 1.0,
        ..default()
    });

    let tether_node_mesh = Mesh::from(Cylinder::new(
        (rope_radius / 8.0) as f32,
        tether_node_length as f32,
    ));
    let tether_node_collider = Collider::cylinder(rope_radius / 8.0, tether_node_length);
    let tether_node_mesh = meshes.add(tether_node_mesh);

    // The root tether node
    let mut root_cmd = commands.spawn((
        DespawnOnExit(UiScreen::Sim),
        CameraTarget,
        TetherRoot,
        RenderLayers::layer(SCENE_LAYER),
        RigidBody::Dynamic,
        sphere_collider.clone(),
        Mesh3d(sphere_mesh.clone()),
        MeshMaterial3d(sphere_material.clone()),
        Mass::from(2.0),
        Transform::from_xyz(0.0, 0.0, 0.0),
    ));
    match prop_ctx.as_ref() {
        // Separate-bodies: the root is the reference orbit, propagated directly
        // (at `epoch`) so it stays consistent with the independently-propagated nodes.
        Some(ctx) if ctx.node_mode == PropagationNodeMode::SeparateBodies => {
            root_cmd.insert(Orbital {
                object_id: String::new(),
                parent_entity: None,
                propagator: Some(KeplerianPropagator::from_eci(epoch, ctx.reference_rv, 1.0)),
            });
        }
        _ => {
            root_cmd.insert(Orbit::FromElements(elements));
        }
    }
    let tether_root = root_cmd.id();

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
        let (mesh, collider, mass, curr_half_extent) = if i == tail_index {
            (
                sphere_mesh.clone(),
                sphere_collider.clone(),
                2.0,
                root_tail_radius,
            )
        } else {
            (
                tether_node_mesh.clone(),
                tether_node_collider.clone(),
                0.1,
                tether_node_half_length,
            )
        };

        let link_spacing = prev_half_extent + curr_half_extent + surface_gap;
        let y = prev_y + link_spacing;

        // Node positions lie along +Y for capture; propagation rotates the layout
        // onto the chosen ECI orientation axis.
        let translation = match prop_ctx.as_ref() {
            Some(ctx) => ctx.layout_rot * Vec3::new(0.0, y as f32, 0.0),
            None => Vec3::new(0.0, y as f32, 0.0),
        };

        let mut node_cmd = commands.spawn((
            DespawnOnExit(UiScreen::Sim),
            RenderLayers::layer(SCENE_LAYER),
            TetherNode { root: tether_root },
            RigidBody::Dynamic,
            collider,
            Mesh3d(mesh),
            MeshMaterial3d(sphere_material.clone()),
            Mass::from(mass),
            Transform::from_translation(translation),
        ));

        // Separate-bodies mode: each node is its own two-body propagator, disabled
        // from local physics and synced from its orbit each step (no joints).
        let separate_bodies = matches!(
            prop_ctx.as_ref(),
            Some(ctx) if ctx.node_mode == PropagationNodeMode::SeparateBodies
        );
        if let Some(ctx) = prop_ctx.as_ref() {
            if separate_bodies {
                let node_eci = ctx.basis.node_initial_eci(ctx.axis_eci, y);
                node_cmd.insert((
                    SeparateBody,
                    RigidBodyDisabled,
                    Orbital {
                        object_id: String::new(),
                        parent_entity: None,
                        propagator: Some(KeplerianPropagator::from_eci(epoch, node_eci, 1.0)),
                    },
                ));
            }
        }
        let sphere = node_cmd.id();

        if !separate_bodies {
            let anchor_y = prev_y + prev_half_extent + surface_gap * 0.5;
            let anchor = match prop_ctx.as_ref() {
                Some(ctx) => {
                    let a = ctx.layout_rot * Vec3::new(0.0, anchor_y as f32, 0.0);
                    DVec3::new(a.x as f64, a.y as f64, a.z as f64)
                }
                None => DVec3::new(0.0, anchor_y, 0.0),
            };
            // Propagation tethers use light joint damping so gravity-gradient
            // libration stays visible; capture tethers keep heavier damping.
            let damping = if prop_ctx.is_some() { 0.05 } else { 1.0 };
            commands.spawn((
                DespawnOnExit(UiScreen::Sim),
                DistanceJoint::new(prev_sphere, sphere).with_anchor(anchor),
                JointDamping {
                    linear: damping,
                    angular: damping,
                },
                JointCollisionDisabled,
            ));
        }

        prev_sphere = sphere;
        prev_half_extent = curr_half_extent;
        prev_y = y;
    }

    // Add tail node to tether entity
    orbital_entities
        .tethers
        .get_mut(&tether_name)
        .expect("Error getting tether")
        .push(prev_sphere);

    Ok(())
}
