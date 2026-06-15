use avian3d::{
    math::PI,
    prelude::{Forces, LinearVelocity, Position, RigidBodyQuery, Rotation, WriteRigidBodyForces},
};
use bevy::{math::DVec3, prelude::*, state::commands};

use crate::{
    components::capture_components::CaptureComponent,
    resources::{
        capture_log::{LogEvent, LogLevel},
        capture_plans::{
            CapturePlanLibrary, CaptureSphereRadius, CompiledCapturePlan,
            CompiledCaptureStateParameters, CompiledCaptureTransition,
        },
        data_collection::{self, DataCollection},
        orbital_cache::OrbitalCache,
        propagation::ActivePropagation,
        world_time::WorldTime,
    },
    systems::physics::PHYS_DT,
};

pub fn capture_state_machine_update(
    mut commands: Commands,
    capture_entities: Query<(Entity, &mut CaptureComponent)>,
    capture_plan_lib: Res<CapturePlanLibrary>,
    mut rb_forces: ParamSet<(Query<RigidBodyQuery>, Query<Forces>)>,
    mut capture_sphere_radius: ResMut<CaptureSphereRadius>,
    orbital_cache: Res<OrbitalCache>,
    mut data_collection: ResMut<DataCollection>,
    world_time: Res<WorldTime>,
    active_propagation: Res<ActivePropagation>,
    mut log_events: MessageWriter<LogEvent>,
) {
    // Propagation sims don't run the capture state machine.
    if active_propagation.enabled {
        return;
    }

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

                // Compute root-relative position/velocity first so that the
                // rb_forces.p0() borrow is fully released before calling
                // resolve_root_state (which needs &mut log_events).
                let root_rv: Option<(f64, f64)> = nodes.first().and_then(|&root_node| {
                    rb_forces.p0().get(root_node).ok().map(|root_rb| {
                        let r = capture_entity_position.0 - root_rb.position.0;
                        let v = root_rb.linear_velocity.0 - capture_entity_linvel.0;
                        (r.length(), v.length())
                    })
                });

                let tether_straightness_deg: f64 = {
                    let rb_query = rb_forces.p0();
                    let positions: Vec<DVec3> = nodes
                        .iter()
                        .filter_map(|&n| rb_query.get(n).ok().map(|rb| rb.position.0))
                        .collect();
                    tether_max_angular_deviation(&positions)
                };

                let shared_state_parameters = if let Some((r_len, v_len)) = root_rv {
                    resolve_root_state(
                        &mut capture_component,
                        plan,
                        r_len,
                        v_len,
                        tether_straightness_deg,
                        &mut capture_sphere_radius,
                        &mut log_events,
                    )
                } else {
                    current_state_parameters(plan, &capture_component.current_state)
                };

                let up = (capture_entity_rotation * DVec3::X).normalize_or(DVec3::X);

                // Pre-read root/tail positions needed for tether_tension force direction.
                let tension_endpoints: Option<(DVec3, DVec3, DVec3, DVec3)> =
                    if shared_state_parameters.is_tether_tension {
                        let rb = rb_forces.p0();
                        let root_data = nodes
                            .first()
                            .and_then(|&n| rb.get(n).ok().map(|rb| (rb.position.0, rb.linear_velocity.0)));
                        let tail_data = nodes
                            .last()
                            .and_then(|&n| rb.get(n).ok().map(|rb| (rb.position.0, rb.linear_velocity.0)));
                        match (root_data, tail_data) {
                            (Some((rp, rv)), Some((tp, tv))) => Some((rp, rv, tp, tv)),
                            _ => None,
                        }
                    } else {
                        None
                    };

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

                    // Insert current pos and vel into tracking vectors
                    if idx == 0 {
                        if let Some(pos_collect) = data_collection.position.get_mut(&capture_entity)
                        {
                            pos_collect.push((
                                world_time.epoch.unix_timestamp()
                                    - world_time.start_epoch.unix_timestamp(),
                                rel_r_len,
                            ));
                        };
                        if let Some(vel_collect) = data_collection.velocity.get_mut(&capture_entity)
                        {
                            vel_collect.push((
                                world_time.epoch.unix_timestamp()
                                    - world_time.start_epoch.unix_timestamp(),
                                rel_v_len,
                            ));
                        };
                    }

                    let max_velocity = shared_state_parameters.max_velocity;
                    let max_force = shared_state_parameters.max_force;

                    let force_vec: DVec3 =
                        if shared_state_parameters.is_tether_tension {
                            let is_root = idx == 0;
                            let is_tail = idx == nodes.len() - 1;

                            // Interior nodes: no force applied in tether_tension state.
                            if !is_root && !is_tail {
                                continue;
                            }

                            if let Some((root_pos, root_vel, tail_pos, tail_vel)) = tension_endpoints {
                                let (outward_axis, endpoint_vel) = if is_root {
                                    (
                                        (root_pos - tail_pos).normalize_or(DVec3::X),
                                        root_vel - tail_vel,
                                    )
                                } else {
                                    (
                                        (tail_pos - root_pos).normalize_or(DVec3::X),
                                        tail_vel - root_vel,
                                    )
                                };

                                // Damp if moving too fast; otherwise apply outward tension.
                                let speed = endpoint_vel.dot(outward_axis);
                                let dir = if speed.abs() > max_velocity {
                                    -endpoint_vel.normalize_or_zero()
                                } else {
                                    outward_axis
                                };

                                // Log tension warning on the root node (once per evaluation).
                                if is_root {
                                    if let Some(max_t) = shared_state_parameters.max_tension_n {
                                        if max_force > max_t {
                                            log_events.write(LogEvent {
                                                level: LogLevel::Warn,
                                                source: "capture",
                                                message: format!(
                                                    "Applied tension {:.1} N exceeds max_tension_n {:.1} N",
                                                    max_force, max_t
                                                ),
                                            });
                                        }
                                    }
                                }

                                dir
                            } else {
                                DVec3::ZERO
                            }
                        } else {
                            let capture_radius = if idx == 0 {
                                root_capture_radius
                            } else {
                                capture_sphere_radius.radius + 1.0
                            };

                            let mut v = DVec3::ZERO;

                            // If vel is high, kill vel
                            if rel_v_len > max_velocity {
                                v += -rel_v.normalize_or_zero() * 2.0;
                            }
                            // If too close, force in opposite dir
                            if rel_r_len < capture_radius * 0.8 {
                                v += -rel_r.normalize_or_zero();
                            }
                            // If we are outside the sphere radius, force in target dir (or slow down)
                            else if rel_r_len > capture_radius {
                                if rel_v.angle_between(rel_r) > PI / 2.0 {
                                    v += -rel_v.normalize_or_zero();
                                }
                                v += rel_r.normalize_or_zero();
                            // Otherwise, force in tangent dir
                            } else {
                                let tangent_axis = if rel_r.cross(up).length_squared() > 1e-6 {
                                    up
                                } else {
                                    DVec3::X
                                };

                                if idx != 0 && capture_component.current_state == "capture" {
                                    v -= tangent_axis.cross(rel_r).normalize_or_zero();
                                } else {
                                    v += tangent_axis.cross(rel_r).normalize_or_zero();
                                }
                            }

                            v
                        };

                    // Apply force
                    if let Ok(mut node_forces) = rb_forces.p1().get_mut(node) {
                        node_forces.apply_force(force_vec.normalize() * max_force);
                    } else {
                        println!("Faled to apply force for node");
                        log_events.write(LogEvent {
                            level: LogLevel::Warn,
                            source: "capture",
                            message: format!(
                                "Failed to apply force to tether node {} (RigidBody unavailable)",
                                idx
                            ),
                        });
                    };
                }
            } else {
                warn!(
                    "Capture plan '{}': tether '{}' not found in orbital_cache.tethers (available: {:?}). \
                     Ensure the plan's tether name matches a registered tether.",
                    capture_component.plan_id,
                    plan.tether,
                    orbital_cache.tethers.keys().collect::<Vec<_>>()
                );
                log_events.write(LogEvent {
                    level: LogLevel::Error,
                    source: "capture",
                    message: format!(
                        "Tether '{}' not found for plan '{}'",
                        plan.tether, capture_component.plan_id
                    ),
                });
            }
        } else {
            warn!(
                "Capture plan '{}' not found in compiled_plans — aborting capture.",
                capture_component.plan_id
            );
            commands.entity(capture_entity).remove::<CaptureComponent>();
            log_events.write(LogEvent {
                level: LogLevel::Error,
                source: "capture",
                message: format!(
                    "Compiled plan '{}' not found — capture aborted",
                    capture_component.plan_id
                ),
            });
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
    tether_straightness_deg: f64,
    capture_sphere_radius: &mut CaptureSphereRadius,
    log_events: &mut MessageWriter<LogEvent>,
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
            apply_transition(
                capture_component,
                transition,
                rel_r_length,
                rel_v_length,
                tether_straightness_deg,
                log_events,
            );
        }
    }

    parameters
}

