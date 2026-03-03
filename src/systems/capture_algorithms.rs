use std::char::MAX;

use avian3d::{
    math::PI,
    prelude::{Forces, LinearVelocity, Position, RigidBodyForces, RigidBodyQuery},
};
use bevy::{
    color::palettes::css::{GREEN, ORANGE, RED},
    prelude::*,
};

use crate::{
    components::capture_components::CaptureComponent,
    resources::{capture_plans::CapturePlanLibrary, orbital_entities::OrbitalEntities},
};

const MAX_SPEED: f32 = 5.0;
const MAX_FORCE: f32 = 1000.0;
const APPROACH_RADIUS: f32 = 20.0;

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
        if let Ok(capture_entity_rb) = rb_forces.p0().get(capture_entity) {
            capture_entity_position = capture_entity_rb.position.clone();
            capture_entity_linvel = capture_entity_rb.linear_velocity.clone();
        } else {
            return;
        }

        // Execute plan state machine
        if let Some(plan) = capture_plan_lib.plans.get(&capture_component.plan_id) {
            if let Some(nodes) = orbital_entities.tethers.get(&plan.tether) {
                for (idx, &node) in nodes.iter().enumerate() {
                    // Get rigidbody of node
                    let world_r: Vec3;
                    let rel_r: Vec3;
                    let rel_v: Vec3;
                    if let Ok(rb) = rb_forces.p0().get(node) {
                        // Calculate relative position / velocity to the target
                        world_r = rb.position.0.clone();
                        rel_r = capture_entity_position.0 - rb.position.0;
                        rel_v = rb.linear_velocity.0 - capture_entity_linvel.0;
                    } else {
                        continue;
                    }

                    // Draw rel_v
                    gizmos.ray(world_r, rel_v, GREEN);

                    // Draw sphere
                    gizmos.sphere(
                        Isometry3d::new(
                            capture_entity_position.0.clone(),
                            Quat::from_euler(EulerRot::XYZ, 0.0, 0.0, 0.0),
                        ),
                        APPROACH_RADIUS,
                        ORANGE,
                    );

                    // Get force for node
                    let mut force_vec: Vec3 = Vec3::ZERO;
                    // If this node is not 0 idx and is close, repel against 0 idx
                    if idx != 0
                        && let Ok(n0) = rb_forces.p0().get(nodes[0])
                        && let rel_n0 = world_r - n0.position.0
                        && rel_n0.length() < 10.0
                    {
                        force_vec = rel_n0.normalize_or_zero();
                    }
                    // If vel is high or angle is very large, kill vel
                    if rel_v.length() > MAX_SPEED {
                        force_vec += -rel_v.normalize_or_zero();
                    }
                    // If vel angle is slightly off, get perpendicular force
                    // if rel_v.angle_between(rel_r) > 0.1 {
                    //     force_vec += rel_v.cross(rel_r.cross(rel_v)).normalize_or_zero()
                    // }

                    // If too close, force in opposite dir
                    if rel_r.length() < APPROACH_RADIUS  * 0.9 {
                        force_vec += -rel_r.normalize_or_zero();
                    }
                    // If we are outside a tolerance of the sphere radius, force in target dir
                    else if rel_r.length() > APPROACH_RADIUS {
                        force_vec += rel_r.normalize_or_zero();
                    // Otherwise, force in tangent dir
                    } else {
                        let tangent_axis = if rel_r.cross(Vec3::Y).length_squared() > 1e-6 {
                            Vec3::Y
                        } else {
                            Vec3::X
                        };

                        force_vec += tangent_axis.cross(rel_r).normalize_or_zero();
                    }

                    // Draw force
                    gizmos.ray(world_r, force_vec, RED);

                    // Apply force
                    if let Ok(mut node_forces) = rb_forces.p1().get_mut(node) {
                        node_forces.apply_force(force_vec * rel_r.length().clamp(10.0, MAX_FORCE));
                    } else {
                        println!("Faled to apply force for node")
                    };
                }
            }
        }
    }
}
