//! Physics bubble: bodies near the anchor are simulated, the rest are propagated.
//!
//! The bubble frame is the anchor's Kepler orbit (its `Orbital.propagator`),
//! translating, with ECI axes. For an **enabled** body, `Orbital.propagator`
//! describes the frame origin's orbit (the anchor's, up to numerical noise) and its
//! ECI state is `propagator.state_eci(epoch) + (Position, LinearVelocity)`. For a
//! **disabled** body, `Orbital.propagator` is its own orbit, and its bubble-frame
//! `Position`/`LinearVelocity` are mirrored from `own_state − frame_state` each step.
//!
//! On **disable** the propagator is rebuilt from `frame_state + (Position,
//! LinearVelocity)`; on **enable** from `own_state − (Position, LinearVelocity)`, the
//! frame origin. The floating-origin reset shifts every enabled propagator by the
//! same `(com_r, com_v)`, which keeps "enabled propagator = frame origin" true.

use avian3d::prelude::{RigidBodyDisabled, RigidBodyQuery};
use bevy::math::DVec3;
use bevy::prelude::*;
use brahe::KeplerianPropagator;
use nalgebra::Vector6;

use crate::SimStep;
use crate::anchor::SimAnchor;
use crate::orbit::Orbital;
use crate::orbit::cache::OrbitalCache;
use crate::telemetry::{LogEvent, LogLevel};
use crate::time::{ManualPhysics, WorldTime};

// Floating origin constants
pub const PHYSICS_ENABLE_RADIUS: f64 = 100.0;
pub const PHYSICS_DISABLE_RADIUS: f64 = PHYSICS_ENABLE_RADIUS * 1.1;

pub fn physics_bubble_add_remove(
    mut commands: Commands,
    disabled_entities: Query<(Entity, &RigidBodyDisabled)>,
    orbital_entities: Query<(Entity, &mut Orbital, RigidBodyQuery), Without<SimAnchor>>,
    target_entity: Single<Entity, With<SimAnchor>>,
    orbital_cache: Res<OrbitalCache>,
    world_time: Res<WorldTime>,
    mut log_events: MessageWriter<LogEvent>,
) {
    let entity = target_entity.into_inner();

    // Get current cartesian state of our target (the frame origin); never mutated
    // (F-23).
    let Some(&target_rv) = orbital_cache.eci_states.get(&entity) else {
        return;
    };

    // Loop through entities to see if any should be disabled/enabled
    for (entity, mut orbital, mut rb) in orbital_entities {
        // F-07: a body without a cached state is skipped, not the rest of the loop.
        let Some(&entity_rv) = orbital_cache.eci_states.get(&entity) else {
            continue;
        };

        let relative = Vector6::new(
            rb.position.x,
            rb.position.y,
            rb.position.z,
            rb.linear_velocity.x,
            rb.linear_velocity.y,
            rb.linear_velocity.z,
        );

        let mut enabled = false;

        if !disabled_entities.contains(entity) && rb.position.0.length() > PHYSICS_DISABLE_RADIUS {
            // The leaving body's own orbit: the frame state plus its relative state.
            orbital.propagator = Some(KeplerianPropagator::from_eci(
                world_time.epoch,
                target_rv + relative,
                1.0,
            ));

            commands.entity(entity).insert(RigidBodyDisabled);
            log_events.write(LogEvent {
                level: LogLevel::Debug,
                source: "propagation",
                message: format!(
                    "Physics disabled for entity {:?} (dist {:.0} km from target)",
                    entity,
                    rb.position.0.length() / 1000.0,
                ),
            });
        } else if disabled_entities.contains(entity)
            && rb.position.0.length() < PHYSICS_ENABLE_RADIUS
        {
            // F-07: the entering body's propagator becomes the frame origin's orbit.
            orbital.propagator = Some(KeplerianPropagator::from_eci(
                world_time.epoch,
                entity_rv - relative,
                1.0,
            ));

            commands.entity(entity).remove::<RigidBodyDisabled>();

            enabled = true;

            log_events.write(LogEvent {
                level: LogLevel::Debug,
                source: "propagation",
                message: format!(
                    "Physics enabled for entity {:?} (dist {:.0} km from target)",
                    entity,
                    rb.position.0.length() / 1000.0,
                ),
            });
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

pub struct PhysicsBubblePlugin;

impl Plugin for PhysicsBubblePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            ManualPhysics,
            physics_bubble_add_remove.in_set(SimStep::Bubble),
        );
    }
}