fn apply_transition(
    capture_component: &mut CaptureComponent,
    transition: &CompiledCaptureTransition,
    rel_r_length: f64,
    rel_v_length: f64,
    tether_straightness_deg: f64,
    log_events: &mut MessageWriter<LogEvent>,
) {
    if let Some(limit) = transition.distance_less_than {
        if rel_r_length < limit {
            transition_to(
                capture_component,
                &transition.to,
                format!("distance {:.1} m < {:.1} m", rel_r_length, limit),
                log_events,
            );
        }
    }

    if let Some(limit) = transition.distance_greater_than {
        if rel_r_length > limit {
            transition_to(
                capture_component,
                &transition.to,
                format!("distance {:.1} m > {:.1} m", rel_r_length, limit),
                log_events,
            );
        }
    }

    if let Some(limit) = transition.relative_velocity_less_than {
        if rel_v_length < limit {
            transition_to(
                capture_component,
                &transition.to,
                format!("rel vel {:.2} m/s < {:.2} m/s", rel_v_length, limit),
                log_events,
            );
        }
    }

    if let Some(limit) = transition.relative_velocity_greater_than {
        if rel_v_length > limit {
            transition_to(
                capture_component,
                &transition.to,
                format!("rel vel {:.2} m/s > {:.2} m/s", rel_v_length, limit),
                log_events,
            );
        }
    }

    if let Some(limit) = transition.tether_straightness_less_than {
        if tether_straightness_deg < limit {
            transition_to(
                capture_component,
                &transition.to,
                format!(
                    "tether deviation {:.1}° < {:.1}°",
                    tether_straightness_deg, limit
                ),
                log_events,
            );
        }
    }
}

