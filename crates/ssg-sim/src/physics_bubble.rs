//! Physics bubble: bodies near the anchor are simulated, the rest are propagated.

use avian3d::prelude::{RigidBodyDisabled, RigidBodyQuery};
use bevy::math::DVec3;
use bevy::prelude::*;
use brahe::KeplerianPropagator;

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
    mut orbital_cache: ResMut<OrbitalCache>,
    world_time: Res<WorldTime>,
    mut log_events: MessageWriter<LogEvent>,
) {
    let entity = target_entity.into_inner();

    // Get current cartesian state of our target
    let Some(mut target_rv) = orbital_cache.eci_states.get_mut(&entity).cloned() else {
        return;
    };

    // Loop through entities to see if any should be disabled/enabled
    for (entity, mut orbital, mut rb) in orbital_entities {
        let Some(entity_rv) = orbital_cache.eci_states.get_mut(&entity) else {
            return;
        };

        let mut enabled = false;

        if !disabled_entities.contains(entity) && rb.position.0.length() > PHYSICS_DISABLE_RADIUS {
            target_rv[0] += rb.position.x;
            target_rv[1] += rb.position.y;
            target_rv[2] += rb.position.z;
            target_rv[3] += rb.linear_velocity.x;
            target_rv[4] += rb.linear_velocity.y;
            target_rv[5] += rb.linear_velocity.z;

            orbital.propagator = Some(KeplerianPropagator::from_eci(
                world_time.epoch,
                target_rv,
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
