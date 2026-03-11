use avian3d::{
    math::PI,
    prelude::{Forces, LinearVelocity, Position, RigidBodyForces, RigidBodyQuery, Rotation},
};
use bevy::prelude::*;

use crate::{
    components::capture_components::CaptureComponent,
    resources::{
        capture_plans::{CapturePlanLibrary, RadiusSliderResource},
        orbital_entities::OrbitalEntities,
    },
};

const MAX_VELOCITY: f32 = 5.0;
const MAX_FORCE: f32 = 100.0;
const MIN_NODE_PROXIMITY: f32 = 20.0;

#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct CaptureGizmoConfigGroup;

pub fn capture_state_machine_update(
    capture_entities: Query<(Entity, &mut CaptureComponent)>,
    capture_plan_lib: ResMut<CapturePlanLibrary>,
    orbital_entities: Res<OrbitalEntities>,
    mut rb_forces: ParamSet<(Query<RigidBodyQuery>, Query<Forces>)>,
    mut gizmos: Gizmos<CaptureGizmoConfigGroup>,
    time: Res<Time>,
    approach_radius: Res<RadiusSliderResource>,
) {
    for (capture_entity, mut capture_component) in capture_entities {
        capture_component.state_elapsed_time_s = time.elapsed_secs_f64();

        // Get current position and velocity of capture entity
        let capture_entity_position: Position;
        let capture_entity_linvel: LinearVelocity;
        let capture_entity_rotation: Rotation;
        if let Ok(capture_entity_rb) = rb_forces.p0().get(capture_entity) {
            capture_entity_position = capture_entity_rb.position.clone();
            capture_entity_linvel = capture_entity_rb.linear_velocity.clone();
            capture_entity_rotation = capture_entity_rb.rotation.clone();
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

                    // Get force for node
                    let mut force_vec: Vec3 = Vec3::ZERO;

                    // Max speed and force  for node
                    let mut max_velocity = (rel_r.length() * 0.2).clamp(3.0, MAX_VELOCITY);
                    let mut max_force = MAX_FORCE;
                    let mut capture_radius = approach_radius.radius;

                    // If this node is not root, reduce max velocity, and sphere radius
                    if idx != 0 {
                        max_velocity /= 2.0;
                        max_force /= 4.0;
                        capture_radius *= 0.8;
                    }

                    // If vel is high, kill vel
                    if rel_v.length() > max_velocity {
                        force_vec += -rel_v.normalize_or_zero();
                    }

                    // If too close, force in opposite dir
                    if rel_r.length() < capture_radius * 0.8 {
                        force_vec += -rel_r.normalize_or_zero();
                    }
                    // If we are outside the sphere radius, force in target dir (or slow down)
                    else if rel_r.length() > capture_radius {
                        if rel_v.angle_between(rel_r) > PI / 2.0 {
                            force_vec += -rel_v.normalize_or_zero();
                        }

                        force_vec += rel_r.normalize_or_zero();
                    // Otherwise, force in tangent dir
                    } else {
                        let up = (capture_entity_rotation * Vec3::X).normalize_or(Vec3::X);

                        let tangent_axis = if rel_r.cross(up).length_squared() > 1e-6 {
                            up
                        } else {
                            Vec3::X
                        };

                        if idx != 0 {
                            force_vec -= tangent_axis.cross(rel_r).normalize_or_zero();
                        } else {
                            force_vec += tangent_axis.cross(rel_r).normalize_or_zero();
                        }
                    }

                    // Draw force
                    gizmos.ray(world_r, force_vec, Srgba::new(1.0, 0.0, 0.0, 0.5));

                    // Draw rel_v
                    gizmos.ray(world_r, rel_v, Srgba::new(0.0, 1.0, 0.0, 0.5));

                    // Draw outer sphere
                    gizmos.sphere(
                        Isometry3d::new(
                            capture_entity_position.0.clone(),
                            capture_entity_rotation.0.clone(),
                        ),
                        approach_radius.radius,
                        Srgba::new(1.0, 0.5, 0.0, 0.5),
                    );

                    // Draw inner sphere
                    gizmos.sphere(
                        Isometry3d::new(
                            capture_entity_position.0.clone(),
                            capture_entity_rotation.0.clone(),
                        ),
                        approach_radius.radius * 0.8,
                        Srgba::new(0.0, 0.8, 0.4, 0.5),
                    );

                    // Apply force
                    if let Ok(mut node_forces) = rb_forces.p1().get_mut(node) {
                        node_forces.apply_force(force_vec * rel_r.length().clamp(1.0, max_force));
                    } else {
                        println!("Faled to apply force for node");
                    };
                }
            }
        }
    }
}
