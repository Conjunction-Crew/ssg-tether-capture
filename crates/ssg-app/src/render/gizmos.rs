use avian3d::prelude::{DistanceJoint, RigidBodyDisabled, RigidBodyQuery};
use bevy::{camera::visibility::RenderLayers, ecs::system::SystemParam, math::DVec3, prelude::*};
use brahe::{AngleFormat, utils::DOrbitStateProvider};

use crate::render::floating_origin_visuals::floating_origin_update_visuals;
use crate::render::layers::{MAP_LAYER, MAP_UNITS_TO_M, SCENE_LAYER, eci_position_to_map};
use crate::ui::catalog_state::SpaceCatalogUiState;
use crate::ui::replay::ReplayBody;
use crate::ui::settings::Settings;
use ssg_sim::capture::plan::CapturePlanLibrary;
use ssg_sim::capture::{CaptureComponent, CaptureForce, CaptureSphereRadius};
use ssg_sim::floating_origin::MAX_ORIGIN_OFFSET;
use ssg_sim::orbit::Orbital;
use ssg_sim::orbit::cache::OrbitalCache;
use ssg_sim::orbit::catalog::{
    GpuElements, GpuOrbitalElements, SpaceObjectCatalog, propagate_catalog_eci_state,
};
use ssg_sim::orbit::selection::{
    EditableOrbitalElements, OrbitalSelectionSource, OrbitalSelectionState, SelectedOrbitalObject,
};
use ssg_sim::physics_bubble::PHYSICS_ENABLE_RADIUS;
use ssg_sim::session::{SimState, UiScreen};
use ssg_sim::tether::{TetherShape, chain_from_joints};
use ssg_sim::time::WorldTime;

#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct CaptureGizmoConfigGroup;

fn draw_orbital_from_elements(
    semi_major: f32,
    eccentricity: f32,
    inclination: f32,
    raan: f32,
    arg_of_perigee: f32,
    color: Srgba,
    gizmos: &mut Gizmos,
) {
    if semi_major <= 0.0 || !(0.0..1.0).contains(&eccentricity) {
        return;
    }

    let map_scale = MAP_UNITS_TO_M as f32;
    let scaled_semi_major = semi_major / map_scale;
    let semi_minor = semi_major * (1.0 - eccentricity * eccentricity).sqrt() / map_scale;

    // The map is drawn in ECI axes (F-08): perifocal → ECI only.
    let rotation = Quat::from_axis_angle(Vec3::Z, raan)
        * Quat::from_axis_angle(Vec3::X, inclination)
        * Quat::from_axis_angle(Vec3::Z, arg_of_perigee);

    let center_offset = rotation * Vec3::new(-scaled_semi_major * eccentricity, 0.0, 0.0);

    gizmos
        .ellipse(
            Isometry3d::new(center_offset, rotation),
            Vec2::new(scaled_semi_major, semi_minor),
            color,
        )
        .resolution(512);
}

fn draw_orbital_selection_from_elements(
    elements: &EditableOrbitalElements,
    color: Srgba,
    gizmos: &mut Gizmos,
) {
    draw_orbital_from_elements(
        elements.semi_major_axis_m as f32,
        elements.eccentricity as f32,
        elements.inclination_rad as f32,
        elements.raan_rad as f32,
        elements.arg_perigee_rad as f32,
        color,
        gizmos,
    );

    let orbital_point = GpuOrbitalElements {
        id: 0,
        a: elements.semi_major_axis_m as f32,
        e: elements.eccentricity as f32,
        i: elements.inclination_rad as f32,
        raan: elements.raan_rad as f32,
        argp: elements.arg_perigee_rad as f32,
        mean_anomaly: elements.mean_anomaly_rad as f32,
        epoch_offset_seconds: 0.0,
    };

    if let Some((position_eci, _velocity_eci)) = propagate_catalog_eci_state(&orbital_point, 0.0) {
        gizmos.sphere(
            Isometry3d::new(eci_position_to_map(position_eci), Quat::IDENTITY),
            1.5,
            color,
        );
    }
}

