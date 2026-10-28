//! Capture: the per-step capture controller and its state machine.

pub mod plan;

use avian3d::{
    math::PI,
    prelude::{Forces, LinearVelocity, Position, RigidBodyQuery, Rotation, WriteRigidBodyForces},
};
use bevy::{ecs::system::SystemParam, math::DVec3, prelude::*};

use crate::SimSet;
use crate::SimStep;
use crate::capture::plan::{
    CapturePlanLibrary, CompiledCapturePlan, CompiledCaptureStateParameters,
    CompiledCaptureTransition, build_capture_component,
};
use crate::orbit::cache::OrbitalCache;
use crate::telemetry::{DataCollection, LogEvent, LogLevel};
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

#[derive(Resource, Debug, Default)]
pub struct CaptureSphereRadius {
    pub radius: f64,
}

/// The read-only resources [`capture_state_machine_update`] reads.
#[derive(SystemParam)]
pub struct CaptureInputs<'w> {
    capture_plan_lib: Res<'w, CapturePlanLibrary>,
    orbital_cache: Res<'w, OrbitalCache>,
    world_time: Res<'w, WorldTime>,
}

pub fn capture_state_machine_update(
    mut commands: Commands,
    capture_entities: Query<(Entity, &mut CaptureComponent)>,
    inputs: CaptureInputs,
    mut rb_forces: ParamSet<(Query<RigidBodyQuery>, Query<Forces>)>,
    mut capture_sphere_radius: ResMut<CaptureSphereRadius>,
    mut data_collection: ResMut<DataCollection>,
    mut log_events: MessageWriter<LogEvent>,
) {
    let CaptureInputs {
        capture_plan_lib,
        orbital_cache,
        world_time,
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

                let shared_state_parameters = if let Some((r_len, v_len)) = root_rv {
                    resolve_root_state(
                        &mut capture_component,
                        plan,
                        r_len,
                        v_len,
                        world_time.epoch - world_time.start_epoch,
                        &mut capture_sphere_radius,
                        &mut log_events,
                    )
                } else {
                    current_state_parameters(plan, &capture_component.current_state)
                };

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
                        // F-11: a zero vector gives a zero force, not NaN.
                        node_forces.apply_force(force_vec.normalize_or_zero() * max_force);
                    } else {
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
    sim_time_s: f64,
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

        if let Some(shrink_rate) = state.parameters.shrink_rate
            && capture_sphere_radius.radius > 0.1
        {
            capture_sphere_radius.radius -= shrink_rate * PHYS_DT;
        }

        for transition in &state.transitions {
            apply_transition(
                capture_component,
                transition,
                rel_r_length,
                rel_v_length,
                sim_time_s,
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
    sim_time_s: f64,
    log_events: &mut MessageWriter<LogEvent>,
) {
    if let Some(limit) = transition.distance_less_than
        && rel_r_length < limit
    {
        transition_to(
            capture_component,
            &transition.to,
            sim_time_s,
            format!("distance {:.1} m < {:.1} m", rel_r_length, limit),
            log_events,
        );
    }

    if let Some(limit) = transition.distance_greater_than
        && rel_r_length > limit
    {
        transition_to(
            capture_component,
            &transition.to,
            sim_time_s,
            format!("distance {:.1} m > {:.1} m", rel_r_length, limit),
            log_events,
        );
    }

    if let Some(limit) = transition.relative_velocity_less_than
        && rel_v_length < limit
    {
        transition_to(
            capture_component,
            &transition.to,
            sim_time_s,
            format!("rel vel {:.2} m/s < {:.2} m/s", rel_v_length, limit),
            log_events,
        );
    }

    if let Some(limit) = transition.relative_velocity_greater_than
        && rel_v_length > limit
    {
        transition_to(
            capture_component,
            &transition.to,
            sim_time_s,
            format!("rel vel {:.2} m/s > {:.2} m/s", rel_v_length, limit),
            log_events,
        );
    }
}

/// Enters `new_state` at `sim_time_s` (sim seconds since the start epoch).
fn transition_to(
    capture_component: &mut CaptureComponent,
    new_state: &str,
    sim_time_s: f64,
    reason: String,
    log_events: &mut MessageWriter<LogEvent>,
) {
    log_events.write(LogEvent {
        level: LogLevel::Info,
        source: "capture",
        message: format!(
            "State: {} → {} ({})",
            capture_component.current_state, new_state, reason
        ),
    });
    capture_component.current_state = String::from(new_state);
    capture_component.state_enter_time_s = sim_time_s;
    capture_component.state_elapsed_time_s = 0.0;
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
/// time is sim seconds since the start epoch (F-38: fixed in P4).
pub fn apply_sim_commands(
    mut commands: Commands,
    mut sim_commands: MessageReader<SimCommand>,
    capture_plan_lib: Res<CapturePlanLibrary>,
    // Absent outside the sim screen, where no capture target exists.
    world_time: Option<Res<WorldTime>>,
    capture_entities: Query<Entity, With<CaptureComponent>>,
    // The radius is absent outside the sim screen, like `WorldTime`.
    (mut data_collection, mut capture_sphere_radius): (
        ResMut<DataCollection>,
        Option<ResMut<CaptureSphereRadius>>,
    ),
    mut log: MessageWriter<LogEvent>,
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
                        // Get plan information
                        if let Some(plan) = capture_plan_lib.plans.get(plan_id) {
                            // Now, mark the entity for capture
                            if let Some(capture_component) = build_capture_component(
                                plan_id.as_str(),
                                plan,
                                world_time.as_ref().map_or(0.0, |t| t.epoch - t.start_epoch),
                            ) {
                                commands.entity(*capture_entity).insert(capture_component);
                                // F-11: every capture starts with the initial radius.
                                if let Some(radius) = capture_sphere_radius.as_mut() {
                                    radius.radius = INITIAL_CAPTURE_SPHERE_RADIUS_M;
                                }
                                log.write(LogEvent {
                                    level: LogLevel::Info,
                                    source: "ui",
                                    message: format!(
                                        "Capture initiated for entity {capture_entity:?} with plan '{plan_id}'"
                                    ),
                                });
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
            .configure_sets(Last, SimSet::ApplyCommands)
            .add_systems(
                ManualPhysics,
                capture_state_machine_update.in_set(SimStep::Capture),
            )
            .add_systems(Last, apply_sim_commands.in_set(SimSet::ApplyCommands));
    }
}
