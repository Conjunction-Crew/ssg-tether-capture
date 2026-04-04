use crate::components::orbit::{Earth, Orbit, Orbital, TetherNode};
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
use brahe::{Epoch, KeplerianPropagator};
use nalgebra::{DVector, Vector6};

fn calculate_com_rv(
    target_entity: Entity,
    rigidbodies: &Query<RigidBodyQuery, Without<RigidBodyDisabled>>,
    nodes: &Query<(Entity, &TetherNode)>,
) -> Option<(DVec3, DVec3)> {
    let Ok(target_rb) = rigidbodies.get(target_entity) else {
        return None;
    };

    let mut weighted_pos = (target_rb.position.0
        + target_rb.rotation.0 * target_rb.center_of_mass.0)
        * target_rb.mass.value();
    let mut weighted_linvel = target_rb.linear_velocity.0 * target_rb.mass.value();
    let mut total_mass = target_rb.mass.value();

    for (node_entity, node) in nodes.iter() {
        if node.root != target_entity {
            continue;
        }

        let Ok(node_rb) = rigidbodies.get(node_entity) else {
            continue;
        };

        weighted_pos += (node_rb.position.0 + node_rb.rotation.0 * node_rb.center_of_mass.0)
            * node_rb.mass.value();
        weighted_linvel += node_rb.linear_velocity.0 * node_rb.mass.value();
        total_mass += node_rb.mass.value();
    }

    if total_mass <= 0.0 {
        return None;
    }

    Some((weighted_pos / total_mass, weighted_linvel / total_mass))
}

