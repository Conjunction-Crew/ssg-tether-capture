use crate::components::orbit::{Earth, Orbit, Orbital, TetherNode, TrueParams};
use crate::components::orbit_camera::CameraTarget;
use crate::constants::{
    MAP_LAYER, MAX_ORIGIN_OFFSET, PHYSICS_DISABLE_RADIUS, PHYSICS_ENABLE_RADIUS,
};
use crate::resources::time_warp::TimeWarp;

use astrora_core::core::constants::GM_EARTH;
use astrora_core::core::elements::{coe_to_rv, rv_to_coe};
use astrora_core::core::linalg::Vector3;
use astrora_core::propagators::keplerian::batch_propagate_states;
use avian3d::prelude::{RigidBody, RigidBodyDisabled, RigidBodyQuery};
use bevy::camera::visibility::RenderLayers;
use bevy::math::DVec3;
use bevy::pbr::Atmosphere;
use bevy::prelude::*;
use ndarray::ArrayView2;

#[derive(Default)]
pub(crate) struct BatchScratch {
    entities: Vec<Entity>,
    true_params: Vec<f64>,
}

pub fn ssg_propagate_keplerian(
    mut orbitals: Query<(Entity, &mut TrueParams, &mut Orbital), With<RigidBody>>,
    time: Res<Time>,
    time_warp: Res<TimeWarp>,
    mut scratch: Local<BatchScratch>,
) {
    let dt = time.delta_secs_f64() * time_warp.multiplier;

    scratch.entities.clear();
    scratch.true_params.clear();
    scratch.entities.reserve(orbitals.iter().len());
    scratch.true_params.reserve(orbitals.iter().len());

    for (entity, true_params, orbital) in &orbitals {
        if orbital.elements.is_none() {
            continue;
        }
        scratch.entities.push(entity);
        scratch.true_params.extend_from_slice(&true_params.r);
        scratch.true_params.extend_from_slice(&true_params.v);
    }
    if scratch.entities.is_empty() {
        return;
    }

    if let Ok(batch_view) =
        ArrayView2::from_shape((scratch.entities.len(), 6), &scratch.true_params)
    {
        let batch_out = match batch_propagate_states(batch_view, &[dt], GM_EARTH) {
            Ok(out) => out,
            Err(_) => return,
        };

        for (i, entity) in scratch.entities.iter().enumerate() {
            if let Ok((_entity, mut true_params, mut orbital)) = orbitals.get_mut(*entity) {
                if let Some(row) = batch_out.row(i).as_slice() {
                    true_params.r.copy_from_slice(&row[0..3]);
                    true_params.v.copy_from_slice(&row[3..6]);
                    if let Ok(new_elements) = rv_to_coe(
                        &Vector3::from(true_params.r),
                        &Vector3::from(true_params.v),
                        GM_EARTH,
                        1e-8,
                    ) {
                        orbital.elements = Some(new_elements);
                    }
                }
            }
        }
    }
}

pub fn init_orbitals(
    mut commands: Commands,
    mut q: Query<(Entity, &Orbit, &mut Orbital, &mut TrueParams), Added<Orbit>>,
) {
    for (entity, init, mut orbital, mut true_params) in &mut q {
        let (r, v) = match init {
            Orbit::FromParams(params) => (params.r, params.v),
            Orbit::FromElements(elements) => {
                let (r, v) = coe_to_rv(elements, GM_EARTH);
                ([r.x, r.y, r.z], [v.x, v.y, v.z])
            }
            Orbit::FromTle(tle) => {
                // TODO: init logic from TLE data (sgp4)
                ([0.0, 0.0, 0.0], [0.0, 0.0, 0.0])
            }
        };

        true_params.r = r;
        true_params.v = v;

        if let Ok(elements) = rv_to_coe(&Vector3::from(r), &Vector3::from(v), GM_EARTH, 1e-8) {
            orbital.elements = Some(elements);
        }

        commands.entity(entity).remove::<Orbit>();
    }
}

pub fn floating_origin(
    true_params_q: Query<
        (&mut TrueParams, &mut Transform),
        (
            With<RigidBodyDisabled>,
            Without<CameraTarget>,
            Without<Earth>,
        ),
    >,
    target_params_s: Single<
        (&mut TrueParams, &Transform),
        (
            Without<RigidBodyDisabled>,
            With<CameraTarget>,
            Without<Earth>,
        ),
    >,
    camera_s: Single<(&mut Atmosphere, &RenderLayers), (With<Camera3d>, Without<Orbital>)>,
    earth: Single<&mut Transform, (With<Earth>, Without<CameraTarget>)>,
) {
    let (mut atmosphere, render_layers) = camera_s.into_inner();

    // Do not calculate floating origin if we are in map view
    if render_layers.intersects(&RenderLayers::layer(MAP_LAYER)) {
        return;
    }

    // We want to position orbital objects relative to the camera's current target
    let (target_params, target_transform) = target_params_s.into_inner();

    // Earth translation becomes new position
    let mut earth_transform = earth.into_inner();
    let new_translation = -Vec3::new(
        (target_params.r[0]) as f32,
        (target_params.r[1]) as f32,
        (target_params.r[2]) as f32,
    );
    earth_transform.translation = new_translation;
    atmosphere.world_position = new_translation;

    // Loop over each other true params to get position
    for (true_params, mut transform) in true_params_q {
        let new_translation = Vec3::new(
            (true_params.r[0] - target_params.r[0]) as f32,
            (true_params.r[1] - target_params.r[1]) as f32,
            (true_params.r[2] - target_params.r[2]) as f32,
        );
        transform.translation = new_translation;
    }
}

