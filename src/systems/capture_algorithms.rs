use avian3d::{
    math::PI,
    prelude::{
        Forces, LinearVelocity, Physics, Position, RigidBodyForces, RigidBodyQuery, Rotation,
    },
};
use bevy::prelude::*;

use crate::{
    components::capture_components::CaptureComponent,
    resources::{
        capture_plans::{CapturePlanLibrary, CaptureSphereRadius},
        orbital_entities::OrbitalEntities,
    },
    systems::physics::PHYS_DT,
};

#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct CaptureGizmoConfigGroup;

pub fn capture_state_machine_update(
    capture_entities: Query<(Entity, &mut CaptureComponent)>,
    capture_plan_lib: ResMut<CapturePlanLibrary>,
    orbital_entities: Res<OrbitalEntities>,
    mut rb_forces: ParamSet<(Query<RigidBodyQuery>, Query<Forces>)>,
    mut gizmos: Gizmos<CaptureGizmoConfigGroup>,
    mut capture_sphere_radius: ResMut<CaptureSphereRadius>,
) {
    for (capture_entity, mut capture_component) in capture_entities {
        capture_component.state_elapsed_time_s += PHYS_DT;

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
                    let mut max_velocity = 0.0;
                    let mut max_force = 0.0;
                    let mut capture_radius = capture_sphere_radius.radius;

                    // Check if state should transition
                    for state in &plan.states {
                        if state.id == capture_component.current_state {
                            // Get parameters for this state
                            if let Some(parameters) = &state.parameters {
                                if let Some(parsed_max_velocity) = parameters.get("max_velocity") {
                                    if let Some(val) = parsed_max_velocity.as_f64() {
                                        max_velocity = val as f32;
                                    }
                                }
                                if let Some(parsed_max_force) = parameters.get("max_force") {
                                    if let Some(val) = parsed_max_force.as_f64() {
                                        max_force = val as f32;
                                    }
                                }

                                if idx == 0 {
                                    if let Some(parsed_shrink_rate) = parameters.get("shrink_rate")
                                    {
                                        if let Some(val) = parsed_shrink_rate.as_f64() {
                                            if capture_sphere_radius.radius > 0.1 {
                                                capture_sphere_radius.radius -=
                                                    (val * PHYS_DT) as f32;
                                            }
                                        }
                                    }

                                    // Loop through transitions
                                    if let Some(transitions) = &state.transitions {
                                        for transition in transitions {
                                            if let Some(to) = transition.get("to") {
                                                // Check distance transition conditions
                                                if let Some(distance) = transition.get("distance") {
                                                    if let Some(less_than) =
                                                        distance.get("less_than")
                                                    {
                                                        if let Some(val) = less_than.as_f64() {
                                                            if rel_r.length() < val as f32 {
                                                                // Transition to 'to'
                                                                if let Some(new_state) = to.as_str()
                                                                {
                                                                    println!(
                                                                        "Transition: {}, Reason: distance {} < {}",
                                                                        new_state,
                                                                        rel_r.length(),
                                                                        val
                                                                    );
                                                                    capture_component
                                                                        .current_state =
                                                                        String::from(new_state);
                                                                    capture_component
                                                                        .state_enter_time_s = 0.0;
                                                                    capture_component
                                                                        .state_elapsed_time_s = 0.0;
                                                                }
                                                            }
                                                        }
                                                    }
                                                    if let Some(greater_than) =
                                                        distance.get("greater_than")
                                                    {
                                                        if let Some(val) = greater_than.as_f64() {
                                                            if rel_r.length() > val as f32 {
                                                                // Transition to 'to'
                                                                if let Some(new_state) = to.as_str()
                                                                {
                                                                    println!(
                                                                        "Transition: {}, Reason: distance {} > {}",
                                                                        new_state,
                                                                        rel_r.length(),
                                                                        val
                                                                    );
                                                                    capture_component
                                                                        .current_state =
                                                                        String::from(new_state);
                                                                    capture_component
                                                                        .state_enter_time_s = 0.0;
                                                                    capture_component
                                                                        .state_elapsed_time_s = 0.0;
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                                // Check velocity transition conditions
                                                if let Some(distance) =
                                                    transition.get("relative_velocity")
                                                {
                                                    if let Some(less_than) =
                                                        distance.get("less_than")
                                                    {
                                                        if let Some(val) = less_than.as_f64() {
                                                            if rel_v.length() < val as f32 {
                                                                // Transition to 'to'
                                                                if let Some(new_state) = to.as_str()
                                                                {
                                                                    println!(
                                                                        "Transition: {}, Reason: velocity {} < {}",
                                                                        new_state,
                                                                        rel_v.length(),
                                                                        val
                                                                    );
                                                                    capture_component
                                                                        .current_state =
                                                                        String::from(new_state);
                                                                    capture_component
                                                                        .state_enter_time_s = 0.0;
                                                                    capture_component
                                                                        .state_elapsed_time_s = 0.0;
                                                                }
                                                            }
                                                        }
                                                    }
                                                    if let Some(greater_than) =
                                                        distance.get("greater_than")
                                                    {
                                                        if let Some(val) = greater_than.as_f64() {
                                                            if rel_v.length() > val as f32 {
                                                                // Transition to 'to'
                                                                if let Some(new_state) = to.as_str()
                                                                {
                                                                    println!(
                                                                        "Transition: {}, Reason: velocity {} > {}",
                                                                        new_state,
                                                                        rel_v.length(),
                                                                        val
                                                                    );
                                                                    capture_component
                                                                        .current_state =
                                                                        String::from(new_state);
                                                                    capture_component
                                                                        .state_enter_time_s = 0.0;
                                                                    capture_component
                                                                        .state_elapsed_time_s = 0.0;
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // If this node is not root, reduce max velocity, and sphere radius
                    if idx != 0 {
                        max_velocity *= 0.9;
                        max_force /= 2.0;
                        capture_radius += 1.0;
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

                        if idx != 0 && capture_component.current_state == "capture" {
                            force_vec -= tangent_axis.cross(rel_r).normalize_or_zero();
                        } else {
                            force_vec += tangent_axis.cross(rel_r).normalize_or_zero();
                        }
                    }

                    // Draw force
                    gizmos.ray(world_r, force_vec, Srgba::new(1.0, 0.0, 0.0, 0.2));

                    // Draw rel_v
                    gizmos.ray(world_r, rel_v, Srgba::new(0.0, 1.0, 0.0, 0.2));

                    // Draw inner sphere
                    gizmos.sphere(
                        Isometry3d::new(
                            capture_entity_position.0.clone(),
                            capture_entity_rotation.0.clone(),
                        ),
                        capture_sphere_radius.radius,
                        Srgba::new(1.0, 0.5, 0.0, 0.2),
                    );

                    // Draw outer sphere
                    gizmos.sphere(
                        Isometry3d::new(
                            capture_entity_position.0.clone(),
                            capture_entity_rotation.0.clone(),
                        ),
                        capture_sphere_radius.radius + 1.0,
                        Srgba::new(0.0, 0.8, 0.4, 0.2),
                    );

                    // Apply force
                    if let Ok(mut node_forces) = rb_forces.p1().get_mut(node) {
                        node_forces.apply_force(force_vec.normalize() * max_force);
                    } else {
                        println!("Faled to apply force for node");
                    };
                }
            }
        }
    }
}