fn selected_orbital_gpu_index(selected: &SelectedOrbitalObject) -> Option<usize> {
    match &selected.source {
        OrbitalSelectionSource::Catalog { gpu_index, .. } => Some(*gpu_index),
        OrbitalSelectionSource::Custom { .. } => None,
    }
}

fn selection_contains_gpu_index(selection: &OrbitalSelectionState, gpu_index: usize) -> bool {
    [&selection.target, &selection.chaser]
        .into_iter()
        .flatten()
        .any(|selected| selected_orbital_gpu_index(selected) == Some(gpu_index))
}

/// The catalog and selection state [`orbital_gizmos`] draws from.
#[derive(SystemParam)]
pub struct CatalogGizmoInputs<'w> {
    catalog_ui_state: Res<'w, SpaceCatalogUiState>,
    catalog: Res<'w, SpaceObjectCatalog>,
    gpu_elements: Res<'w, GpuElements>,
    orbital_selection: Option<Res<'w, OrbitalSelectionState>>,
}

pub fn orbital_gizmos(
    orbitals: Query<&Orbital>,
    camera_s: Single<&RenderLayers, (With<Camera3d>, Without<Orbital>)>,
    world_time: Res<WorldTime>,
    mut gizmos: Gizmos,
    catalog_inputs: CatalogGizmoInputs,
    sim_state: Res<State<SimState>>,
) {
    let CatalogGizmoInputs {
        catalog_ui_state,
        catalog,
        gpu_elements,
        orbital_selection,
    } = catalog_inputs;
    let render_layers = camera_s.into_inner();

    // Do not render orbit gizmos in scene view
    if render_layers.intersects(&RenderLayers::layer(SCENE_LAYER)) {
        return;
    }

    // Draw orbitals with an active propagator
    for orbital in orbitals {
        let Some(propagator) = orbital.propagator.clone() else {
            continue;
        };

        if let Ok(elements) = propagator.state_koe_osc(world_time.epoch, AngleFormat::Radians) {
            if elements.y >= 1.0 {
                continue;
            }

            draw_orbital_from_elements(
                elements.x as f32,
                elements.y as f32,
                elements.z as f32,
                elements.w as f32,
                elements.a as f32,
                Srgba::new(0.0, 0.0, 1.0, 0.1),
                &mut gizmos,
            );
        }
    }

    if *sim_state.get() == SimState::Running {
        return;
    }

    let orbital_selection = orbital_selection.as_deref();

    // Draw the orbital of the currently selected catalog object
    if let Some(id) = catalog_ui_state.into_inner().selected_index
        && let Some(entry) = catalog.entries.get(id)
    {
        let catalog_orbit_already_selected = orbital_selection
            .is_some_and(|selection| selection_contains_gpu_index(selection, entry.gpu_index));

        if !catalog_orbit_already_selected
            && let Some(elements) = gpu_elements.0.get(entry.gpu_index)
        {
            draw_orbital_from_elements(
                elements.a,
                elements.e,
                elements.i,
                elements.raan,
                elements.argp,
                Srgba::new(0.0, 0.0, 1.0, 0.1),
                &mut gizmos,
            );
        }
    }

    if let Some(orbital_selection) = orbital_selection {
        if let Some(target) = orbital_selection.target.as_ref() {
            draw_orbital_selection_from_elements(
                &target.elements,
                Srgba::new(0.2, 1.0, 0.45, 0.22),
                &mut gizmos,
            );
        }

        if let Some(chaser) = orbital_selection.chaser.as_ref() {
            draw_orbital_selection_from_elements(
                &chaser.elements,
                Srgba::new(1.0, 0.72, 0.15, 0.22),
                &mut gizmos,
            );
        }
    }
}

/// The capture state [`capture_gizmos`] draws.
#[derive(SystemParam)]
pub struct CaptureGizmoInputs<'w> {
    capture_plan_lib: Res<'w, CapturePlanLibrary>,
    orbital_entities: Res<'w, OrbitalCache>,
    capture_sphere_radius: Res<'w, CaptureSphereRadius>,
}

/// The length of the force ray at the phase's `max_force_n` (1 m per N is too long).
const FORCE_GIZMO_LENGTH_M: f64 = 2.0;

