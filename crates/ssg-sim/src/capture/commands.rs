//! The commands the app sends to the simulation and the system that applies them.

use super::state_machine::report_capture_ended;
use super::*;

/// Commands the app sends to the simulation (D1).
#[derive(Message, Debug, Clone)]
pub enum SimCommand {
    /// Mark `target` for capture with the plan `plan_id` (the UI's "Capture").
    StartCapture {
        target: Option<Entity>,
        plan_id: String,
    },
}

/// The plans and the clock [`apply_sim_commands`] reads.
#[derive(SystemParam)]
pub struct SimCommandInputs<'w> {
    capture_plan_lib: Res<'w, CapturePlanLibrary>,
    // Absent outside the sim screen, where no capture target exists.
    world_time: Option<Res<'w, WorldTime>>,
}

/// Applies the queued [`SimCommand`]s in `Last` ([`SimSet::ApplyCommands`]), the
/// frame the UI sends them, as the UI handler did before P3. The capture start
/// time is sim seconds since the start epoch (F-38: fixed in P4). Every command
/// that starts a capture is appended to the [`SimInputLog`] after the current
/// step; a refused one changes nothing and is not an input.
pub fn apply_sim_commands(
    mut commands: Commands,
    mut sim_commands: MessageReader<SimCommand>,
    inputs: SimCommandInputs,
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
    let SimCommandInputs {
        capture_plan_lib,
        world_time,
    } = inputs;
    for command in sim_commands.read() {
        match command {
            SimCommand::StartCapture {
                target: entity,
                plan_id,
            } => {
                if let Some(capture_entity) = entity {
                    // Check if the entity is not already marked for capture
                    if !capture_entities.contains(*capture_entity) {
                        // F-52: look the plan up (and build its component) before
                        // touching the capture in progress, so a start that fails
                        // leaves that capture as it was.
                        let Some(plan) = capture_plan_lib.plans.get(plan_id) else {
                            log.write(LogEvent {
                                level: LogLevel::Error,
                                source: "ui",
                                message: format!("Capture plan '{plan_id}' not found"),
                            });
                            continue;
                        };
                        let Some(capture_component) = build_capture_component(
                            plan_id.as_str(),
                            plan,
                            world_time.as_ref().map_or(0.0, |t| t.epoch - t.start_epoch),
                        ) else {
                            log.write(LogEvent {
                                level: LogLevel::Error,
                                source: "ui",
                                message: format!(
                                    "Failed to initiate capture for entity {capture_entity:?} with plan '{plan_id}'"
                                ),
                            });
                            continue;
                        };
                        // Remove CaptureComponent from entities (if any)
                        for marked_entity in capture_entities {
                            commands.entity(marked_entity).remove::<CaptureComponent>();
                        }
                        for body in &capture_force_bodies {
                            commands.entity(body).remove::<CaptureForce>();
                        }
                        // Now, mark the entity for capture
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
                        let step = world_time
                            .as_ref()
                            .map_or(0, |t| ((t.epoch - t.start_epoch) / PHYS_DT).round() as u64);
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
