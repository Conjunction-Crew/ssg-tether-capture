//! Capture: the per-step capture controller and its state machine.

pub mod plan;

#[cfg(debug_assertions)]
use avian3d::prelude::RigidBody;
use avian3d::{
    math::PI,
    prelude::{
        Forces, LinearVelocity, Position, RigidBodyDisabled, RigidBodyQuery, Rotation,
        WriteRigidBodyForces,
    },
};
use bevy::{ecs::system::SystemParam, log::debug_once, math::DVec3, prelude::*};
use brahe::Epoch;

use crate::SimSet;
use crate::SimStep;
use crate::capture::plan::{
    CapturePlanLibrary, CompiledCapturePlan, CompiledCaptureStateParameters,
    CompiledCaptureTransition, PhaseEnd, build_capture_component,
};
use crate::orbit::cache::OrbitalCache;
use crate::orbit::lvlh::LvlhFrame;
use crate::physics_bubble::body_eci_state;
use crate::recording::inputs::SimInputLog;
use crate::telemetry::{DataCollection, EphemerisSample, LogEvent, LogLevel};
use crate::time::{ManualPhysics, PHYS_DT, WorldTime};

// Component to add to an entity to attempt a capture.
// There should only ever be 0 or 1 entity with this component at a time.
#[derive(Component, Debug, Clone)]
pub struct CaptureComponent {
    pub plan_id: String,
    pub current_state: String,
    pub state_enter_time_s: f64,
    pub state_elapsed_time_s: f64,
}

/// The capture-sphere radius at the start of every capture (F-11).
pub const INITIAL_CAPTURE_SPHERE_RADIUS_M: f64 = 25.0;

/// Seconds the force bound takes to rise from 0 to the phase's `max_force_n`, at
/// the capture start and at every phase entry (F-31, Q10 row 8). A direction
/// reversal does not restart it (C2).
pub const FORCE_RAMP_S: f64 = 1.0;

/// The relative width of the hysteresis band on the controller's three branch
/// thresholds (the `0.8 R` inner edge, the `R` outer edge and the velocity kill),
/// so a body at a threshold does not flip branch every step (F-31). The phase
/// transitions have no band.
pub const BRANCH_DEAD_BAND: f64 = 0.02;

/// The capture controller's per-body state and output, on the tether root and the
/// tail while a capture runs: inserted when the capture starts
/// ([`apply_sim_commands`]) and removed with the [`CaptureComponent`]. The root is
/// the chaser (design D8).
#[derive(Component, Debug, Clone, Copy, Default, PartialEq)]
pub struct CaptureForce {
    /// The force the controller handed to avian this step, in N; zero for a body
    /// the current phase does not push. The gizmo reads it (F-41).
    pub applied: DVec3,
    /// The ramped bound in N: 0 at the capture start and at every phase entry,
    /// rising by `max_force_n × PHYS_DT / FORCE_RAMP_S` per pushed step up to
    /// `max_force_n`.
    pub ramp_n: f64,
    /// The unit direction of the last non-zero applied force. Drives nothing.
    pub direction: DVec3,
    /// Inside the inner edge (`0.8 R`), with the dead band.
    pub inner: bool,
    /// Outside the outer edge (`R`), with the dead band.
    pub outer: bool,
    /// Above the velocity limit, with the dead band.
    pub kill: bool,
}

impl CaptureForce {
    /// Updates the three branch states for this step's range `rel_r_len` and speed
    /// `rel_v_len` with the dead band: the inner edge is entered below
    /// `0.8 R (1 − band)` and left above `0.8 R (1 + band)`, the outer edge entered
    /// above `R (1 + band)` and left below `R (1 − band)`, the kill entered above
    /// `max_velocity` and left below `max_velocity (1 − band)`.
    fn update_branches(
        &mut self,
        rel_r_len: f64,
        rel_v_len: f64,
        capture_radius: f64,
        max_velocity: f64,
    ) {
        let band = BRANCH_DEAD_BAND;
        let inner_edge = capture_radius * 0.8;
        self.inner = if self.inner {
            rel_r_len <= inner_edge * (1.0 + band)
        } else {
            rel_r_len < inner_edge * (1.0 - band)
        };
        self.outer = if self.outer {
            rel_r_len >= capture_radius * (1.0 - band)
        } else {
            rel_r_len > capture_radius * (1.0 + band)
        };
        self.kill = if self.kill {
            rel_v_len >= max_velocity * (1.0 - band)
        } else {
            rel_v_len > max_velocity
        };
    }
}

