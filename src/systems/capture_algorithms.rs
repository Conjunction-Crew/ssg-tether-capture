use avian3d::{
    math::PI,
    prelude::{Forces, LinearVelocity, Position, RigidBodyQuery, Rotation, WriteRigidBodyForces},
};
use bevy::{math::DVec3, prelude::*, state::commands};

use crate::{
    components::{
        capture_components::{CaptureAxis, CaptureComponent},
        orbit::TetherNode,
    },
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
    capture_axes: Query<&CaptureAxis>,
    // Index of interior tether nodes (no Position access, to avoid conflicting with the
    // rb_forces ParamSet); used to measure straightness of the real rope.
    tether_node_index: Query<(Entity, &TetherNode)>,
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

                // Position of the free (tail) end, used to tension the two ends apart.
                let tail_position: Option<DVec3> = if nodes.len() > 1 {
                    nodes
                        .last()
                        .and_then(|&tail| rb_forces.p0().get(tail).ok().map(|rb| rb.position.0))
                } else {
                    None
                };

                // The interior rope segments are joint-driven and not in `nodes`; gather
                // them so straightness reflects the actual rope rather than just its ends.
                let root_node = nodes.first().copied();
                let interior_entities: Vec<Entity> = root_node
                    .map(|root| {
                        tether_node_index
                            .iter()
                            .filter(|(_, tn)| tn.root == root)
                            .map(|(e, _)| e)
                            .collect()
                    })
                    .unwrap_or_default();

                // Gather all node positions (root, tail, then interior segments) for the
                // straightness and closest-approach metrics. The borrow is scoped so it is
                // released before resolve_root_phase below.
                let node_positions: Vec<DVec3> = {
                    let rb_query = rb_forces.p0();
                    nodes
                        .iter()
                        .chain(interior_entities.iter())
                        .filter_map(|&n| rb_query.get(n).ok().map(|rb| rb.position.0))
                        .collect()
                };

                // How far the tether deviates from its own straight root→tail line,
                // normalized by that line's length (0 = perfectly straight). Independent of
                // the tether's orientation relative to the RSO, so a tangent-but-straight
                // tether still reads as straight.
                let straightness = match (root_position, tail_position) {
                    (Some(root_pos), Some(tail_pos)) => {
                        tether_straightness(&node_positions, root_pos, tail_pos)
                    }
                    _ => 0.0,
                };

                // Closest approach of any part of the tether to the RSO — the meaningful
                // "distance" for capture readiness now that the tether closes in tangentially
                // (its root stays ~half a tether-length away from the RSO).
                let closest_approach = node_positions
                    .iter()
                    .map(|p| (*p - capture_entity_position.0).length())
                    .fold(f64::INFINITY, f64::min);

                let shared_phase_parameters = if let Some((_r_len, v_len)) = root_rv {
                    resolve_root_phase(
                        &mut capture_component,
                        plan,
                        closest_approach,
                        v_len,
                        straightness,
                        &mut capture_sphere_radius,
                        &mut log_events,
                    )
                } else {
                    current_phase_parameters(plan, &capture_component.current_phase)
                };

                // Circulate the tether around the RSO's defined capture axis (body-frame
                // axis rotated into world space, matching capture_axis_gizmos) so the
                // `capture` phase wraps the tether in the plane perpendicular to it.
                let axis_body = capture_axes
                    .get(capture_entity)
                    .map(|ca| ca.axis)
                    .unwrap_or(DVec3::Z);
                let up = (capture_entity_rotation * axis_body).normalize_or(DVec3::Z);
                let straightening = capture_component.current_phase == "terminal";

                // Natural (unstretched) rope length, used by the terminal phase to know how
                // far to tension the two ends apart before the rope is taut.
                let natural_len = capture_plan_lib
                    .plans
                    .get(&capture_component.plan_id)
                    .and_then(|p| p.device.as_ref())
                    .map(|d| d.tether_length)
                    .filter(|l| *l > 0.0)
                    .unwrap_or(20.0);

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

                    let max_velocity = shared_phase_parameters.max_velocity;
                    let max_force = shared_phase_parameters.max_force;
                    let capture_radius = if idx == 0 {
                        root_capture_radius
                    } else {
                        capture_sphere_radius.radius + 1.0
                    };

                    let applied_force = if straightening {
                        // Terminal phase: drive this end (root or tail) so the straight rope
                        // lies in the capture plane, oriented tangent to the capture orbit,
                        // then translates inward toward the RSO. Forces are only applied at
                        // the two ends; the interior is joint-driven.
                        let node_pos = capture_entity_position.0 - rel_r;
                        let other_pos = if idx == 0 {
                            tail_position
                        } else {
                            root_position
                        };
                        terminal_end_force(
                            node_pos,
                            capture_entity_position.0,
                            up,
                            rel_v,
                            other_pos,
                            idx == 0,
                            natural_len,
                            straightness,
                            max_velocity,
                            max_force,
                        )
                    } else {
                        let mut force_vec = DVec3::ZERO;
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
                            // When the radial direction is (anti)parallel to the capture
                            // axis the cross product degenerates; fall back to any vector
                            // perpendicular to the axis so circulation stays around it.
                            let tangent_axis = if rel_r.cross(up).length_squared() > 1e-6 {
                                up
                            } else {
                                up.any_orthonormal_vector()
                            };

                            if idx != 0 && capture_component.current_phase == "capture" {
                                force_vec -= tangent_axis.cross(rel_r).normalize_or_zero();
                            } else {
                                force_vec += tangent_axis.cross(rel_r).normalize_or_zero();
                            }
                        }

                        force_vec.normalize_or_zero() * max_force
                    };

                    // Apply force
                    if let Ok(mut node_forces) = rb_forces.p1().get_mut(node) {
                        node_forces.apply_force(applied_force);
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

                // Damp the transverse ("jump-rope") oscillation of the interior rope nodes
                // while straightening. The end servo brakes the two ends, but the interior
                // segments are only joint-coupled and ring as a standing wave. For each
                // interior node we subtract the velocity a *rigid* straight rope would have
                // at that point — interpolated between the two ends along the line — and damp
                // only the remaining component perpendicular to the line. Subtracting the
                // interpolated (not merely averaged) end velocity removes the rope's
                // translation AND its rotation toward tangent, so the damping bleeds off the
                // oscillation without fighting either the closing or the orientation change.
                if straightening {
                    if let (Some(root_pos), Some(tail_pos)) = (root_position, tail_position) {
                        let line_vec = tail_pos - root_pos;
                        let line_len = line_vec.length();
                        let line_dir = line_vec.normalize_or_zero();
                        if line_len > 1e-3 {
                            const TRANSVERSE_DAMP: f64 = 2.0;
                            let max_force = shared_phase_parameters.max_force;

                            // Velocities of the two ends relative to the RSO.
                            let (root_vel, tail_vel) = {
                                let q = rb_forces.p0();
                                let end_vel = |e: Option<&Entity>| {
                                    e.and_then(|&n| q.get(n).ok())
                                        .map(|rb| rb.linear_velocity.0 - capture_entity_linvel.0)
                                        .unwrap_or(DVec3::ZERO)
                                };
                                (end_vel(nodes.first()), end_vel(nodes.last()))
                            };

                            for &node in &interior_entities {
                                let v_trans = {
                                    let rb_query = rb_forces.p0();
                                    let Ok(rb) = rb_query.get(node) else {
                                        continue;
                                    };
                                    let v_rel = rb.linear_velocity.0 - capture_entity_linvel.0;
                                    // Expected rigid velocity = end velocities interpolated by
                                    // the node's position along the line.
                                    let t = ((rb.position.0 - root_pos).dot(line_dir) / line_len)
                                        .clamp(0.0, 1.0);
                                    let v_expected = root_vel * (1.0 - t) + tail_vel * t;
                                    let v_osc = v_rel - v_expected;
                                    v_osc - v_osc.dot(line_dir) * line_dir
                                };
                                let f = (-v_trans * TRANSVERSE_DAMP).clamp_length_max(max_force);
                                if let Ok(mut node_forces) = rb_forces.p1().get_mut(node) {
                                    node_forces.apply_force(f);
                                }
                            }
                        }
                    }
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

/// Force applied to a tether *end* (root or tail) during the terminal phase, using a
/// **velocity servo** rather than a force-toward-position controller. It assembles a desired
/// velocity (relative to the RSO) from goals built around the tether *center* (midpoint of
/// the two ends): orient + straighten the rope so it lies along the **tangent** to the
/// capture orbit (in the capture plane), pull the center onto the capture plane, and — once
/// the rope is in-plane, straight, and tangent — translate inward toward the RSO while
/// holding that pose. The desired velocity is capped at `max_velocity` and the actual
/// velocity is driven toward it, so the servo *brakes* into place instead of overshooting.
#[allow(clippy::too_many_arguments)]
fn terminal_end_force(
    node_pos: DVec3,
    rso_pos: DVec3,
    axis: DVec3,
    rel_v: DVec3,
    other_pos: Option<DVec3>,
    is_root: bool,
    natural_len: f64,
    straightness: f64,
    max_velocity: f64,
    max_force: f64,
) -> DVec3 {
    // Desired-velocity gains (m/s of target speed per metre of position error). The result
    // is capped at max_velocity, so these mainly set how early the end eases in.
    const ORIENT_GAIN: f64 = 1.0;
    const PLANE_GAIN: f64 = 1.0;
    // Steady in-plane approach speed (m/s) toward the RSO once in-plane/straight/tangent.
    const CLOSE_SPEED: f64 = 0.15;
    // Straightness below which the rope counts as "straight" for the close gate.
    const STRAIGHT_THRESHOLD: f64 = 0.15;
    // |line_dir·tangent| above which the rope counts as "tangent" for the close gate.
    const TANGENT_THRESHOLD: f64 = 0.9;
    // Center axial offset (m) below which the rope counts as "in-plane" for the close gate.
    const PLANE_TOLERANCE: f64 = 1.0;

    // Without the other end we can't define the tether line / center; fall back to simply
    // pulling this end onto the capture plane.
    let Some(other) = other_pos else {
        let axial = (node_pos - rso_pos).dot(axis);
        let v_des = (-axial * axis * PLANE_GAIN).clamp_length_max(max_velocity);
        let kv = max_force / max_velocity.max(1e-3);
        return ((v_des - rel_v) * kv).clamp_length_max(max_force);
    };

    // Geometry built around the tether center (midpoint of the two ends).
    let center = (node_pos + other) * 0.5;
    let center_rel = center - rso_pos;
    let center_axial = center_rel.dot(axis);
    let center_inplane = center_rel - center_axial * axis;
    let radial_hat = center_inplane
        .try_normalize()
        .unwrap_or_else(|| axis.any_orthonormal_vector());
    // Tangent to the capture orbit at this azimuth (in the capture plane).
    let tangent_hat = axis
        .cross(radial_hat)
        .try_normalize()
        .unwrap_or_else(|| axis.any_orthonormal_vector());

    // This end's offset from the center, and the tangent-aligned target offset. Keep the
    // side the end is already on; default by root/tail when the line is ~perpendicular.
    let this_offset = node_pos - center;
    let side = this_offset.dot(tangent_hat);
    let sign = if side.abs() > 1e-3 {
        side.signum()
    } else if is_root {
        1.0
    } else {
        -1.0
    };
    let target_offset = sign * (natural_len * 0.5) * tangent_hat;

    let mut v_des = DVec3::ZERO;

    // Orient + straighten: drive this end to its tangent-aligned, length-L/2 offset.
    v_des += (target_offset - this_offset) * ORIENT_GAIN;

    // Pull the center onto the capture plane (both ends translate together).
    v_des += -center_axial * axis * PLANE_GAIN;

    // Close toward the RSO along the center radial, gated on the rope being in-plane,
    // straight, and tangent — so closing only starts once aligned and *maintains* alignment
    // (radial-only motion keeps the azimuth, hence tangency, fixed).
    let straight_gate = ((STRAIGHT_THRESHOLD - straightness) / STRAIGHT_THRESHOLD).clamp(0.0, 1.0);
    let line_dir = (node_pos - other).normalize_or_zero();
    let tangent_align = line_dir.dot(tangent_hat).abs();
    let tangent_gate =
        ((tangent_align - TANGENT_THRESHOLD) / (1.0 - TANGENT_THRESHOLD)).clamp(0.0, 1.0);
    let plane_gate = (1.0 - center_axial.abs() / PLANE_TOLERANCE).clamp(0.0, 1.0);
    let align_gate = straight_gate * tangent_gate * plane_gate;
    v_des += -radial_hat * CLOSE_SPEED * align_gate;

    // Rate-limit the desired speed, then servo the actual velocity toward it. kv saturates
    // a full max_velocity error at max_force.
    let v_des = v_des.clamp_length_max(max_velocity);
    let kv = max_force / max_velocity.max(1e-3);
    ((v_des - rel_v) * kv).clamp_length_max(max_force)
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

/// Maximum perpendicular deviation of the tether nodes from the straight line between the
/// tether's two ends (`end_a`→`end_b`), normalized by that line's length. `0.0` means the
/// chain lies perfectly on the line (fully straight), independent of how the line is
/// oriented relative to the RSO. Returns `0.0` for degenerate inputs.
pub(crate) fn tether_straightness(node_positions: &[DVec3], end_a: DVec3, end_b: DVec3) -> f64 {
    let line_len = (end_b - end_a).length();
    if line_len < 1e-6 {
        return 0.0;
    }
    let max_perp = node_positions
        .iter()
        .map(|&p| (p - project_onto_line(p, end_a, end_b)).length())
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
