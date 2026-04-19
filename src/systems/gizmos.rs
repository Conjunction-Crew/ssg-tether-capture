use avian3d::{
    math::PI,
    prelude::{RigidBodyDisabled, RigidBodyQuery},
};
use bevy::{
    camera::visibility::RenderLayers,
    math::{DQuat, DVec3},
    prelude::*,
};
use brahe::{AngleFormat, utils::DOrbitStateProvider};

use crate::{
    components::{capture_components::CaptureComponent, orbit::Orbital},
    constants::{
        MAP_LAYER, MAP_UNITS_TO_M, MAX_ORIGIN_OFFSET, PHYSICS_ENABLE_RADIUS, SCENE_LAYER,
        orbit_frame_rotation,
    },
    plugins::gpu_compute::{
        GpuElements, GpuOrbitalElements, eci_position_to_map, propagate_catalog_eci_state,
    },
    plugins::orbital_mechanics::SimState,
    resources::{
        capture_plans::{CapturePlanLibrary, CaptureSphereRadius},
        orbital_cache::OrbitalCache,
        settings::Settings,
        space_catalog::{
            EditableOrbitalElements, OrbitalSelectionSource, OrbitalSelectionState,
            SelectedOrbitalObject, SpaceCatalogUiState, SpaceObjectCatalog,
        },
        world_time::WorldTime,
    },
};

#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct CaptureGizmoConfigGroup;

fn draw_orbital_from_elements(
    semi_major: f32,
    eccentricity: f32,
    inclination: f32,
    raan: f32,
    arg_of_perigee: f32,
    frame_rotation: Quat,
    color: Srgba,
    gizmos: &mut Gizmos,
) {
    if semi_major <= 0.0 || !(0.0..1.0).contains(&eccentricity) {
        return;
    }

    let map_scale = MAP_UNITS_TO_M as f32;
    let scaled_semi_major = semi_major / map_scale;
    let semi_minor = semi_major * (1.0 - eccentricity * eccentricity).sqrt() / map_scale;

    let rotation = frame_rotation
        * Quat::from_axis_angle(Vec3::Z, raan)
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
        orbit_frame_rotation(),
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
        epoch_offset_seconds: elements.epoch_offset_seconds as f32,
    };

    if let Some((position_eci, _velocity_eci)) =
        propagate_catalog_eci_state(&orbital_point, elements.epoch_offset_seconds as f32)
    {
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

pub fn orbital_gizmos(
    orbitals: Query<&Orbital>,
    camera_s: Single<&RenderLayers, (With<Camera3d>, Without<Orbital>)>,
    world_time: Res<WorldTime>,
    mut gizmos: Gizmos,
    catalog_ui_state: Res<SpaceCatalogUiState>,
    catalog: Res<SpaceObjectCatalog>,
    gpu_elements: Res<GpuElements>,
    orbital_selection: Option<Res<OrbitalSelectionState>>,
    sim_state: Res<State<SimState>>,
) {
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
                orbit_frame_rotation(),
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
    if let Some(id) = catalog_ui_state.into_inner().selected_index {
        if let Some(entry) = catalog.entries.get(id) {
            let catalog_orbit_already_selected = orbital_selection
                .is_some_and(|selection| selection_contains_gpu_index(selection, entry.gpu_index));

            if !catalog_orbit_already_selected {
                if let Some(elements) = gpu_elements.0.get(entry.gpu_index) {
                    draw_orbital_from_elements(
                        elements.a,
                        elements.e,
                        elements.i,
                        elements.raan,
                        elements.argp,
                        orbit_frame_rotation(),
                        Srgba::new(0.0, 0.0, 1.0, 0.1),
                        &mut gizmos,
                    );
                }
            }
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

fn capture_force_direction(
    rel_r: DVec3,
    rel_v: DVec3,
    capture_entity_rotation: DQuat,
    capture_radius: f64,
    max_velocity: f64,
    tangent_sign: f64,
) -> DVec3 {
    let mut force_vec = DVec3::ZERO;

    if rel_v.length() > max_velocity {
        force_vec += -rel_v.normalize_or_zero();
    }

    if rel_r.length() < capture_radius * 0.8 {
        force_vec += -rel_r.normalize_or_zero();
    } else if rel_r.length() > capture_radius {
        if rel_v.angle_between(rel_r) > PI / 2.0 {
            force_vec += -rel_v.normalize_or_zero();
        }

        force_vec += rel_r.normalize_or_zero();
    } else {
        let up = (capture_entity_rotation * DVec3::X).normalize_or(DVec3::X);
        let tangent_axis = if rel_r.cross(up).length_squared() > 1e-6 {
            up
        } else {
            DVec3::X
        };

        force_vec += tangent_sign * tangent_axis.cross(rel_r).normalize_or_zero();
    }

    force_vec
}

pub fn capture_gizmos(
    capture_entities: Query<(Entity, &CaptureComponent)>,
    capture_plan_lib: Res<CapturePlanLibrary>,
    orbital_entities: Res<OrbitalCache>,
    rigidbodies: Query<RigidBodyQuery>,
    capture_sphere_radius: Res<CaptureSphereRadius>,
    settings: Res<Settings>,
    mut gizmos: Gizmos<CaptureGizmoConfigGroup>,
    camera_s: Single<&RenderLayers, (With<Camera3d>, Without<Orbital>)>,
) {
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
        let Some(state) = plan
            .states
            .iter()
            .find(|state| state.id == capture_component.current_state)
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

        let (base_max_velocity, capture_state) = if let Some(parameters) = &state.parameters {
            let max_velocity = parameters
                .get("max_velocity")
                .and_then(|value| value.as_f64())
                .unwrap_or(0.0) as f64;
            (max_velocity, capture_component.current_state == "capture")
        } else {
            (0.0, capture_component.current_state == "capture")
        };

        for (idx, &node) in nodes.iter().enumerate() {
            let Ok(rb) = rigidbodies.get(node) else {
                continue;
            };

            let rel_r = capture_entity_rb.position.0 - rb.position.0;
            let rel_v = rb.linear_velocity.0 - capture_entity_rb.linear_velocity.0;

            let mut max_velocity = base_max_velocity;
            let mut capture_radius = capture_sphere_radius.radius;
            if idx != 0 {
                max_velocity *= 0.9;
                capture_radius += 1.0;
            }

            let tangent_sign = if idx != 0 && capture_state { -1.0 } else { 1.0 };
            let force_vec = capture_force_direction(
                rel_r,
                rel_v,
                capture_entity_rb.rotation.0,
                capture_radius,
                max_velocity,
                tangent_sign,
            );

            gizmos.ray(
                rb.position.as_vec3(),
                force_vec.as_vec3(),
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