/// A running capture records an [`EphemerisSample`] at every step since the sim
/// start that is a multiple of this (64 steps = 1 s), for the app's OEM export.
pub const EPHEMERIS_EVERY_STEPS: u64 = 64;

#[derive(Resource, Debug, Default)]
pub struct CaptureSphereRadius {
    pub radius: f64,
}

/// A capture-state transition, written by the capture controller next to its
/// `State: from → to (reason)` log line. `epoch` is the sub-step the transition
/// fired in, so `round((epoch − start_epoch) / PHYS_DT)` is its exact fixed step,
/// also when one update runs several steps (warp, F-17).
#[derive(Message, Debug, Clone, PartialEq)]
pub struct CaptureTransition {
    pub entity: Entity,
    pub from: String,
    pub to: String,
    pub epoch: Epoch,
    pub reason: String,
}

/// Written once when a capture enters a phase that has `end` (Q10 row 7), next
/// to the log line `Capture ended: <outcome> (phase <id>)`. Nothing else changes:
/// the controller keeps running in that phase (E4). `epoch` is the sub-step it
/// happened in, as for [`CaptureTransition`].
#[derive(Message, Debug, Clone, PartialEq)]
pub struct CaptureEnded {
    pub entity: Entity,
    pub phase: String,
    pub outcome: PhaseEnd,
    pub epoch: Epoch,
}

/// The messages [`capture_state_machine_update`] writes.
#[derive(SystemParam)]
pub struct CaptureMessages<'w> {
    log_events: MessageWriter<'w, LogEvent>,
    transitions: MessageWriter<'w, CaptureTransition>,
    ended: MessageWriter<'w, CaptureEnded>,
}

/// The read-only data [`capture_state_machine_update`] reads.
#[derive(SystemParam)]
pub struct CaptureInputs<'w, 's> {
    capture_plan_lib: Res<'w, CapturePlanLibrary>,
    orbital_cache: Res<'w, OrbitalCache>,
    world_time: Res<'w, WorldTime>,
    disabled: Query<'w, 's, (), With<RigidBodyDisabled>>,
}

/// The velocity of a tether node relative to the capture target, measured in the
/// target's LVLH frame (F-35): `rel_v − omega × rel_r` with `rel_r = node −
/// target` and `rel_v = node_v − target_v`, in ECI axes. Without a frame (the
/// target has no cached state yet) the plain difference.
fn node_relative_velocity(frame: Option<&LvlhFrame>, rel_r: DVec3, rel_v: DVec3) -> DVec3 {
    match frame {
        Some(frame) => frame.relative_velocity_in_frame(rel_r, rel_v),
        None => rel_v,
    }
}

