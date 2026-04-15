use std::fmt::Write;

use crate::{
    components::{
        capture_components::{CaptureComponent, State},
        orbit::Orbital,
        user_interface::{
            CaptureGuidanceReadout, CaptureTelemetryReadout, OrbitLabel, TimeWarpReadout,
        },
    },
    constants::{EARTH_RADIUS, MAP_UNITS_TO_M},
    resources::{
        capture_plans::{CapturePlanLibrary, CaptureSphereRadius, CompiledCaptureState},
        orbital_cache::OrbitalCache,
        world_time::WorldTime,
    },
};

use avian3d::prelude::{RigidBodyDisabled, RigidBodyQueryReadOnly};
use bevy::{camera::visibility::RenderLayers, math::DVec3, prelude::*};
use nalgebra::Vector6;

struct CaptureMetrics {
    range_m: f64,
    relative_speed_m_s: f64,
    closing_speed_m_s: f64,
    target_speed_m_s: f64,
    target_altitude_m: f64,
}

const MAP_LABEL_BOX_SIZE_PX: f32 = 30.0;

fn position_label_at_viewport_center(node: &mut Node, center: Vec2, size_px: f32) {
    let half_size = size_px * 0.5;

    node.left = Val::Px(center.x - half_size);
    node.top = Val::Px(center.y - half_size);
    node.width = Val::Px(size_px);
    node.height = Val::Px(size_px);
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
            let Some(plan) = capture_plans.compiled_plans.get(&capture.plan_id) else {
                text.0 = format!("Active capture plan `{}` is not loaded.", capture.plan_id);
                continue;
            };

            let Some(state) = plan.state(&capture.current_state) else {
                text.0 = format!(
                    "Current state `{}` was not found in plan `{}`.",
                    capture.current_state, capture.plan_id
                );
                continue;
            };

            let mut body = String::new();
            let time_in_state = capture.state_elapsed_time_s.max(0.0);
            let plan_display_name = capture_plans
                .plans
                .get(&capture.plan_id)
                .map(|p| p.name.as_str())
                .unwrap_or(capture.plan_id.as_str());

            writeln!(body, "Target: {}", readout.target_label).unwrap();
            writeln!(body, "Plan: {}", plan_display_name).unwrap();
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

        let Some(plan) = capture_plans.compiled_plans.get(&readout.plan_id) else {
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
        let plan_display_name = capture_plans
            .plans
            .get(&readout.plan_id)
            .map(|p| p.name.as_str())
            .unwrap_or(readout.plan_id.as_str());
        writeln!(body, "Target: {}", readout.target_label).unwrap();
        writeln!(body, "Status: Idle").unwrap();
        writeln!(body, "Plan: {}", plan_display_name).unwrap();
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
    rigidbodies: Query<(RigidBodyQueryReadOnly, Has<RigidBodyDisabled>, Entity)>,
    orbital_cache: Res<OrbitalCache>,
) {
    let (cam, cam_transform, render_layers) = camera.into_inner();

    for (mut node, label) in &mut labels {
        if !render_layers.intersects(&RenderLayers::layer(crate::constants::MAP_LAYER)) {
            node.display = Display::None;
            continue;
        }

        node.display = Display::Block;

        let Some(entity) = label.entity else {
            continue;
        };
        let Ok((rb, disabled, entity)) = rigidbodies.get(entity) else {
            continue;
        };

        let Some(params) = orbital_cache.eci_states.get(&entity) else {
            continue;
        };

        let base = DVec3::new(
            params[0] / MAP_UNITS_TO_M,
            params[1] / MAP_UNITS_TO_M,
            params[2] / MAP_UNITS_TO_M,
        );
        let world_position = if disabled {
            base
        } else {
            base + rb.position.0 / MAP_UNITS_TO_M
        };

        if let Ok(viewport_position) =
            cam.world_to_viewport(cam_transform, world_position.as_vec3())
        {
            position_label_at_viewport_center(&mut node, viewport_position, MAP_LABEL_BOX_SIZE_PX);
        } else {
            node.display = Display::None;
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

fn append_state_parameters(body: &mut String, state: &CompiledCaptureState) {
    writeln!(body, "State parameters").unwrap();
    let p = &state.parameters;
    writeln!(body, "- max_velocity: {:.4}", p.max_velocity).unwrap();
    writeln!(body, "- max_force: {:.4}", p.max_force).unwrap();
    if let Some(sr) = p.shrink_rate {
        writeln!(body, "- shrink_rate: {:.4}", sr).unwrap();
    }
}

fn append_transitions(
    body: &mut String,
    state: &CompiledCaptureState,
    current_range: Option<f64>,
    current_rel_speed: Option<f64>,
    active_capture: bool,
) {
    if active_capture {
        writeln!(body, "Possible transitions").unwrap();
    } else {
        writeln!(body, "Upcoming transitions").unwrap();
    }

    if state.transitions.is_empty() {
        writeln!(body, "- none").unwrap();
        return;
    }

    for t in &state.transitions {
        let mut conditions = Vec::new();
        if let Some(limit) = t.distance_less_than {
            conditions.push(format_condition("distance", "<", limit, "m", current_range));
        }
        if let Some(limit) = t.distance_greater_than {
            conditions.push(format_condition("distance", ">", limit, "m", current_range));
        }
        if let Some(limit) = t.relative_velocity_less_than {
            conditions.push(format_condition(
                "relative velocity",
                "<",
                limit,
                "m/s",
                current_rel_speed,
            ));
        }
        if let Some(limit) = t.relative_velocity_greater_than {
            conditions.push(format_condition(
                "relative velocity",
                ">",
                limit,
                "m/s",
                current_rel_speed,
            ));
        }
        if conditions.is_empty() {
            writeln!(body, "- {} when conditions are met", t.to).unwrap();
        } else {
            writeln!(body, "- {} when {}", t.to, conditions.join(" and ")).unwrap();
        }
    }
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