/// Returns the maximum angular deviation (degrees) of any interior tether node
/// from the straight line between the root (index 0) and tail (last index).
/// Returns 0.0 when there are fewer than 3 nodes or the endpoints coincide.
fn tether_max_angular_deviation(node_positions: &[DVec3]) -> f64 {
    let n = node_positions.len();
    if n < 3 {
        return 0.0;
    }
    let root = node_positions[0];
    let tail = node_positions[n - 1];
    let root_to_tail = tail - root;
    let length = root_to_tail.length();
    if length < 1e-6 {
        return 0.0;
    }
    let axis = root_to_tail / length;

    node_positions[1..n - 1].iter().fold(0.0_f64, |max_deg, &pos| {
        let offset = pos - root;
        let along = offset.dot(axis);
        let lateral = (offset - along * axis).length();
        let angle_deg = lateral.atan2(along).to_degrees();
        max_deg.max(angle_deg)
    })
}

fn transition_to(
    capture_component: &mut CaptureComponent,
    new_state: &str,
    reason: String,
    log_events: &mut MessageWriter<LogEvent>,
) {
    println!("Transition: {}, Reason: {}", new_state, reason);
    log_events.write(LogEvent {
        level: LogLevel::Info,
        source: "capture",
        message: format!(
            "State: {} → {} ({})",
            capture_component.current_state, new_state, reason
        ),
    });
    capture_component.current_state = String::from(new_state);
    capture_component.state_enter_time_s = 0.0;
    capture_component.state_elapsed_time_s = 0.0;
}