pub fn init_orbitals(
    mut commands: Commands,
    mut orbital_entities: ResMut<OrbitalEntities>,
    mut q: Query<(Entity, &Orbit, &mut Orbital), Added<Orbit>>,
) {
    for (entity, init, mut orbital) in &mut q {
        match init {
            Orbit::FromElements(elements) => {
                let epoch = Epoch::now();
                let propagator = KeplerianPropagator::from_keplerian(
                    epoch,
                    *elements,
                    brahe::AngleFormat::Radians,
                    1.0,
                );
                if let Ok(eci) = propagator.state_eci(epoch) {
                    orbital_entities
                        .propagators
                        .push(KeplerianPropagator::from_eci(epoch, eci, 1.0));
                    orbital.propagator_id = orbital_entities.propagators.len() - 1;
                    println!("ECI Initialized to: {}", eci);
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

pub fn floating_origin_update_visuals(
    target_params_s: Single<
        (&Orbital, &Transform),
        (
            Without<RigidBodyDisabled>,
            With<CameraTarget>,
            Without<Earth>,
        ),
    >,
    camera_s: Single<(&mut Atmosphere, &RenderLayers), (With<Camera3d>, Without<Orbital>)>,
    earth: Single<&mut Transform, (With<Earth>, Without<CameraTarget>)>,
    orbitals: ResMut<OrbitalEntities>,
    world_time: Res<WorldTime>,
) {
    let (mut atmosphere, render_layers) = camera_s.into_inner();

    // Do not calculate floating origin if we are in map view
    if render_layers.intersects(&RenderLayers::layer(MAP_LAYER)) {
        return;
    }

    // We want to position orbital objects relative to the camera's current target
    let (target_orbital, target_transform) = target_params_s.into_inner();

    // Get current cartesian state of our target
    let Some(prop) = orbitals.propagators.get(target_orbital.propagator_id) else {
        return;
    };

    let Ok(target_rv) = prop.state_eci(world_time.epoch) else {
        return;
    };

    // Earth translation becomes new position
    let mut earth_transform = earth.into_inner();
    let new_translation = -Vec3::new(
        (target_rv[0]) as f32 + target_transform.translation.x,
        (target_rv[1]) as f32 + target_transform.translation.y,
        (target_rv[2]) as f32 + target_transform.translation.z,
    );
    earth_transform.translation = new_translation;
    atmosphere.world_position = new_translation;
}

pub fn target_entity_reset_origin(
    true_params_query: Query<&mut Orbital, Without<RigidBodyDisabled>>,
    rigidbodies: Query<RigidBodyQuery, Without<RigidBodyDisabled>>,
    nodes: Query<(Entity, &TetherNode)>,
    target_entity_q: Query<Entity, (With<CameraTarget>, Without<RigidBodyDisabled>)>,
    mut orbitals: ResMut<OrbitalEntities>,
    world_time: Res<WorldTime>,
) {
    let Ok(target_entity) = target_entity_q.single() else {
        return;
    };

    let Ok(target_rb) = rigidbodies.get(target_entity) else {
        return;
    };

    if target_rb.position.0.length() <= MAX_ORIGIN_OFFSET {
        return;
    }

    println!("RESETTING! EPOCH: {}", world_time.epoch);

    let Some((com_r, com_v)) = calculate_com_rv(target_entity, &rigidbodies, &nodes) else {
        return;
    };

    // Accumulate current linvel and position into rigidbodies
    println!("Num to reset: {}", true_params_query.iter().len());
    for orbital in true_params_query {
        if let Some(prop) = orbitals.propagators.get_mut(orbital.propagator_id) {
            let Ok(rv) = prop.state_eci(world_time.epoch) else {
                continue;
            };

            let new_rv = rv
                + DVector::<f64>::from_vec(vec![
                    com_r.x, com_r.y, com_r.z, com_v.x, com_v.y, com_v.z,
                ]);

            // Rebuild propagator
            *prop = KeplerianPropagator::from_eci(world_time.epoch, new_rv, 1.0);

            println!("New propagator, New rv: {}", new_rv);
        }
    }

    // Reset rigidbodies
    for mut rb in rigidbodies {
        rb.position.0 -= com_r;
        rb.linear_velocity.0 -= com_v;
    }
}

pub fn physics_bubble_add_remove(
    mut commands: Commands,
    disabled_entities: Query<(Entity, &RigidBodyDisabled)>,
    orbital_entities: Query<(Entity, &mut Orbital, RigidBodyQuery), Without<CameraTarget>>,
    target_entity: Single<&Orbital, With<CameraTarget>>,
    mut orbitals: ResMut<OrbitalEntities>,
    world_time: Res<WorldTime>,
) {
    let target_orbital = target_entity.into_inner();

    // Get current cartesian state of our target
    let Some(origin_prop) = orbitals.propagators.get_mut(target_orbital.propagator_id) else {
        return;
    };

    let Ok(mut target_rv) = origin_prop.state_eci(world_time.epoch) else {
        return;
    };

    // Loop through entities to see if any should be disabled/enabled
    for (entity, entity_orbital, mut rb) in orbital_entities {
        let Some(prop) = orbitals.propagators.get_mut(entity_orbital.propagator_id) else {
            continue;
        };

        let Ok(mut entity_rv) = prop.state_eci(world_time.epoch) else {
            continue;
        };

        let mut enabled = false;

        if !disabled_entities.contains(entity) && rb.position.0.length() > PHYSICS_DISABLE_RADIUS {
            target_rv[0] += rb.position.x;
            target_rv[1] += rb.position.y;
            target_rv[2] += rb.position.z;
            target_rv[3] += rb.linear_velocity.x;
            target_rv[4] += rb.linear_velocity.y;
            target_rv[5] += rb.linear_velocity.z;

            *prop = KeplerianPropagator::from_eci(world_time.epoch, target_rv, 1.0);

            commands.entity(entity).insert(RigidBodyDisabled);
            println!("DISABLED SOMETHING");
        } else if disabled_entities.contains(entity)
            && rb.position.0.length() < PHYSICS_ENABLE_RADIUS
        {
            // println!("rel: {}", relative_pos);
            entity_rv[0] -= rb.position.x;
            entity_rv[1] -= rb.position.y;
            entity_rv[2] -= rb.position.z;
            entity_rv[3] -= rb.linear_velocity.x;
            entity_rv[4] -= rb.linear_velocity.y;
            entity_rv[5] -= rb.linear_velocity.z;

            // *prop = KeplerianPropagator::from_eci(world_time.epoch, entity_rv, 1.0);

            commands.entity(entity).remove::<RigidBodyDisabled>();

            enabled = true;

            println!("ENABLED SOMETHING");
        }

        // Set disabled bodies rigidbody values to their global relative state (for capture algorithm)
        if !enabled && disabled_entities.contains(entity) {
            rb.position.0 = DVec3::new(
                entity_rv[0] - target_rv[0],
                entity_rv[1] - target_rv[1],
                entity_rv[2] - target_rv[2],
            );
            rb.linear_velocity.0 = DVec3::new(
                entity_rv[3] - target_rv[3],
                entity_rv[4] - target_rv[4],
                entity_rv[5] - target_rv[5],
            );
        }
    }
}
