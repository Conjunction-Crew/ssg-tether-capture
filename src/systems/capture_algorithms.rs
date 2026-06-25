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
            CapturePlanLibrary, CaptureSphereRadius, CompiledCapturePhaseParameters,
            CompiledCapturePlan, CompiledCaptureTransition,
        },
        data_collection::{self, DataCollection},
        orbital_cache::OrbitalCache,
        world_time::WorldTime,
    },
    systems::physics::PHYS_DT,
};

pub fn capture_phase_machine_update(
    mut commands: Commands,
    capture_entities: Query<(Entity, &mut CaptureComponent)>,
    capture_plan_lib: Res<CapturePlanLibrary>,
    mut rb_forces: ParamSet<(Query<RigidBodyQuery>, Query<Forces>)>,
    mut capture_sphere_radius: ResMut<CaptureSphereRadius>,
    orbital_cache: Res<OrbitalCache>,
    mut data_collection: ResMut<DataCollection>,
    world_time: Res<WorldTime>,
    mut log_events: MessageWriter<LogEvent>,
) {
    for (capture_entity, mut capture_component) in capture_entities {
        capture_component.phase_elapsed_time_s += PHYS_DT;

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

        // Execute plan phase machine
        if let Some(plan) = capture_plan_lib
            .compiled_plans
            .get(&capture_component.plan_id)
        {
            if let Some(nodes) = orbital_cache.tethers.get(&plan.tether) {
                let root_capture_radius = capture_sphere_radius.radius;

                // Compute root-relative position/velocity and the root node position
                // first so that the rb_forces.p0() borrow is fully released before
                // calling resolve_root_phase (which needs &mut log_events).
                let (root_rv, root_position): (Option<(f64, f64)>, Option<DVec3>) = nodes
                    .first()
                    .and_then(|&root_node| {
                        rb_forces.p0().get(root_node).ok().map(|root_rb| {
                            let r = capture_entity_position.0 - root_rb.position.0;
                            let v = root_rb.linear_velocity.0 - capture_entity_linvel.0;
                            ((r.length(), v.length()), root_rb.position.0)
                        })
                    })
                    .map(|(rv, pos)| (Some(rv), Some(pos)))
                    .unwrap_or((None, None));

                // Gather all node positions for the straightness metric. The borrow is
                // scoped so it is released before resolve_root_phase below.
                let node_positions: Vec<DVec3> = {
                    let rb_query = rb_forces.p0();
                    nodes
                        .iter()
                        .filter_map(|&n| rb_query.get(n).ok().map(|rb| rb.position.0))
                        .collect()
                };

                // How far the tether deviates from the straight root→RSO line, normalized
                // by that line's length (0 = perfectly straight). Used by the `terminal`
                // phase to decide when the tether is straight enough to capture.
                let straightness =
                    tether_straightness(&node_positions, capture_entity_position.0);

                let shared_phase_parameters = if let Some((r_len, v_len)) = root_rv {
                    resolve_root_phase(
                        &mut capture_component,
                        plan,
                        r_len,
                        v_len,
                        straightness,
                        &mut capture_sphere_radius,
                        &mut log_events,
                    )
                } else {
                    current_phase_parameters(plan, &capture_component.current_phase)
                };

                let up = (capture_entity_rotation * DVec3::X).normalize_or(DVec3::X);
                let straightening = capture_component.current_phase == "terminal";

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

                    let mut force_vec = DVec3::ZERO;
                    let max_velocity = shared_phase_parameters.max_velocity;
                    let max_force = shared_phase_parameters.max_force;
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
                    // If we are outside the sphere radius, force in RSO dir (or slow down)
                    else if rel_r_len > capture_radius {
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

                        if idx != 0 && capture_component.current_phase == "capture" {
                            force_vec -= tangent_axis.cross(rel_r).normalize_or_zero();
                        } else {
                            force_vec += tangent_axis.cross(rel_r).normalize_or_zero();
                        }
                    }

                    // During the terminal phase, bias every node toward the straight
                    // root→RSO line so the tether straightens out before capture.
                    if straightening {
                        if let Some(root_pos) = root_position {
                            let node_pos = capture_entity_position.0 - rel_r;
                            let toward_line =
                                project_onto_line(node_pos, root_pos, capture_entity_position.0)
                                    - node_pos;
                            force_vec += toward_line.normalize_or_zero();
                        }
                    }

                    // Apply force
                    if let Ok(mut node_forces) = rb_forces.p1().get_mut(node) {
                        node_forces.apply_force(force_vec.normalize_or_zero() * max_force);
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

/// Closest point on the segment line through `a`→`b` to `p` (infinite line projection).
fn project_onto_line(p: DVec3, a: DVec3, b: DVec3) -> DVec3 {
    let dir = b - a;
    let len_sq = dir.length_squared();
    if len_sq < 1e-12 {
        return a;
    }
    a + dir * ((p - a).dot(dir) / len_sq)
}

/// Maximum perpendicular deviation of the tether nodes from the straight line between
/// the root node and the RSO, normalized by that line's length. `0.0` means the chain
/// lies perfectly on the line (fully straight). Returns `0.0` for degenerate inputs.
pub(crate) fn tether_straightness(node_positions: &[DVec3], rso_position: DVec3) -> f64 {
    let Some(&root) = node_positions.first() else {
        return 0.0;
    };
    let line_len = (rso_position - root).length();
    if line_len < 1e-6 {
        return 0.0;
    }
    let max_perp = node_positions
        .iter()
        .map(|&p| (p - project_onto_line(p, root, rso_position)).length())
        .fold(0.0_f64, f64::max);
    max_perp / line_len
}

fn current_phase_parameters(
    plan: &CompiledCapturePlan,
    current_phase: &str,
) -> CompiledCapturePhaseParameters {
    plan.phase(current_phase)
        .map(|phase| phase.parameters)
        .unwrap_or_default()
}

#[allow(clippy::too_many_arguments)]
fn resolve_root_phase(
    capture_component: &mut CaptureComponent,
    plan: &CompiledCapturePlan,
    rel_r_length: f64,
    rel_v_length: f64,
    straightness: f64,
    capture_sphere_radius: &mut CaptureSphereRadius,
    log_events: &mut MessageWriter<LogEvent>,
) -> CompiledCapturePhaseParameters {
    let Some(&start_index) = plan.phase_indices.get(&capture_component.current_phase) else {
        return CompiledCapturePhaseParameters::default();
    };

    let mut parameters = CompiledCapturePhaseParameters::default();

    for phase in &plan.phases[start_index..] {
        if phase.id != capture_component.current_phase {
            continue;
        }

        parameters = phase.parameters;

        if let Some(shrink_rate) = phase.parameters.shrink_rate {
            if capture_sphere_radius.radius > 0.1 {
                capture_sphere_radius.radius -= shrink_rate * PHYS_DT;
            }
        }

        for transition in &phase.transitions {
            apply_transition(
                capture_component,
                transition,
                rel_r_length,
                rel_v_length,
                straightness,
                log_events,
            );
        }
    }

    parameters
}

/// Evaluates a transition with AND semantics: a transition fires only when *every*
/// condition it specifies is satisfied. Existing single-condition transitions behave
/// exactly as before; multi-condition transitions (e.g. `terminal`→`capture` requiring
/// both proximity and straightness) now require all conditions together.
fn apply_transition(
    capture_component: &mut CaptureComponent,
    transition: &CompiledCaptureTransition,
    rel_r_length: f64,
    rel_v_length: f64,
    straightness: f64,
    log_events: &mut MessageWriter<LogEvent>,
) {
    // (satisfied, reason) for every condition the transition specifies.
    let mut conditions: Vec<(bool, String)> = Vec::new();

    if let Some(limit) = transition.distance_less_than {
        conditions.push((
            rel_r_length < limit,
            format!("distance {:.1} m < {:.1} m", rel_r_length, limit),
        ));
    }
    if let Some(limit) = transition.distance_greater_than {
        conditions.push((
            rel_r_length > limit,
            format!("distance {:.1} m > {:.1} m", rel_r_length, limit),
        ));
    }
    if let Some(limit) = transition.relative_velocity_less_than {
        conditions.push((
            rel_v_length < limit,
            format!("rel vel {:.2} m/s < {:.2} m/s", rel_v_length, limit),
        ));
    }
    if let Some(limit) = transition.relative_velocity_greater_than {
        conditions.push((
            rel_v_length > limit,
            format!("rel vel {:.2} m/s > {:.2} m/s", rel_v_length, limit),
        ));
    }
    if let Some(limit) = transition.straightness_less_than {
        conditions.push((
            straightness < limit,
            format!("straightness {:.3} < {:.3}", straightness, limit),
        ));
    }

    if !conditions.is_empty() && conditions.iter().all(|(satisfied, _)| *satisfied) {
        let reason = conditions
            .into_iter()
            .map(|(_, reason)| reason)
            .collect::<Vec<_>>()
            .join(", ");
        transition_to(capture_component, &transition.to, reason, log_events);
    }
}

fn transition_to(
    capture_component: &mut CaptureComponent,
    new_phase: &str,
    reason: String,
    log_events: &mut MessageWriter<LogEvent>,
) {
    println!("Transition: {}, Reason: {}", new_phase, reason);
    log_events.write(LogEvent {
        level: LogLevel::Info,
        source: "capture",
        message: format!(
            "Phase: {} → {} ({})",
            capture_component.current_phase, new_phase, reason
        ),
    });
    capture_component.current_phase = String::from(new_phase);
    capture_component.phase_enter_time_s = 0.0;
    capture_component.phase_elapsed_time_s = 0.0;
}
