//! The capture state machine: one controller step per tether per physics step.

use super::*;

pub fn capture_state_machine_update(
    mut commands: Commands,
    capture_entities: Query<(Entity, &mut CaptureComponent)>,
    inputs: CaptureInputs,
    mut rb_forces: ParamSet<(Query<RigidBodyQuery>, Query<Forces>)>,
    mut capture_forces: Query<(Entity, &mut CaptureForce)>,
    outputs: CaptureOutputs,
    mut messages: CaptureMessages,
) {
    let CaptureInputs {
        capture_plan_lib,
        orbital_cache,
        world_time,
        disabled,
        joints,
    } = inputs;
    let CaptureOutputs {
        mut capture_sphere_radius,
        mut data_collection,
    } = outputs;
    for (capture_entity, mut capture_component) in capture_entities {
        capture_component.state_elapsed_time_s += PHYS_DT;

        // Get current position and velocity of capture entity
        let capture_entity_position: Position;
        let capture_entity_linvel: LinearVelocity;
        let capture_entity_rotation: Rotation;
        if let Ok(capture_entity_rb) = rb_forces.p0().get(capture_entity) {
            capture_entity_position = *capture_entity_rb.position;
            capture_entity_linvel = *capture_entity_rb.linear_velocity;
            capture_entity_rotation = *capture_entity_rb.rotation;
        } else {
            // F-39: one capture entity without a rigid body does not stop the others.
            continue;
        }

        // The capture target's own LVLH frame (F-35).
        let target_eci = body_eci_state(
            &orbital_cache,
            capture_entity,
            capture_entity_position.0,
            capture_entity_linvel.0,
            disabled.contains(capture_entity),
        );
        let frame = target_eci.and_then(|state| LvlhFrame::from_eci(&state));
        if frame.is_none() {
            debug_once!(
                "capture target {capture_entity:?} has no cached ECI state: relative velocity in ECI axes"
            );
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
                // resolve_root_state (which needs &mut messages).
                let root_rv: Option<(f64, f64, Option<f64>)> =
                    nodes.first().and_then(|&root_node| {
                        rb_forces.p0().get(root_node).ok().map(|root_rb| {
                            let r = capture_entity_position.0 - root_rb.position.0;
                            let v = node_relative_velocity(
                                frame.as_ref(),
                                -r,
                                root_rb.linear_velocity.0 - capture_entity_linvel.0,
                            );
                            (r.length(), v.length(), closing_rate(r, v))
                        })
                    });
                // P9 S3: the tether's shape, only for a phase that has a transition
                // on it (no cost, and nothing read, otherwise).
                let straightness =
                    if plan
                        .state(&capture_component.current_state)
                        .is_some_and(|state| {
                            state
                                .transitions
                                .iter()
                                .any(|transition| transition.uses_tether_straightness())
                        })
                    {
                        nodes.first().and_then(|&root_node| {
                            let rb_query = rb_forces.p0();
                            let chain: Vec<DVec3> = chain_from_joints(root_node, joints.iter())
                                .into_iter()
                                .filter_map(|body| rb_query.get(body).ok().map(|rb| rb.position.0))
                                .collect();
                            tether_straightness(&chain)
                        })
                    } else {
                        None
                    };

                // The ephemeris sample (read-only: no force or transition depends on
                // it). The cache and the positions describe this step's end state.
                let step = ((world_time.epoch - world_time.start_epoch) / PHYS_DT).round() as u64;
                if step.is_multiple_of(EPHEMERIS_EVERY_STEPS)
                    && let Some(samples) = data_collection.ephemeris.get_mut(&capture_entity)
                    && let Some(target) = target_eci
                    && let Some(chaser) = nodes.first().and_then(|&root_node| {
                        let rb_query = rb_forces.p0();
                        let root_rb = rb_query.get(root_node).ok()?;
                        body_eci_state(
                            &orbital_cache,
                            root_node,
                            root_rb.position.0,
                            root_rb.linear_velocity.0,
                            disabled.contains(root_node),
                        )
                    })
                {
                    samples.push(EphemerisSample {
                        epoch: world_time.epoch,
                        target,
                        chaser,
                    });
                }

                let (shared_state_parameters, entered_phase) =
                    if let Some((r_len, v_len, closing)) = root_rv {
                        let measured = TransitionInputs {
                            distance_m: r_len,
                            relative_speed_mps: v_len,
                            closing_rate_mps: closing,
                            tether_straightness: straightness,
                            time_in_phase_s: capture_component.state_elapsed_time_s,
                        };
                        resolve_root_state(
                            capture_entity,
                            &mut capture_component,
                            plan,
                            &measured,
                            &world_time,
                            &mut capture_sphere_radius,
                            &mut messages,
                        )
                    } else {
                        (
                            current_state_parameters(plan, &capture_component.current_state),
                            false,
                        )
                    };
                // C2: the ramp restarts at every phase entry (and at the capture
                // start, where `CaptureForce` is inserted at 0), never on a reversal.
                if entered_phase {
                    for &node in nodes {
                        if let Ok((_, mut state)) = capture_forces.get_mut(node) {
                            state.ramp_n = 0.0;
                        }
                    }
                }

                let up = (capture_entity_rotation * CAPTURE_CIRCLING_AXIS)
                    .normalize_or(CAPTURE_CIRCLING_AXIS);

                // D18: the root's `rel_r` and `rel_v` of this step, for the tail's lag
                // law (the root is index 0).
                let mut root_rel: Option<(DVec3, DVec3)> = None;
                for (idx, &node) in nodes.iter().enumerate() {
                    let (rel_r, rel_v, mass) = {
                        let rb_query = rb_forces.p0();
                        let Ok(rb) = rb_query.get(node) else {
                            continue;
                        };

                        // `rel_r` points from the node to the target; `rel_v` is
                        // the node's LVLH-frame velocity relative to it (F-35).
                        let rel_r = capture_entity_position.0 - rb.position.0;
                        (
                            rel_r,
                            node_relative_velocity(
                                frame.as_ref(),
                                -rel_r,
                                rb.linear_velocity.0 - capture_entity_linvel.0,
                            ),
                            rb.mass.value(),
                        )
                    };
                    let rel_r_len = rel_r.length();
                    let rel_v_len = rel_v.length();
                    if idx == 0 {
                        root_rel = Some((rel_r, rel_v));
                    }

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

                    // C7: the root always; the tail only in a phase with `tail_actuation`.
                    let pushed = idx == 0 || shared_state_parameters.tail_actuation;
                    let Ok((_, mut state)) = capture_forces.get_mut(node) else {
                        // Every capture start inserts it on the root and the tail.
                        if !pushed {
                            continue;
                        }
                        messages.log_events.write(LogEvent {
                            level: LogLevel::Warn,
                            source: "capture",
                            message: format!(
                                "Failed to apply force to tether node {idx} (no CaptureForce)"
                            ),
                        });
                        continue;
                    };

                    let mut force_vec = DVec3::ZERO;
                    // D16: a `hold` phase reads its speed limit as 0.
                    let hold = shared_state_parameters.actuation == Actuation::Hold;
                    let phase_max_velocity = if hold {
                        0.0
                    } else {
                        shared_state_parameters.max_velocity
                    };
                    // D14: the limit falls with the sphere in a phase that asks for it.
                    let max_velocity = if shared_state_parameters.velocity_scales_with_sphere {
                        phase_max_velocity
                            * (capture_sphere_radius.radius
                                / capture_component.sphere_radius_at_entry_m)
                    } else {
                        phase_max_velocity
                    };
                    let max_force = shared_state_parameters.max_force;
                    // P9 S4: the phase's gains (the constants by default).
                    let gains = &shared_state_parameters.controller;
                    // D7 (C4): the kill band is at least one step's Δv of this body at
                    // the phase's force, from the unscaled limit.
                    let kill_band = (gains.dead_band_fraction * phase_max_velocity)
                        .max(2.0 * max_force * PHYS_DT / mass);
                    // D16: how far `x` is across one kill band, in 0..=1. A band of 0
                    // (`max_force_n` and `max_velocity_mps` both 0, where the ramp is
                    // 0 too) is the step the law tends to, not 0 / 0 (F-11).
                    let across_band = |x: f64| {
                        if kill_band > 0.0 {
                            (x / kill_band).clamp(0.0, 1.0)
                        } else {
                            f64::from(u8::from(x > 0.0))
                        }
                    };
                    let capture_radius = if idx == 0 {
                        root_capture_radius
                    } else {
                        capture_sphere_radius.radius + 1.0
                    };
                    state.update_branches(
                        rel_r_len,
                        rel_v_len,
                        capture_radius,
                        max_velocity,
                        kill_band,
                        gains,
                    );

                    if !pushed {
                        state.applied = DVec3::ZERO;
                        continue;
                    }

                    // D18: the tail's lag law, in a phase that pushes the tail with
                    // `tail_spacing_m`, outside `capture` (the counter-tangent wrap)
                    // and `hold`: the weight of its tangent term.
                    let counter = idx != 0 && capture_component.current_state == "capture";
                    let lag_weight = match (shared_state_parameters.tail_spacing_m, root_rel) {
                        (Some(spacing), Some(root)) if idx != 0 && !counter && !hold => {
                            Some(tail_lag_weight(
                                root,
                                (rel_r, rel_v),
                                up,
                                capture_radius,
                                spacing,
                                gains,
                            ))
                        }
                        _ => None,
                    };

                    // D16: the hold law brakes the body to rest, continuously (no
                    // dead band), in place of the branches.
                    if hold {
                        force_vec = -rel_v.normalize_or_zero() * across_band(rel_v_len);
                    }
                    // D18: the circle law, in a phase that shrinks the sphere, in
                    // place of the three branches (whose states are still updated).
                    else if let Some(shrink_rate) = shared_state_parameters.shrink_rate {
                        let tangent = forward_tangent(rel_r, up);
                        let tangent_term = match lag_weight {
                            Some(w) => tangent * w,
                            None if counter => -tangent,
                            None => tangent,
                        };
                        force_vec += circle_law(
                            rel_r,
                            rel_v,
                            capture_radius,
                            shrink_rate,
                            tangent_term,
                            lag_weight.is_some(),
                            gains,
                        );
                    }
                    // If too close, force in opposite dir
                    else if state.inner {
                        force_vec += -rel_r.normalize_or_zero();
                    }
                    // If we are outside the sphere radius, force in target dir (or slow down)
                    else if state.outer {
                        if rel_v.angle_between(rel_r) > PI / 2.0 {
                            force_vec += -rel_v.normalize_or_zero();
                        }

                        force_vec += rel_r.normalize_or_zero();
                    // Otherwise, force in tangent dir
                    } else {
                        let tangent = forward_tangent(rel_r, up);
                        if let Some(w) = lag_weight {
                            force_vec += tangent * w;
                        } else if counter {
                            force_vec -= tangent;
                        } else {
                            force_vec += tangent;
                        }
                    }

                    // D16: in a phase with `hold_plane`, a component toward the
                    // circling plane through the target, proportional to the height.
                    if shared_state_parameters.hold_plane {
                        let h = -rel_r.dot(up);
                        force_vec += -up * (h / gains.plane_hold_band_m).clamp(-1.0, 1.0);
                    }

                    // F-31: `max_force_n` is an upper bound reached through the ramp.
                    state.ramp_n =
                        (state.ramp_n + max_force * PHYS_DT / gains.force_ramp_s).min(max_force);
                    let force = if hold {
                        force_vec.clamp_length_max(1.0) * state.ramp_n
                    } else {
                        // D16 (F-31), the proportional limiter: the push along its own
                        // direction fades over one kill band as the velocity component
                        // along it reaches the limit, so a body at the limit is not
                        // re-pushed and re-braked every step (D7's measured 41,595
                        // reversals per terminal phase were this), while a velocity
                        // that drifts off the push direction is still corrected; the
                        // brake acts on the speed above the limit.
                        // F-11: a zero vector gives a zero force, not NaN.
                        //
                        // D18, the magnitude rule: the push keeps the composed law's
                        // magnitude, clamped to 1, so a proportional term is a
                        // proportional force (normalising it turned every small
                        // correction of the circle and lag laws into a full-force flip:
                        // 96,994 tail reversals per terminal phase). A vector within
                        // 1e-9 of unit length or longer counts as 1, so the branch law's
                        // unit sums give the same force as before, bit for bit.
                        let push_direction = force_vec.normalize_or_zero();
                        let magnitude = if force_vec.length_squared() >= 1.0 - 1e-9 {
                            1.0
                        } else {
                            force_vec.length()
                        };
                        let along = rel_v.dot(push_direction);
                        let push = push_direction * magnitude * across_band(max_velocity - along);
                        let brake =
                            -rel_v.normalize_or_zero() * across_band(rel_v_len - max_velocity);
                        (push + brake).clamp_length_max(1.0) * state.ramp_n
                    };
                    let direction = force.normalize_or_zero();
                    debug_assert!(
                        force.is_finite() && force.length() <= max_force * (1.0 + 1e-12),
                        "force {force:?} on tether node {idx} is not finite or above {max_force} N"
                    );

                    // Apply force
                    if let Ok(mut node_forces) = rb_forces.p1().get_mut(node) {
                        node_forces.apply_force(force);
                        state.applied = force;
                        if direction != DVec3::ZERO {
                            state.direction = direction;
                        }
                    } else {
                        state.applied = DVec3::ZERO;
                        messages.log_events.write(LogEvent {
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
                messages.log_events.write(LogEvent {
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
            for (entity, _) in &capture_forces {
                commands.entity(entity).remove::<CaptureForce>();
            }
            messages.log_events.write(LogEvent {
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

/// One step of the capture state machine (Q14, F-24). Only the current state's
/// transitions are checked, in plan order. A transition is satisfied when every
/// condition it carries holds; one without a condition never fires. The first
/// satisfied transition fires and nothing more happens this step (no cascade).
/// The state current after this check sets this step's parameters and shrinks the
/// capture sphere once, so when a transition fires, the new state's limits and
/// shrink rate already apply to this step's forces. The `bool` is whether a
/// transition fired (a phase entry, which restarts the force ramp).
fn resolve_root_state(
    capture_entity: Entity,
    capture_component: &mut CaptureComponent,
    plan: &CompiledCapturePlan,
    measured: &TransitionInputs,
    world_time: &WorldTime,
    capture_sphere_radius: &mut CaptureSphereRadius,
    messages: &mut CaptureMessages,
) -> (CompiledCaptureStateParameters, bool) {
    let Some(state) = plan.state(&capture_component.current_state) else {
        return (CompiledCaptureStateParameters::default(), false);
    };

    let fired = state.transitions.iter().find_map(|transition| {
        transition_satisfied(transition, measured).map(|reason| (transition.to.clone(), reason))
    });
    let entered_phase = fired.is_some();
    if let Some((to, reason)) = fired {
        let end = plan.state(&to).and_then(|state| state.end);
        transition_to(
            capture_entity,
            capture_component,
            (&to, end, reason),
            world_time,
            capture_sphere_radius.radius,
            messages,
        );
    }

    let parameters = current_state_parameters(plan, &capture_component.current_state);
    if let Some(shrink_rate) = parameters.shrink_rate
        && capture_sphere_radius.radius > plan.min_capture_radius_m
    {
        capture_sphere_radius.radius -= shrink_rate * PHYS_DT;
    }

    (parameters, entered_phase)
}

/// What a transition's conditions are checked against in one step: the root's
/// range to the target and LVLH relative speed, the closing rate (`None` at a range
/// of 0), the tether's straightness (`None` when not measured or undefined) and the
/// time in the current phase, already advanced this step.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct TransitionInputs {
    pub distance_m: f64,
    pub relative_speed_mps: f64,
    pub closing_rate_mps: Option<f64>,
    pub tether_straightness: Option<f64>,
    pub time_in_phase_s: f64,
}

/// The reason string if every condition of `transition` holds (`None` if one does
/// not, or if it has no condition), e.g. `distance 1500.0 m < 2000.0 m, rel vel
/// 1.00 m/s < 5.00 m/s`. A condition on a quantity that is `None` does not hold.
pub(crate) fn transition_satisfied(
    transition: &CompiledCaptureTransition,
    measured: &TransitionInputs,
) -> Option<String> {
    let rel_r_length = measured.distance_m;
    let rel_v_length = measured.relative_speed_mps;
    let state_elapsed_time_s = measured.time_in_phase_s;
    let checks = [
        transition.distance_less_than.map(|limit| {
            (
                rel_r_length < limit,
                format!("distance {:.1} m < {:.1} m", rel_r_length, limit),
            )
        }),
        transition.distance_greater_than.map(|limit| {
            (
                rel_r_length > limit,
                format!("distance {:.1} m > {:.1} m", rel_r_length, limit),
            )
        }),
        transition.relative_velocity_less_than.map(|limit| {
            (
                rel_v_length < limit,
                format!("rel vel {:.2} m/s < {:.2} m/s", rel_v_length, limit),
            )
        }),
        transition.relative_velocity_greater_than.map(|limit| {
            (
                rel_v_length > limit,
                format!("rel vel {:.2} m/s > {:.2} m/s", rel_v_length, limit),
            )
        }),
        transition.time_in_phase_less_than.map(|limit| {
            (
                state_elapsed_time_s < limit,
                format!(
                    "time in phase {:.1} s < {:.1} s",
                    state_elapsed_time_s, limit
                ),
            )
        }),
        transition.time_in_phase_greater_than.map(|limit| {
            (
                state_elapsed_time_s > limit,
                format!(
                    "time in phase {:.1} s > {:.1} s",
                    state_elapsed_time_s, limit
                ),
            )
        }),
        transition.closing_rate_less_than.map(|limit| {
            optional_check(measured.closing_rate_mps, |rate| {
                (
                    rate < limit,
                    format!("closing rate {rate:.2} m/s < {limit:.2} m/s"),
                )
            })
        }),
        transition.closing_rate_greater_than.map(|limit| {
            optional_check(measured.closing_rate_mps, |rate| {
                (
                    rate > limit,
                    format!("closing rate {rate:.2} m/s > {limit:.2} m/s"),
                )
            })
        }),
        transition.tether_straightness_less_than.map(|limit| {
            optional_check(measured.tether_straightness, |bow| {
                (
                    bow < limit,
                    format!("tether straightness {bow:.3} < {limit:.3}"),
                )
            })
        }),
        transition.tether_straightness_greater_than.map(|limit| {
            optional_check(measured.tether_straightness, |bow| {
                (
                    bow > limit,
                    format!("tether straightness {bow:.3} > {limit:.3}"),
                )
            })
        }),
    ];

    let mut reasons = Vec::new();
    for (holds, reason) in checks.into_iter().flatten() {
        if !holds {
            return None;
        }
        reasons.push(reason);
    }
    (!reasons.is_empty()).then(|| reasons.join(", "))
}

/// A check on a quantity that may be undefined: one that is `None` does not hold.
fn optional_check(value: Option<f64>, check: impl FnOnce(f64) -> (bool, String)) -> (bool, String) {
    match value {
        Some(value) => check(value),
        None => (false, String::new()),
    }
}

/// A transition that fired: the phase it enters, that phase's end outcome (if
/// it is an end phase) and the reason logged.
type FiredTransition<'a> = (&'a str, Option<PhaseEnd>, String);

/// Enters `new_state` at the current sim time (`state_enter_time_s` is sim seconds
/// since the start epoch) with the sphere at `sphere_radius_m`, and reports it as a
/// log line and a [`CaptureTransition`], and, when `new_state` is an end phase
/// (`end`), as a [`CaptureEnded`].
fn transition_to(
    capture_entity: Entity,
    capture_component: &mut CaptureComponent,
    (new_state, end, reason): FiredTransition,
    world_time: &WorldTime,
    sphere_radius_m: f64,
    messages: &mut CaptureMessages,
) {
    messages.log_events.write(LogEvent {
        level: LogLevel::Info,
        source: "capture",
        message: format!(
            "State: {} → {} ({})",
            capture_component.current_state, new_state, reason
        ),
    });
    messages.transitions.write(CaptureTransition {
        entity: capture_entity,
        from: capture_component.current_state.clone(),
        to: String::from(new_state),
        epoch: world_time.epoch,
        reason,
    });
    if let Some(outcome) = end {
        report_capture_ended(
            capture_entity,
            new_state,
            outcome,
            world_time.epoch,
            &mut messages.log_events,
            &mut messages.ended,
        );
    }
    capture_component.current_state = String::from(new_state);
    capture_component.state_enter_time_s = world_time.epoch - world_time.start_epoch;
    capture_component.state_elapsed_time_s = 0.0;
    capture_component.sphere_radius_at_entry_m = sphere_radius_m;
}

/// Writes the [`CaptureEnded`] message and its Info log line.
pub(super) fn report_capture_ended(
    entity: Entity,
    phase: &str,
    outcome: PhaseEnd,
    epoch: Epoch,
    log: &mut MessageWriter<LogEvent>,
    ended: &mut MessageWriter<CaptureEnded>,
) {
    let label = match outcome {
        PhaseEnd::Success => "success",
        PhaseEnd::Abort => "abort",
    };
    log.write(LogEvent {
        level: LogLevel::Info,
        source: "capture",
        message: format!("Capture ended: {label} (phase {phase})"),
    });
    ended.write(CaptureEnded {
        entity,
        phase: phase.to_string(),
        outcome,
        epoch,
    });
}

/// A rigid body that takes part in the physics step.
#[cfg(debug_assertions)]
type EnabledRigidBody = (With<RigidBody>, Without<RigidBodyDisabled>);

/// Debug builds only: panics if an enabled rigid body has a non-finite `Position`
/// or `LinearVelocity`, naming the entity and the step (no NaN reaches avian).
#[cfg(debug_assertions)]
pub fn assert_finite_state(
    bodies: Query<(Entity, &Position, &LinearVelocity), EnabledRigidBody>,
    world_time: Res<WorldTime>,
) {
    for (entity, position, velocity) in &bodies {
        if !position.0.is_finite() || !velocity.0.is_finite() {
            let step = ((world_time.epoch - world_time.start_epoch) / PHYS_DT).round();
            panic!(
                "non-finite state for {entity:?} at step {step}: position {:?}, velocity {:?}",
                position.0, velocity.0
            );
        }
    }
}