pub fn target_entity_reset_origin(
    true_params_query: Query<&mut TrueParams, Without<RigidBodyDisabled>>,
    rigidbodies: Query<RigidBodyQuery>,
    nodes: Query<(Entity, &TetherNode)>,
    target_entity_q: Query<Entity, (With<CameraTarget>, Without<RigidBodyDisabled>)>,
) {
    let Ok(target_entity) = target_entity_q.single() else {
        return;
    };

    let com_pos: Vec3;
    let com_linvel: Vec3;

    if let Ok(target_rb) = rigidbodies.get(target_entity) {
        // Check if we are too far from the origin, or if linvel is too high
        if target_rb.position.length() > MAX_ORIGIN_OFFSET {
            // Calculate the average velocity/position relative to center of mass (COM)
            let mut weighted_pos = (target_rb.position.0
                + target_rb.rotation.0 * target_rb.center_of_mass.0)
                * target_rb.mass.value();
            let mut weighted_linvel = target_rb.linear_velocity.0 * target_rb.mass.value();
            let mut total_mass = target_rb.mass.value();

            for (node_entity, node) in nodes {
                if node.root == target_entity {
                    if let Ok(node_rb) = rigidbodies.get(node_entity) {
                        weighted_pos += (node_rb.position.0
                            + node_rb.rotation.0 * node_rb.center_of_mass.0)
                            * node_rb.mass.value();
                        weighted_linvel += node_rb.linear_velocity.0 * node_rb.mass.value();
                        total_mass += node_rb.mass.value();
                    }
                }
            }

            // Total mass must be > 0
            if total_mass <= 0.0 {
                return;
            }

            com_pos = weighted_pos / total_mass;
            com_linvel = weighted_linvel / total_mass;
        } else {
            return;
        }
    } else {
        return;
    }

    // Accumulate current linvel and position into true_params before reset
    for mut true_params in true_params_query {
        true_params.r[0] += com_pos.x as f64;
        true_params.r[1] += com_pos.y as f64;
        true_params.r[2] += com_pos.z as f64;
        true_params.v[0] += com_linvel.x as f64;
        true_params.v[1] += com_linvel.y as f64;
        true_params.v[2] += com_linvel.z as f64;
    }

    // Finally, reset all rigidbodies by the com params for the target
    for mut rb in rigidbodies {
        rb.position.0 -= com_pos;
        rb.linear_velocity.0 -= com_linvel;
    }
}
pub fn physics_bubble_add_remove(
    mut commands: Commands,
    disabled_entities: Query<(Entity, &RigidBodyDisabled)>,
    orbital_entities: Query<(Entity, &mut TrueParams, RigidBodyQuery), Without<CameraTarget>>,
    target_entity: Single<&TrueParams, With<CameraTarget>>,
) {
    let target_true = target_entity.into_inner();

    // Floating origin is currently at target_true
    let origin_pos = DVec3::new(target_true.r[0], target_true.r[1], target_true.r[2]);
    let origin_vel = DVec3::new(target_true.v[0], target_true.v[1], target_true.v[2]);

    // Loop through entities to see if any should be disabled/enabled
    for (entity, mut true_params, mut rb) in orbital_entities {
        let relative_pos =
            DVec3::new(true_params.r[0], true_params.r[1], true_params.r[2]) - origin_pos;

        if !disabled_entities.contains(entity)
            && (rb.position.0).length() > PHYSICS_DISABLE_RADIUS as f32
        {
            true_params.r[0] += rb.position.x as f64;
            true_params.r[1] += rb.position.y as f64;
            true_params.r[2] += rb.position.z as f64;
            true_params.v[0] += rb.linear_velocity.x as f64;
            true_params.v[1] += rb.linear_velocity.y as f64;
            true_params.v[2] += rb.linear_velocity.z as f64;
            commands.entity(entity).insert(RigidBodyDisabled);
            println!("DISABLED SOMETHING");
        } else if disabled_entities.contains(entity)
            && (relative_pos).length() < PHYSICS_ENABLE_RADIUS
        {
            let relative_vel =
                DVec3::new(true_params.v[0], true_params.v[1], true_params.v[2]) - origin_vel;
            println!("rel: {}", relative_pos);

            true_params.r[0] -= relative_pos.x;
            true_params.r[1] -= relative_pos.y;
            true_params.r[2] -= relative_pos.z;
            true_params.v[0] -= relative_vel.x;
            true_params.v[1] -= relative_vel.y;
            true_params.v[2] -= relative_vel.z;

            rb.position.0 = relative_pos.as_vec3();
            rb.linear_velocity.0 = relative_vel.as_vec3();
            commands.entity(entity).remove::<RigidBodyDisabled>();
            println!("ENABLED SOMETHING");
        }
    }
}
