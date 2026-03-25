use std::fmt::Write;

use crate::{
    components::{
        capture_components::{CaptureComponent, State},
        orbit::Orbital,
        user_interface::{
            CaptureGuidanceReadout, CaptureTelemetryReadout, OrbitLabel, TimeWarpReadout,
        },
    },
    constants::{EARTH_RADIUS, MAP_UNITS_TO_M, SCENE_LAYER},
    resources::{
        capture_plans::{CapturePlanLibrary, CaptureSphereRadius},
        orbital_cache::{self, OrbitalCache},
        world_time::WorldTime,
    },
};

use crate::components::user_interface::OrbitLabel;

use avian3d::prelude::{RigidBodyDisabled, RigidBodyQueryReadOnly};
use bevy::{camera::visibility::RenderLayers, math::DVec3, prelude::*};
use brahe::utils::DOrbitStateProvider;
use nalgebra::Vector6;
use serde_json::Value;

struct CaptureMetrics {
    range_m: f64,
    relative_speed_m_s: f64,
    closing_speed_m_s: f64,
    target_speed_m_s: f64,
    target_altitude_m: f64,
}

pub fn update_time_warp_readout(
    mut readouts: Query<&mut Text, With<TimeWarpReadout>>,
    world_time: Res<WorldTime>,
) {
    for mut text in &mut readouts {
        text.0 = format!("{}x", world_time.multiplier);
    }
}

pub fn update_capture_telemetry(
    bodies: Query<(RigidBodyQueryReadOnly, Has<RigidBodyDisabled>)>,
    capture_targets: Query<&CaptureComponent>,
    capture_sphere_radius: Res<CaptureSphereRadius>,
    mut readouts: Query<(&mut Text, &CaptureTelemetryReadout)>,
    orbitals: Res<OrbitalCache>,
) {
    for (mut text, readout) in &mut readouts {
        let Some(metrics) = capture_metrics(
            &bodies,
            readout.target_entity,
            readout.reference_entity,
            &orbitals,
        ) else {
            text.0 = format!(
                "{}\nWaiting for live capture telemetry...",
                readout.target_label
            );
            continue;
        };

        let capture_status = match readout
            .target_entity
            .and_then(|entity| capture_targets.get(entity).ok())
        {
            Some(capture) => format!("Engaged ({})", capture.current_state),
            None => "Idle".to_string(),
        };

        let inside_capture_sphere = if metrics.range_m <= capture_sphere_radius.radius as f64 {
            "Yes"
        } else {
            "No"
        };

        text.0 = format!(
            concat!(
                "Target: {}\n",
                "Capture status: {}\n",
                "Range to tether root: {:.2} m\n",
                "Relative speed: {:.2} m/s\n",
                "Closing rate: {:.2} m/s\n",
                "Inside capture sphere: {}\n",
                "Capture sphere radius: {:.2} m\n",
                "Target altitude: {:.1} m\n",
                "Target speed: {:.2} m/s"
            ),
            readout.target_label,
            capture_status,
            metrics.range_m,
            metrics.relative_speed_m_s,
            metrics.closing_speed_m_s,
            inside_capture_sphere,
            capture_sphere_radius.radius,
            metrics.target_altitude_m,
            metrics.target_speed_m_s,
        );
    }
}

pub fn update_capture_guidance(
    bodies: Query<(RigidBodyQueryReadOnly, Has<RigidBodyDisabled>)>,
    capture_targets: Query<&CaptureComponent>,
    capture_plans: Res<CapturePlanLibrary>,
    capture_sphere_radius: Res<CaptureSphereRadius>,
    mut readouts: Query<(&mut Text, &CaptureGuidanceReadout)>,
    orbitals: Res<OrbitalCache>,
    world_time: Res<WorldTime>,
) {
    for (mut text, readout) in &mut readouts {
        let current_metrics = capture_metrics(
            &bodies,
            readout.target_entity,
            readout.reference_entity,
            &orbitals,
        );
        let current_range = current_metrics.as_ref().map(|metrics| metrics.range_m);
        let current_rel_speed = current_metrics
            .as_ref()
            .map(|metrics| metrics.relative_speed_m_s);

        if let Some(capture) = readout
            .target_entity
            .and_then(|entity| capture_targets.get(entity).ok())
        {
            let Some(plan) = capture_plans.plans.get(&capture.plan_id) else {
                text.0 = format!("Active capture plan `{}` is not loaded.", capture.plan_id);
                continue;
            };

            let Some(state) = plan
                .states
                .iter()
                .find(|state| state.id == capture.current_state)
            else {
                text.0 = format!(
                    "Current state `{}` was not found in plan `{}`.",
                    capture.current_state, capture.plan_id
                );
                continue;
            };

            let mut body = String::new();
            let time_in_state = capture.state_elapsed_time_s.max(0.0);

            writeln!(body, "Target: {}", readout.target_label).unwrap();
            writeln!(body, "Plan: {}", capture.plan_id).unwrap();
            writeln!(body, "Current state: {}", capture.current_state).unwrap();
            writeln!(body, "Time in state: {:.1} s", time_in_state).unwrap();
            writeln!(
                body,
                "Capture sphere radius: {:.2} m",
                capture_sphere_radius.radius
            )
            .unwrap();
            writeln!(body).unwrap();

            append_state_parameters(&mut body, state);
            writeln!(body).unwrap();

            append_transitions(&mut body, state, current_range, current_rel_speed, true);

            text.0 = body.trim_end().to_string();
            continue;
        }

        let Some(plan) = capture_plans.plans.get(&readout.plan_id) else {
            text.0 = format!("Capture plan `{}` is not loaded.", readout.plan_id);
            continue;
        };

        let Some(initial_state) = plan.states.first() else {
            text.0 = format!(
                "Capture plan `{}` does not define any states.",
                readout.plan_id
            );
            continue;
        };

        let mut body = String::new();
        writeln!(body, "Target: {}", readout.target_label).unwrap();
        writeln!(body, "Status: Idle").unwrap();
        writeln!(body, "Plan: {}", readout.plan_id).unwrap();
        writeln!(body, "Initial state: {}", initial_state.id).unwrap();
        writeln!(
            body,
            "Capture sphere radius: {:.2} m",
            capture_sphere_radius.radius
        )
        .unwrap();
        writeln!(body).unwrap();
        writeln!(body, "Press Capture to start this plan.").unwrap();
        writeln!(body).unwrap();

        append_transitions(
            &mut body,
            initial_state,
            current_range,
            current_rel_speed,
            false,
        );

        text.0 = body.trim_end().to_string();
    }
}

