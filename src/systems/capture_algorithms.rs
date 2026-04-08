use avian3d::{
    math::PI,
    prelude::{Forces, LinearVelocity, Position, RigidBodyQuery, Rotation, WriteRigidBodyForces},
};
use bevy::{
    math::{DMat3, DVec3},
    prelude::*,
};
use brahe::GM_EARTH;
use nalgebra::{DMatrix, Matrix3, Matrix6, Vector3, Vector6};

use crate::{
    components::{capture_components::CaptureComponent, orbit_camera::CameraTarget},
    resources::{
        capture_plans::{
            CapturePlanLibrary, CaptureSphereRadius, CompiledCapturePlan,
            CompiledCaptureStateParameters, CompiledCaptureTransition,
        },
        orbital_cache::{self, OrbitalCache},
    },
    systems::physics::PHYS_DT,
};

pub fn capture_state_machine_update(
    capture_entities: Query<(Entity, &mut CaptureComponent)>,
    chaser_entity: Single<Entity, With<CameraTarget>>,
    capture_plan_lib: Res<CapturePlanLibrary>,
    mut rb_forces: ParamSet<(Query<RigidBodyQuery>, Query<Forces>)>,
    mut capture_sphere_radius: ResMut<CaptureSphereRadius>,
    orbital_cache: Res<OrbitalCache>,
) {
    let chaser_e = chaser_entity.into_inner();
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
        if let Some(plan) = capture_plan_lib
            .compiled_plans
            .get(&capture_component.plan_id)
        {
            if let Some(nodes) = orbital_cache.tethers.get(&plan.tether) {
                let root_capture_radius = capture_sphere_radius.radius;

                let shared_state_parameters = nodes
                    .first()
                    .and_then(|&root_node| {
                        rb_forces.p0().get(root_node).ok().map(|root_rb| {
                            let root_rel_r = capture_entity_position.0 - root_rb.position.0;
                            let root_rel_v = root_rb.linear_velocity.0 - capture_entity_linvel.0;
                            resolve_root_state(
                                &mut capture_component,
                                plan,
                                root_rel_r.length(),
                                root_rel_v.length(),
                                &mut capture_sphere_radius,
                            )
                        })
                    })
                    .unwrap_or_else(|| {
                        current_state_parameters(plan, &capture_component.current_state)
                    });

                let up = (capture_entity_rotation * DVec3::X).normalize_or(DVec3::X);

                for (idx, &node) in nodes.iter().enumerate() {
                    let (rel_r, rel_v) = {
                        let rb_query = rb_forces.p0();
                        let Ok(rb) = rb_query.get(node) else {
                            continue;
                        };

                        (
                            capture_entity_position.0 - rb.position.0,
                            rb.linear_velocity.0 - capture_entity_linvel.0,
                        )
                    };
                    let rel_r_len = rel_r.length();
                    let rel_v_len = rel_v.length();

                    let mut force_vec = DVec3::ZERO;
                    let max_velocity = shared_state_parameters.max_velocity;
                    let max_force = shared_state_parameters.max_force;
                    let capture_radius = if idx == 0 {
                        root_capture_radius
                    } else {
                        capture_sphere_radius.radius + 1.0
                    };

                    // If vel is high, kill vel
                    if rel_v_len > max_velocity {
                        force_vec += -rel_v.normalize_or_zero() * 2.0;
                    }
                    // If too close, force in opposite dir
                    if rel_r_len < capture_radius * 0.8 {
                        force_vec += -rel_r.normalize_or_zero();
                    }
                    // If we are outside the sphere radius, force in target dir (or slow down)
                    else if rel_r_len > capture_radius {
                        // if capture_component.current_state == "rendezvous" {
                        //     if let (Some(target_rv), Some(chaser_rv)) = (
                        //         orbital_cache.eci_states.get(&capture_entity),
                        //         orbital_cache.eci_states.get(&chaser_e),
                        //     ) {
                        //         if let Some(cw_result) = clohessy_wiltshire_solver(
                        //             *chaser_rv, *target_rv, PHYS_DT,
                        //         ) {
                        //             force_vec += cw_result.0.normalize_or_zero();
                        //         }
                        //     }
                        // }
                        if rel_v.angle_between(rel_r) > PI / 2.0 {
                            force_vec += -rel_v.normalize_or_zero();
                        }

                        force_vec += rel_r.normalize_or_zero();
                    // Otherwise, force in tangent dir
                    } else {
                        let tangent_axis = if rel_r.cross(up).length_squared() > 1e-6 {
                            up
                        } else {
                            DVec3::X
                        };

                        if idx != 0 && capture_component.current_state == "capture" {
                            force_vec -= tangent_axis.cross(rel_r).normalize_or_zero();
                        } else {
                            force_vec += tangent_axis.cross(rel_r).normalize_or_zero();
                        }
                    }

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

fn current_state_parameters(
    plan: &CompiledCapturePlan,
    current_state: &str,
) -> CompiledCaptureStateParameters {
    plan.state(current_state)
        .map(|state| state.parameters)
        .unwrap_or_default()
}

fn resolve_root_state(
    capture_component: &mut CaptureComponent,
    plan: &CompiledCapturePlan,
    rel_r_length: f64,
    rel_v_length: f64,
    capture_sphere_radius: &mut CaptureSphereRadius,
) -> CompiledCaptureStateParameters {
    let Some(&start_index) = plan.state_indices.get(&capture_component.current_state) else {
        return CompiledCaptureStateParameters::default();
    };

    let mut parameters = CompiledCaptureStateParameters::default();

    for state in &plan.states[start_index..] {
        if state.id != capture_component.current_state {
            continue;
        }

        parameters = state.parameters;

        if let Some(shrink_rate) = state.parameters.shrink_rate {
            if capture_sphere_radius.radius > 0.1 {
                capture_sphere_radius.radius -= shrink_rate * PHYS_DT;
            }
        }

        for transition in &state.transitions {
            apply_transition(capture_component, transition, rel_r_length, rel_v_length);
        }
    }

    parameters
}

fn apply_transition(
    capture_component: &mut CaptureComponent,
    transition: &CompiledCaptureTransition,
    rel_r_length: f64,
    rel_v_length: f64,
) {
    if let Some(limit) = transition.distance_less_than {
        if rel_r_length < limit {
            transition_to(
                capture_component,
                &transition.to,
                format!("distance {} < {}", rel_r_length, limit),
            );
        }
    }

    if let Some(limit) = transition.distance_greater_than {
        if rel_r_length > limit {
            transition_to(
                capture_component,
                &transition.to,
                format!("distance {} > {}", rel_r_length, limit),
            );
        }
    }

    if let Some(limit) = transition.relative_velocity_less_than {
        if rel_v_length < limit {
            transition_to(
                capture_component,
                &transition.to,
                format!("velocity {} < {}", rel_v_length, limit),
            );
        }
    }

    if let Some(limit) = transition.relative_velocity_greater_than {
        if rel_v_length > limit {
            transition_to(
                capture_component,
                &transition.to,
                format!("velocity {} > {}", rel_v_length, limit),
            );
        }
    }
}

fn transition_to(capture_component: &mut CaptureComponent, new_state: &str, reason: String) {
    println!("Transition: {}, Reason: {}", new_state, reason);
    capture_component.current_state = String::from(new_state);
    capture_component.state_enter_time_s = 0.0;
    capture_component.state_elapsed_time_s = 0.0;
}

// Derived from: 
// https://webthesis.biblio.polito.it/27813/1/tesi.pdf
// https://vtechworks.lib.vt.edu/server/api/core/bitstreams/cb2265b0-34c3-44e1-8c99-d21aeae1e95c/content
fn clohessy_wiltshire_solver(
    chaser_rv: Vector6<f64>,
    target_rv: Vector6<f64>,
    ts: f64,
) -> Option<(DVec3, DVec3)> {
    // Step 1: Build target LVLH frame needed for HCW targeting
    let target_r = Vector3::new(target_rv[0], target_rv[1], target_rv[2]);
    let target_v = Vector3::new(target_rv[3], target_rv[4], target_rv[5]);
    if target_r.norm() == 0.0 {
        return None;
    }

    let target_radial = target_r / target_r.norm();
    let target_cross = target_r.cross(&target_v);
    if target_cross.norm() == 0.0 {
        return None;
    }

    let target_normal = target_cross / target_cross.norm();
    let target_along_track = target_normal.cross(&target_radial);

    // Step 2: Assemble rotation matrix
    let eci_to_ric = Matrix3::new(
        target_radial.x,
        target_radial.y,
        target_radial.z,
        target_along_track.x,
        target_along_track.y,
        target_along_track.z,
        target_normal.x,
        target_normal.y,
        target_normal.z,
    );

    // Step 3: Calculate initial relative position/velocity in LVLH frame
    let chaser_r = Vector3::new(chaser_rv[0], chaser_rv[1], chaser_rv[2]);
    let chaser_v = Vector3::new(chaser_rv[3], chaser_rv[4], chaser_rv[5]);

    let relative_r_eci = chaser_r - target_r;
    let relative_r_ric = eci_to_ric * relative_r_eci;

    let relative_v_eci = chaser_v - target_v;

    // Step 4: Calculate initial relative velocity in LVLH
    let n = (GM_EARTH / target_r.norm().powi(3)).sqrt();
    let omega = Vector3::new(0.0, 0.0, n);
    let relative_v_ric = eci_to_ric * relative_v_eci + relative_r_ric.cross(&omega);

    // HCW initial state pieces
    let r0 = relative_r_ric;
    let v0 = relative_v_ric;

    // Step 5: HCW state transition matrix blocks
    let nt = n * ts;
    let s = nt.sin();
    let c = nt.cos();

    let phi_rr = Matrix3::new(
        4.0 - 3.0 * c, 0.0, 0.0,
        6.0 * (s - nt), 1.0, 0.0,
        0.0, 0.0, c,
    );

    let phi_rv = Matrix3::new(
        s / n,                   2.0 * (1.0 - c) / n, 0.0,
        2.0 * (c - 1.0) / n,    (4.0 * s - 3.0 * nt) / n, 0.0,
        0.0,                     0.0,                 s / n,
    );

    // Rendezvous target: arrive at origin of LVLH frame
    let desired_final_r_ric = Vector3::zeros();

    // Solve one-impulse HCW intercept:
    // r_f = Phi_rr * r0 + Phi_rv * v_plus
    let rhs = desired_final_r_ric - phi_rr * r0;
    let phi_rv_inv = phi_rv.try_inverse()?;
    let v_plus = phi_rv_inv * rhs;

    let delta_v_rel = v_plus - v0;
    let delta_v_eci = eci_to_ric.transpose() * delta_v_rel;

    Some((
        DVec3::new(delta_v_rel.x, delta_v_rel.y, delta_v_rel.z),
        DVec3::new(delta_v_eci.x, delta_v_eci.y, delta_v_eci.z),
    ))
}