pub fn capture_gizmos(
    capture_entities: Query<(Entity, &CaptureComponent)>,
    capture_inputs: CaptureGizmoInputs,
    rigidbodies: Query<RigidBodyQuery>,
    capture_forces: Query<&CaptureForce>,
    settings: Res<Settings>,
    mut gizmos: Gizmos<CaptureGizmoConfigGroup>,
    camera_s: Single<&RenderLayers, (With<Camera3d>, Without<Orbital>)>,
) {
    let CaptureGizmoInputs {
        capture_plan_lib,
        orbital_entities,
        capture_sphere_radius,
    } = capture_inputs;
    let render_layers = camera_s.into_inner();

    if !settings.capture_gizmos || render_layers.intersects(&RenderLayers::layer(MAP_LAYER)) {
        return;
    }

    for (capture_entity, capture_component) in capture_entities {
        let Ok(capture_entity_rb) = rigidbodies.get(capture_entity) else {
            continue;
        };
        let Some(plan) = capture_plan_lib.plans.get(&capture_component.plan_id) else {
            continue;
        };
        let Some(phase) = plan
            .phases
            .iter()
            .find(|phase| phase.id == capture_component.current_state)
        else {
            continue;
        };
        let Some(nodes) = orbital_entities.tethers.get(&plan.tether) else {
            continue;
        };

        gizmos.sphere(
            Isometry3d::new(
                capture_entity_rb.position.as_vec3(),
                capture_entity_rb.rotation.as_quat(),
            ),
            capture_sphere_radius.radius as f32,
            Srgba::new(1.0, 0.5, 0.0, 0.2),
        );
        gizmos.sphere(
            Isometry3d::new(
                capture_entity_rb.position.as_vec3(),
                capture_entity_rb.rotation.as_quat(),
            ),
            capture_sphere_radius.radius as f32 + 1.0,
            Srgba::new(0.0, 0.8, 0.4, 0.2),
        );

        // F-41: the force the controller applied (its `CaptureForce`), drawn
        // 2 m long at the phase's bound, and the velocity relative to the target.
        let max_force = phase.max_force_n;
        for &node in nodes {
            let (Ok(rb), Ok(force)) = (rigidbodies.get(node), capture_forces.get(node)) else {
                continue;
            };

            let rel_v = rb.linear_velocity.0 - capture_entity_rb.linear_velocity.0;
            let force_ray = if max_force > 0.0 {
                force.applied / max_force * FORCE_GIZMO_LENGTH_M
            } else {
                DVec3::ZERO
            };

            gizmos.ray(
                rb.position.as_vec3(),
                force_ray.as_vec3(),
                Srgba::new(1.0, 0.0, 0.0, 0.2),
            );
            gizmos.ray(
                rb.position.as_vec3(),
                rel_v.as_vec3(),
                Srgba::new(0.0, 1.0, 0.0, 0.2),
            );
        }
    }
}

/// The running tethers' body positions in chain order: each tether's root
/// (`OrbitalCache.tethers[_][0]`) along its distance joints to the tail.
pub(crate) fn running_tether_chains(
    cache: &OrbitalCache,
    joints: &Query<&DistanceJoint>,
    transforms: &Query<&Transform>,
) -> Vec<Vec<Vec3>> {
    cache
        .tethers
        .values()
        .filter_map(|nodes| nodes.first())
        .map(|&root| {
            chain_from_joints(root, joints.iter())
                .into_iter()
                .filter_map(|body| transforms.get(body).ok())
                .map(|transform| transform.translation)
                .collect()
        })
        .collect()
}

/// The replay's tether body positions in body-table order (root, nodes, tail):
/// the replay bodies with a tether shape (the target has none).
pub(crate) fn replay_tether_chain(
    bodies: &Query<(&ReplayBody, &Transform), With<TetherShape>>,
) -> Vec<Vec3> {
    let mut chain: Vec<(usize, Vec3)> = bodies
        .iter()
        .map(|(body, transform)| (body.index, transform.translation))
        .collect();
    chain.sort_by_key(|(index, _)| *index);
    chain.into_iter().map(|(_, position)| position).collect()
}