pub fn map_orbitals(
    camera: Single<(&Camera, &GlobalTransform, &RenderLayers), With<Camera3d>>,
    mut labels: Query<(&mut Node, &OrbitLabel)>,
    rigidbodies: Query<(RigidBodyQueryReadOnly, Entity)>,
    orbital_cache: Res<OrbitalCache>,
    world_time: Res<WorldTime>,
) {
    let (cam, cam_transform, render_layers) = camera.into_inner();

    for (mut node, label) in &mut labels {
        if render_layers.intersects(&RenderLayers::layer(SCENE_LAYER)) {
            node.display = Display::None;
        } else {
            node.display = Display::Block;
        }
        let Some(entity) = label.entity else {
            return;
        };
        let Ok((rb, entity)) = rigidbodies.get(entity) else {
            return;
        };

        let Some(params) = orbital_cache.eci_states.get(&entity) else {
            return;
        };

        let mut world_position = DVec3::new(
            params[0] / MAP_UNITS_TO_M,
            params[1] / MAP_UNITS_TO_M,
            params[2] / MAP_UNITS_TO_M,
        );

        world_position += rb.position.0 / MAP_UNITS_TO_M;

        if let Ok(viewport_position) =
            cam.world_to_viewport(cam_transform, world_position.as_vec3())
        {
            node.top = Val::Px(viewport_position.y);
            node.left = Val::Px(viewport_position.x);
        }
    }
}

fn capture_metrics(
    bodies: &Query<(RigidBodyQueryReadOnly, Has<RigidBodyDisabled>)>,
    target_entity: Option<Entity>,
    reference_entity: Option<Entity>,
    orbital_cache: &Res<OrbitalCache>,
) -> Option<CaptureMetrics> {
    let target_entity = target_entity?;
    let reference_entity = reference_entity?;

    let Ok((target_rb, target_disabled)) = bodies.get(target_entity) else {
        return None;
    };
    let Ok((reference_rb, reference_disabled)) = bodies.get(reference_entity) else {
        return None;
    };

    let Some(target_true) = orbital_cache.eci_states.get(&target_entity) else {
        return None;
    };
    let Some(reference_true) = orbital_cache.eci_states.get(&reference_entity) else {
        return None;
    };

    let target_position = world_position(&target_true, target_rb.position.0, target_disabled);
    let reference_position =
        world_position(&reference_true, reference_rb.position.0, reference_disabled);
    let relative_position = target_position - reference_position;

    let target_velocity =
        world_velocity(&target_true, target_rb.linear_velocity.0, target_disabled);
    let reference_velocity = world_velocity(
        &reference_true,
        reference_rb.linear_velocity.0,
        reference_disabled,
    );
    let relative_velocity = target_velocity - reference_velocity;

    let range_m = relative_position.length();
    let closing_speed_m_s = if range_m > 1e-6 {
        -(relative_position / range_m).dot(relative_velocity)
    } else {
        0.0
    };

    Some(CaptureMetrics {
        range_m,
        relative_speed_m_s: relative_velocity.length(),
        closing_speed_m_s,
        target_speed_m_s: target_velocity.length(),
        target_altitude_m: target_position.length() - EARTH_RADIUS as f64,
    })
}

fn world_position(true_params: &Vector6<f64>, position: DVec3, disabled: bool) -> DVec3 {
    let base = DVec3::new(true_params[0], true_params[1], true_params[2]);
    if disabled { base } else { base + position }
}

