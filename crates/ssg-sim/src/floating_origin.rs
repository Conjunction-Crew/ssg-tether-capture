//! Floating origin: keeps the anchor body close to the physics origin.

use avian3d::prelude::{RigidBodyDisabled, RigidBodyQuery};
use bevy::{math::DVec3, prelude::*};
use brahe::KeplerianPropagator;
use brahe::utils::DOrbitStateProvider;
use nalgebra::DVector;

use crate::SimStep;
use crate::anchor::SimAnchor;
use crate::orbit::Orbital;
use crate::orbit::cache::OrbitalCache;
use crate::physics_bubble::PHYSICS_ENABLE_RADIUS;
use crate::telemetry::{LogEvent, LogLevel};
use crate::tether::{TetherNode, TetherRoot};
use crate::time::{ManualPhysics, PHYS_DT, WorldTime};

pub const MAX_ORIGIN_OFFSET: f64 = PHYSICS_ENABLE_RADIUS / 2.0;

/// One floating-origin reset (P7, C2): every enabled body's `Position` and
/// `LinearVelocity` moved by minus `(com_r, com_v)` at the end of fixed step `step`.
#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub struct OriginReset {
    pub step: u64,
    pub com_r: DVec3,
    pub com_v: DVec3,
}

pub fn calculate_com_rv(
    entities: Query<Entity, With<TetherRoot>>,
    rigidbodies: Query<RigidBodyQuery, Without<RigidBodyDisabled>>,
    nodes: Query<(Entity, &TetherNode)>,
    mut orbital_cache: ResMut<OrbitalCache>,
) {
    for entity in entities {
        let Ok(target_rb) = rigidbodies.get(entity) else {
            continue;
        };

        let mut weighted_pos = (target_rb.position.0
            + target_rb.rotation.0 * target_rb.center_of_mass.0)
            * target_rb.mass.value();
        let mut weighted_linvel = target_rb.linear_velocity.0 * target_rb.mass.value();
        let mut total_mass = target_rb.mass.value();

        for (node_entity, node) in nodes.iter() {
            if node.root != entity {
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
            continue;
        }

        orbital_cache.com_rv.insert(
            entity,
            (weighted_pos / total_mass, weighted_linvel / total_mass),
        );
    }
}

pub fn target_entity_reset_origin(
    mut true_params_query: Query<&mut Orbital, Without<RigidBodyDisabled>>,
    mut rigidbodies: Query<RigidBodyQuery, Without<RigidBodyDisabled>>,
    target_entity_q: Query<Entity, (With<SimAnchor>, Without<RigidBodyDisabled>)>,
    world_time: Res<WorldTime>,
    orbital_cache: Res<OrbitalCache>,
    (mut log_events, mut resets): (MessageWriter<LogEvent>, MessageWriter<OriginReset>),
) {
    let Ok(target_entity) = target_entity_q.single() else {
        return;
    };

    let Some((com_r, com_v)) = orbital_cache.com_rv.get(&target_entity) else {
        return;
    };

    if com_r.length() <= MAX_ORIGIN_OFFSET {
        return;
    }

    log_events.write(LogEvent {
        level: LogLevel::Debug,
        source: "propagation",
        message: format!("Floating origin reset (epoch: {})", world_time.epoch),
    });
    resets.write(OriginReset {
        step: ((world_time.epoch - world_time.start_epoch) / PHYS_DT).round() as u64,
        com_r: *com_r,
        com_v: *com_v,
    });

    // Accumulate current linvel and position into rigidbodies
    true_params_query.par_iter_mut().for_each(|mut orbital| {
        if let Some(prop) = orbital.propagator.as_mut() {
            let Ok(rv) = prop.state_eci(world_time.epoch) else {
                return;
            };

            let new_rv = rv
                + DVector::<f64>::from_vec(vec![
                    com_r.x, com_r.y, com_r.z, com_v.x, com_v.y, com_v.z,
                ]);

            // Rebuild propagator
            *prop = KeplerianPropagator::from_eci(world_time.epoch, new_rv, 1.0);
        }
    });

    // Reset rigidbodies
    rigidbodies.par_iter_mut().for_each(|mut rb| {
        rb.position.0 -= com_r;
        rb.linear_velocity.0 -= com_v;
    });
}

pub struct FloatingOriginPlugin;

impl Plugin for FloatingOriginPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<OriginReset>().add_systems(
            ManualPhysics,
            (
                calculate_com_rv.in_set(SimStep::CenterOfMass),
                target_entity_reset_origin.in_set(SimStep::OriginReset),
            ),
        );
    }
}