/// A line between consecutive tether bodies, in the running sim and in a replay:
/// the rope's nodes are 3 cm point masses, so the line is what shows the rope
/// (P7.5 S2). Drawn in the scene view only, in the tether's red.
pub fn tether_segments(
    state: Res<State<SimState>>,
    cache: Res<OrbitalCache>,
    joints: Query<&DistanceJoint>,
    transforms: Query<&Transform>,
    replay_bodies: Query<(&ReplayBody, &Transform), With<TetherShape>>,
    mut gizmos: Gizmos<CaptureGizmoConfigGroup>,
) {
    let chains = match state.get() {
        SimState::Running => running_tether_chains(&cache, &joints, &transforms),
        SimState::Replay => vec![replay_tether_chain(&replay_bodies)],
        SimState::Setup => return,
    };
    for chain in chains {
        gizmos.linestrip(chain, Srgba::new(1.0, 0.0, 0.0, 1.0));
    }
}

pub fn dev_gizmos(
    true_params_query: Query<(&Orbital, &Transform), Without<RigidBodyDisabled>>,
    mut gizmos: Gizmos,
    settings: Res<Settings>,
    camera_s: Single<&RenderLayers, (With<Camera3d>, Without<Orbital>)>,
    orbital_cache: Res<OrbitalCache>,
) {
    let render_layers = camera_s.into_inner();

    if !settings.dev_gizmos || render_layers.intersects(&RenderLayers::layer(MAP_LAYER)) {
        return;
    }

    // Origin gizmo
    gizmos.axes(Transform::from_xyz(0.0, 0.0, 0.0), 2.0);

    // Physics enable radius gizmo
    gizmos.sphere(
        Isometry3d::new(
            Vec3::new(0.0, 0.0, 0.0),
            Quat::from_euler(EulerRot::XYZ, 0.0, 0.0, 0.0),
        ),
        PHYSICS_ENABLE_RADIUS as f32,
        Srgba::new(1.0, 0.0, 0.0, 0.2),
    );

    // Center of Mass gizmos
    orbital_cache
        .com_rv
        .iter()
        .for_each(|(_entity, (com_r, _com_v))| {
            gizmos.sphere(
                Isometry3d::new(
                    com_r.as_vec3(),
                    Quat::from_euler(EulerRot::XYZ, 0.0, 0.0, 0.0),
                ),
                1.0,
                Srgba::new(0.0, 1.0, 1.0, 0.5),
            );
        });

    // Floating origin reset gizmo
    gizmos.sphere(
        Isometry3d::new(
            Vec3::new(0.0, 0.0, 0.0),
            Quat::from_euler(EulerRot::XYZ, 0.0, 0.0, 0.0),
        ),
        MAX_ORIGIN_OFFSET as f32,
        Srgba::new(1.0, 0.0, 1.0, 0.2),
    );

    for (_orbital, transform) in true_params_query {
        gizmos.arrow(
            transform.translation + Vec3::new(10.0, 10.0, 10.0),
            transform.translation,
            Color::srgb(0.0, 1.0, 0.5),
        );
    }
}

/// Gizmo configs, the clear colour, the gizmo systems and the floating-origin
/// visuals (which run with the scene gizmos, as before P3).
pub struct GizmoPlugin;

impl Plugin for GizmoPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (dev_gizmos, capture_gizmos, floating_origin_update_visuals)
                .run_if(in_state(UiScreen::Sim))
                .run_if(in_state(SimState::Running)),
        )
        .add_systems(Update, tether_segments.run_if(in_state(UiScreen::Sim)))
        .add_systems(Last, orbital_gizmos.run_if(in_state(UiScreen::Sim)))
        .insert_gizmo_config(
            DefaultGizmoConfigGroup,
            GizmoConfig {
                render_layers: RenderLayers::from_layers(&[SCENE_LAYER, MAP_LAYER]),
                ..default()
            },
        )
        .insert_gizmo_config(
            CaptureGizmoConfigGroup,
            GizmoConfig {
                render_layers: RenderLayers::layer(SCENE_LAYER),
                ..default()
            },
        )
        .insert_resource(ClearColor(Color::srgb(0.0, 0.0, 0.0)));
    }
}
