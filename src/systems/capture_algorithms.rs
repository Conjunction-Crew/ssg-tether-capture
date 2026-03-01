use std::char::MAX;

use avian3d::{
    math::PI,
    prelude::{Forces, LinearVelocity, Position, RigidBodyForces, RigidBodyQuery},
};
use bevy::{
    color::palettes::css::{GREEN, RED},
    prelude::*,
};

use crate::{
    components::capture_components::CaptureComponent,
    resources::{
        capture_plans::CapturePlanLibrary,
        orbital_entities::{self, OrbitalEntities},
    },
};

const MAX_SPEED: f32 = 10.0;
const MAX_FORCE: f32 = 1000.0;

pub fn capture_state_machine_update(
    mut commands: Commands,
    capture_entities: Query<(Entity, &mut CaptureComponent)>,
    capture_plan_lib: ResMut<CapturePlanLibrary>,
    orbital_entities: Res<OrbitalEntities>,
    mut rb_forces: ParamSet<(Query<RigidBodyQuery>, Query<Forces>)>,
    mut gizmos: Gizmos,
    time: Res<Time>,
) {
    for (capture_entity, mut capture_component) in capture_entities {
        capture_component.state_elapsed_time_s = time.elapsed_secs_f64();

        // Get current position and velocity of capture entity
        let capture_entity_position: Position;
        let capture_entity_linvel: LinearVelocity;
        let rigidbodies = rb_forces.p0();
        if let Ok(capture_entity_rb) = rigidbodies.get(capture_entity) {
            capture_entity_position = capture_entity_rb.position.clone();
            capture_entity_linvel = capture_entity_rb.linear_velocity.clone();
        } else {
            return;
        }

        // Execute plan state machine
        if let Some(plan) = capture_plan_lib.plans.get(&capture_component.plan_id) {
            if let Some(nodes) = orbital_entities.tethers.get(&plan.tether) {
                for &node in nodes {
                    // Get rigidbody of node
                    let rigidbodies = rb_forces.p0();
                    if let Ok(rb) = rigidbodies.get(node) {
                        // Calculate relative position / velocity to the target
                        let world_r = rb.position.0.clone();
                        let rel_r = capture_entity_position.0 - rb.position.0;
                        let rel_v = rb.linear_velocity.0 - capture_entity_linvel.0;

                        // Draw rel_v
                        gizmos.ray(world_r, rel_v, GREEN);

                        // Get force for node
                        let mut forces = rb_forces.p1();
                        if let Ok(mut node_forces) = forces.get_mut(node) {
                            let force_vec: Vec3;
                            // If vel is high, kill vel
                            if rel_v.angle_between(rel_r) > 1.0 || rel_v.length() > MAX_SPEED {
                                force_vec = -rel_v.normalize_or_zero();
                            }
                            // If vel angle is slightly off, get perpendicular force
                            else if rel_v.angle_between(rel_r) > 0.1 {
                                force_vec = rel_v.cross(rel_r.cross(rel_v)).normalize_or_zero()
                            }
                            // Otherwise, force in target dir
                            else {
                                force_vec = rel_r.normalize_or_zero();
                            }

                            // Draw force
                            gizmos.ray(world_r, force_vec, RED);

                            node_forces
                                .apply_force(force_vec * rel_r.length().clamp(10.0, MAX_FORCE));
                        } else {
                            println!("Faled to apply force for node")
                        };
                    }
                }
            }
        }
    }
}