pub fn capture_state_machine_update(
    mut commands: Commands,
    capture_entities: Query<(Entity, &mut CaptureComponent)>,
    inputs: CaptureInputs,
    mut rb_forces: ParamSet<(Query<RigidBodyQuery>, Query<Forces>)>,
    mut capture_forces: Query<(Entity, &mut CaptureForce)>,
    mut capture_sphere_radius: ResMut<CaptureSphereRadius>,
    mut data_collection: ResMut<DataCollection>,
    mut messages: CaptureMessages,
) {
    let CaptureInputs {
        capture_plan_lib,
        orbital_cache,
        world_time,
        disabled,
    } = inputs;
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
                let root_rv: Option<(f64, f64)> = nodes.first().and_then(|&root_node| {
                    rb_forces.p0().get(root_node).ok().map(|root_rb| {
                        let r = capture_entity_position.0 - root_rb.position.0;
                        let v = node_relative_velocity(
                            frame.as_ref(),
                            -r,
                            root_rb.linear_velocity.0 - capture_entity_linvel.0,
                        );
                        (r.length(), v.length())
                    })
                });

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

                let (shared_state_parameters, entered_phase) = if let Some((r_len, v_len)) = root_rv
                {
                    resolve_root_state(
                        capture_entity,
                        &mut capture_component,
                        plan,
                        (r_len, v_len),
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

                let up = (capture_entity_rotation * DVec3::X).normalize_or(DVec3::X);

                for (idx, &node) in nodes.iter().enumerate() {
                    let (rel_r, rel_v) = {
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
                    let max_velocity = shared_state_parameters.max_velocity;
                    let max_force = shared_state_parameters.max_force;
                    let capture_radius = if idx == 0 {
                        root_capture_radius
                    } else {
                        capture_sphere_radius.radius + 1.0
                    };
                    state.update_branches(rel_r_len, rel_v_len, capture_radius, max_velocity);

                    if !pushed {
                        state.applied = DVec3::ZERO;
                        continue;
                    }

                    // If vel is high, kill vel
                    if state.kill {
                        force_vec += -rel_v.normalize_or_zero() * 2.0;
                    }
                    // If too close, force in opposite dir
                    if state.inner {
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

                    // F-31: `max_force_n` is an upper bound reached through the ramp.
                    state.ramp_n =
                        (state.ramp_n + max_force * PHYS_DT / FORCE_RAMP_S).min(max_force);
                    // F-11: a zero vector gives a zero force, not NaN.
                    let direction = force_vec.normalize_or_zero();
                    let force = direction * state.ramp_n;
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
    (rel_r_length, rel_v_length): (f64, f64),
    world_time: &WorldTime,
    capture_sphere_radius: &mut CaptureSphereRadius,
    messages: &mut CaptureMessages,
) -> (CompiledCaptureStateParameters, bool) {
    let Some(state) = plan.state(&capture_component.current_state) else {
        return (CompiledCaptureStateParameters::default(), false);
    };

    let fired = state.transitions.iter().find_map(|transition| {
        transition_satisfied(transition, rel_r_length, rel_v_length)
            .map(|reason| (transition.to.clone(), reason))
    });
    let entered_phase = fired.is_some();
    if let Some((to, reason)) = fired {
        let end = plan.state(&to).and_then(|state| state.end);
        transition_to(
            capture_entity,
            capture_component,
            &to,
            end,
            world_time,
            reason,
            messages,
        );
    }

    let parameters = current_state_parameters(plan, &capture_component.current_state);
    if let Some(shrink_rate) = parameters.shrink_rate
        && capture_sphere_radius.radius > 0.1
    {
        capture_sphere_radius.radius -= shrink_rate * PHYS_DT;
    }

    (parameters, entered_phase)
}

/// The reason string if every condition of `transition` holds (`None` if one does
/// not, or if it has no condition), e.g. `distance 1500.0 m < 2000.0 m, rel vel
/// 1.00 m/s < 5.00 m/s`.
fn transition_satisfied(
    transition: &CompiledCaptureTransition,
    rel_r_length: f64,
    rel_v_length: f64,
) -> Option<String> {
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

/// Enters `new_state` at the current sim time (`state_enter_time_s` is sim seconds
/// since the start epoch) and reports it as a log line and a [`CaptureTransition`],
/// and, when `new_state` is an end phase (`end`), as a [`CaptureEnded`].
fn transition_to(
    capture_entity: Entity,
    capture_component: &mut CaptureComponent,
    new_state: &str,
    end: Option<PhaseEnd>,
    world_time: &WorldTime,
    reason: String,
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
}

/// Writes the [`CaptureEnded`] message and its Info log line.
fn report_capture_ended(
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

/// Debug builds only: panics if an enabled rigid body has a non-finite `Position`
/// or `LinearVelocity`, naming the entity and the step (no NaN reaches avian).
#[cfg(debug_assertions)]
pub fn assert_finite_state(
    bodies: Query<
        (Entity, &Position, &LinearVelocity),
        (With<RigidBody>, Without<RigidBodyDisabled>),
    >,
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

/// Commands the app sends to the simulation (D1).
#[derive(Message, Debug, Clone)]
pub enum SimCommand {
    /// Mark `target` for capture with the plan `plan_id` (the UI's "Capture").
    StartCapture {
        target: Option<Entity>,
        plan_id: String,
    },
}

/// Applies the queued [`SimCommand`]s in `Last` ([`SimSet::ApplyCommands`]), the
/// frame the UI sends them, as the UI handler did before P3. The capture start
/// time is sim seconds since the start epoch (F-38: fixed in P4). Every command
/// that starts a capture is appended to the [`SimInputLog`] after the current
/// step; a refused one changes nothing and is not an input.
pub fn apply_sim_commands(
    mut commands: Commands,
    mut sim_commands: MessageReader<SimCommand>,
    capture_plan_lib: Res<CapturePlanLibrary>,
    // Absent outside the sim screen, where no capture target exists.
    world_time: Option<Res<WorldTime>>,
    capture_entities: Query<Entity, With<CaptureComponent>>,
    // The tether registry (absent outside the sim screen) and the bodies that
    // carry the controller's state of the capture being replaced.
    (orbital_cache, capture_force_bodies): (
        Option<Res<OrbitalCache>>,
        Query<Entity, With<CaptureForce>>,
    ),
    // The radius is absent outside the sim screen, like `WorldTime`.
    (mut data_collection, mut capture_sphere_radius): (
        ResMut<DataCollection>,
        Option<ResMut<CaptureSphereRadius>>,
    ),
    (mut log, mut ended, mut input_log): (
        MessageWriter<LogEvent>,
        MessageWriter<CaptureEnded>,
        ResMut<SimInputLog>,
    ),
) {
    for command in sim_commands.read() {
        match command {
            SimCommand::StartCapture {
                target: entity,
                plan_id,
            } => {
                if let Some(capture_entity) = entity {
                    // Check if the entity is not already marked for capture
                    if !capture_entities.contains(*capture_entity) {
                        // Remove CaptureComponent from entities (if any)
                        for marked_entity in capture_entities {
                            commands.entity(marked_entity).remove::<CaptureComponent>();
                        }
                        for body in &capture_force_bodies {
                            commands.entity(body).remove::<CaptureForce>();
                        }
                        // Get plan information
                        if let Some(plan) = capture_plan_lib.plans.get(plan_id) {
                            // Now, mark the entity for capture
                            if let Some(capture_component) = build_capture_component(
                                plan_id.as_str(),
                                plan,
                                world_time.as_ref().map_or(0.0, |t| t.epoch - t.start_epoch),
                            ) {
                                commands.entity(*capture_entity).insert(capture_component);
                                // The controller's state on the root and the tail, from
                                // the first step (the ramp starts at 0 here).
                                if let Some(nodes) = orbital_cache
                                    .as_ref()
                                    .and_then(|cache| cache.tethers.get(&plan.tether))
                                {
                                    for &node in nodes {
                                        commands.entity(node).insert(CaptureForce::default());
                                    }
                                }
                                // F-11: every capture starts with the initial radius, the
                                // plan's `capture_sphere.initial_radius_m` (25 m by default).
                                if let Some(radius) = capture_sphere_radius.as_mut() {
                                    radius.radius = plan.initial_capture_radius_m();
                                }
                                log.write(LogEvent {
                                    level: LogLevel::Info,
                                    source: "ui",
                                    message: format!(
                                        "Capture initiated for entity {capture_entity:?} with plan '{plan_id}'"
                                    ),
                                });
                                let epoch = world_time.as_ref().map(|t| t.epoch);
                                let step = world_time.as_ref().map_or(0, |t| {
                                    ((t.epoch - t.start_epoch) / PHYS_DT).round() as u64
                                });
                                input_log.push_start_capture(step, *capture_entity, plan_id);
                                if let (Some(first), Some(epoch)) = (plan.phases.first(), epoch)
                                    && let Some(outcome) = first.end
                                {
                                    report_capture_ended(
                                        *capture_entity,
                                        &first.id,
                                        outcome,
                                        epoch,
                                        &mut log,
                                        &mut ended,
                                    );
                                }
                            } else {
                                log.write(LogEvent {
                                    level: LogLevel::Error,
                                    source: "ui",
                                    message: format!(
                                        "Failed to initiate capture for entity {capture_entity:?} with plan '{plan_id}'"
                                    ),
                                });
                                continue;
                            }

                            // Begin data collection on entity
                            data_collection
                                .position
                                .insert(*capture_entity, Vec::<(f64, f64)>::new());
                            data_collection
                                .velocity
                                .insert(*capture_entity, Vec::<(f64, f64)>::new());
                            data_collection
                                .ephemeris
                                .insert(*capture_entity, Vec::new());
                        } else {
                            log.write(LogEvent {
                                level: LogLevel::Error,
                                source: "ui",
                                message: format!("Capture plan '{plan_id}' not found"),
                            });
                        }
                    } else {
                        log.write(LogEvent {
                            level: LogLevel::Warn,
                            source: "ui",
                            message: format!(
                                "Entity {capture_entity:?} already marked for capture — ignoring"
                            ),
                        });
                    }
                } else {
                    log.write(LogEvent {
                        level: LogLevel::Warn,
                        source: "ui",
                        message: format!(
                            "Capture initiated with no target entity for plan '{plan_id}'"
                        ),
                    });
                }
            }
        }
    }
}

pub struct CapturePlugin;

impl Plugin for CapturePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<SimCommand>()
            .add_message::<CaptureTransition>()
            .add_message::<CaptureEnded>()
            .configure_sets(Last, SimSet::ApplyCommands)
            .add_systems(
                ManualPhysics,
                capture_state_machine_update.in_set(SimStep::Capture),
            )
            .add_systems(Last, apply_sim_commands.in_set(SimSet::ApplyCommands));
        #[cfg(debug_assertions)]
        app.add_systems(
            ManualPhysics,
            assert_finite_state
                .in_set(SimStep::Capture)
                .after(capture_state_machine_update),
        );
    }
}
