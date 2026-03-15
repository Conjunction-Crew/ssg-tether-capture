use crate::components::orbit::{Earth, Orbit, Orbital, TetherNode, TrueParams};
use crate::components::orbit_camera::CameraTarget;
use crate::constants::{
    MAP_LAYER, MAX_ORIGIN_OFFSET, PHYSICS_DISABLE_RADIUS, PHYSICS_ENABLE_RADIUS,
};
use crate::resources::orbital_entities::OrbitalEntities;
use crate::resources::world_time::WorldTime;

use avian3d::prelude::{RigidBody, RigidBodyDisabled, RigidBodyQuery};
use bevy::camera::visibility::RenderLayers;
use bevy::math::DVec3;
use bevy::pbr::Atmosphere;
use bevy::prelude::*;
use brahe::utils::DOrbitStateProvider;
use brahe::{Epoch, KeplerianPropagator, par_propagate_to_s};
use nalgebra::Vector6;

pub fn ssg_propagate_keplerian(
    mut orbitals: Query<(Entity, &mut TrueParams, &mut Orbital), With<RigidBody>>,
    mut world_time: ResMut<WorldTime>,
    mut orbital_entities: ResMut<OrbitalEntities>,
    time: Res<Time>,
) {
    let dt = time.delta_secs_f64() * world_time.multiplier;
    world_time.epoch += dt;

    par_propagate_to_s(
        orbital_entities.propagators.as_mut_slice(),
        world_time.epoch,
    );

    for (_entity, mut true_params, orbital) in orbitals {
        let propagator = orbital_entities.propagators[orbital.propagator_id].clone();
        if let Ok(eci) = propagator.state_eci(world_time.epoch) {
            true_params.rv = eci;
        }
    }
}

pub fn init_orbitals(
    mut commands: Commands,
    mut orbital_entities: ResMut<OrbitalEntities>,
    mut q: Query<(Entity, &Orbit, &mut Orbital, &mut TrueParams), Added<Orbit>>,
) {
    for (entity, init, mut orbital, mut true_params) in &mut q {
        true_params.rv = match init {
            Orbit::FromParams(params) => params.rv,
            Orbit::FromElements(elements) => {
                let epoch = Epoch::now();
                orbital.elements = Some(*elements);
                let propagator = KeplerianPropagator::from_keplerian(
                    epoch,
                    *elements,
                    brahe::AngleFormat::Radians,
                    1.0,
                );
                if let Ok(eci) = propagator.state_eci(epoch) {
                    orbital_entities.propagators.push(propagator);
                    orbital.propagator_id = orbital_entities.propagators.len() - 1;
                    eci
                } else {
                    Vector6::new(0.0, 0.0, 0.0, 0.0, 0.0, 0.0)
                }
            }
            Orbit::FromTle(tle) => {
                // TODO: init logic from TLE data (sgp4)
                Vector6::new(0.0, 0.0, 0.0, 0.0, 0.0, 0.0)
            }
        };

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
        (target_params.rv[0]) as f32,
        (target_params.rv[1]) as f32,
        (target_params.rv[2]) as f32,
    );
    earth_transform.translation = new_translation;
    atmosphere.world_position = new_translation;

    // Loop over each other true params to get position
    for (true_params, mut transform) in true_params_q {
        let new_translation = Vec3::new(
            (true_params.rv[0] - target_params.rv[0]) as f32,
            (true_params.rv[1] - target_params.rv[1]) as f32,
            (true_params.rv[2] - target_params.rv[2]) as f32,
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
        true_params.rv[0] += com_pos.x as f64;
        true_params.rv[1] += com_pos.y as f64;
        true_params.rv[2] += com_pos.z as f64;
        true_params.rv[3] += com_linvel.x as f64;
        true_params.rv[4] += com_linvel.y as f64;
        true_params.rv[5] += com_linvel.z as f64;
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
    let origin_pos = DVec3::new(target_true.rv[0], target_true.rv[1], target_true.rv[2]);
    let origin_vel = DVec3::new(target_true.rv[3], target_true.rv[4], target_true.rv[5]);

    // Loop through entities to see if any should be disabled/enabled
    for (entity, mut true_params, mut rb) in orbital_entities {
        let relative_pos =
            DVec3::new(true_params.rv[0], true_params.rv[1], true_params.rv[2]) - origin_pos;

        if !disabled_entities.contains(entity)
            && (rb.position.0).length() > PHYSICS_DISABLE_RADIUS as f32
        {
            true_params.rv[0] += rb.position.x as f64;
            true_params.rv[1] += rb.position.y as f64;
            true_params.rv[2] += rb.position.z as f64;
            true_params.rv[3] += rb.linear_velocity.x as f64;
            true_params.rv[4] += rb.linear_velocity.y as f64;
            true_params.rv[5] += rb.linear_velocity.z as f64;
            commands.entity(entity).insert(RigidBodyDisabled);
            println!("DISABLED SOMETHING");
        } else if disabled_entities.contains(entity)
            && (relative_pos).length() < PHYSICS_ENABLE_RADIUS
        {
            let relative_vel =
                DVec3::new(true_params.rv[0], true_params.rv[1], true_params.rv[2]) - origin_vel;
            println!("rel: {}", relative_pos);

            true_params.rv[0] -= relative_pos.x;
            true_params.rv[1] -= relative_pos.y;
            true_params.rv[2] -= relative_pos.z;
            true_params.rv[3] -= relative_vel.x;
            true_params.rv[4] -= relative_vel.y;
            true_params.rv[5] -= relative_vel.z;

            rb.position.0 = relative_pos.as_vec3();
            rb.linear_velocity.0 = relative_vel.as_vec3();
            commands.entity(entity).remove::<RigidBodyDisabled>();
            println!("ENABLED SOMETHING");
        }
    }
}