fn world_velocity(true_params: &Vector6<f64>, linear_velocity: DVec3, disabled: bool) -> DVec3 {
    let base = DVec3::new(true_params[3], true_params[4], true_params[5]);
    if disabled {
        base
    } else {
        base + linear_velocity
    }
}

fn append_state_parameters(body: &mut String, state: &State) {
    writeln!(body, "State parameters").unwrap();

    let Some(parameters) = state.parameters.as_ref().and_then(Value::as_object) else {
        writeln!(body, "- none").unwrap();
        return;
    };

    if parameters.is_empty() {
        writeln!(body, "- none").unwrap();
        return;
    }

    let mut ordered_keys = Vec::new();
    for key in ["max_velocity", "max_force", "shrink_rate"] {
        if parameters.contains_key(key) {
            ordered_keys.push(key.to_string());
        }
    }

    let mut remaining_keys = parameters
        .keys()
        .filter(|key| !ordered_keys.iter().any(|ordered| ordered == *key))
        .cloned()
        .collect::<Vec<_>>();
    remaining_keys.sort();
    ordered_keys.extend(remaining_keys);

    for key in ordered_keys {
        if let Some(value) = parameters.get(&key).and_then(format_value) {
            writeln!(body, "- {}: {}", key, value).unwrap();
        }
    }
}

fn append_transitions(
    body: &mut String,
    state: &State,
    current_range: Option<f64>,
    current_rel_speed: Option<f64>,
    active_capture: bool,
) {
    if active_capture {
        writeln!(body, "Possible transitions").unwrap();
    } else {
        writeln!(body, "Upcoming transitions").unwrap();
    }

    let Some(transitions) = &state.transitions else {
        writeln!(body, "- none").unwrap();
        return;
    };

    if transitions.is_empty() {
        writeln!(body, "- none").unwrap();
        return;
    }

    let mut found_transition = false;
    for transition in transitions {
        if let Some(line) = format_transition_line(transition, current_range, current_rel_speed) {
            found_transition = true;
            writeln!(body, "{}", line).unwrap();
        }
    }

    if !found_transition {
        writeln!(body, "- conditions not available").unwrap();
    }
}

fn format_transition_line(
    transition: &Value,
    current_range: Option<f64>,
    current_rel_speed: Option<f64>,
) -> Option<String> {
    let to = transition.get("to").and_then(Value::as_str)?;
    let mut conditions = Vec::new();

    if let Some(distance) = transition.get("distance") {
        let units = distance.get("units").and_then(Value::as_str).unwrap_or("m");
        if let Some(limit) = distance.get("less_than").and_then(Value::as_f64) {
            conditions.push(format_condition(
                "distance",
                "<",
                limit,
                units,
                current_range,
            ));
        }
        if let Some(limit) = distance.get("greater_than").and_then(Value::as_f64) {
            conditions.push(format_condition(
                "distance",
                ">",
                limit,
                units,
                current_range,
            ));
        }
    }

    if let Some(relative_velocity) = transition.get("relative_velocity") {
        if let Some(limit) = relative_velocity.get("less_than").and_then(Value::as_f64) {
            conditions.push(format_condition(
                "relative velocity",
                "<",
                limit,
                "m/s",
                current_rel_speed,
            ));
        }
        if let Some(limit) = relative_velocity
            .get("greater_than")
            .and_then(Value::as_f64)
        {
            conditions.push(format_condition(
                "relative velocity",
                ">",
                limit,
                "m/s",
                current_rel_speed,
            ));
        }
    }

    if conditions.is_empty() {
        return Some(format!("- {} when conditions are met", to));
    }

    Some(format!("- {} when {}", to, conditions.join(" and ")))
}

fn format_condition(
    label: &str,
    comparator: &str,
    threshold: f64,
    units: &str,
    current_value: Option<f64>,
) -> String {
    let threshold_text = if units.is_empty() {
        format!("{:.2}", threshold)
    } else {
        format!("{:.2} {}", threshold, units)
    };

    if let Some(current) = current_value {
        let ready = match comparator {
            "<" => current < threshold,
            ">" => current > threshold,
            _ => false,
        };
        let readiness = if ready { "ready" } else { "waiting" };

        if units.is_empty() {
            format!(
                "{} {} {} (current {:.2}, {})",
                label, comparator, threshold_text, current, readiness
            )
        } else {
            format!(
                "{} {} {} (current {:.2} {}, {})",
                label, comparator, threshold_text, current, units, readiness
            )
        }
    } else {
        format!("{} {} {}", label, comparator, threshold_text)
    }
}

fn format_value(value: &Value) -> Option<String> {
    if let Some(float) = value.as_f64() {
        return Some(format!("{:.2}", float));
    }

    if let Some(integer) = value.as_i64() {
        return Some(integer.to_string());
    }

    if let Some(boolean) = value.as_bool() {
        return Some(boolean.to_string());
    }

    value.as_str().map(ToString::to_string)
}
