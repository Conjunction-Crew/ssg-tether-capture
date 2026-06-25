use std::fmt::Write;

use crate::{
    components::{
        capture_components::CaptureComponent,
        user_interface::{
            CaptureGuidanceReadout, CaptureTelemetryReadout, OrbitLabel, TimeWarpReadout,
        },
    },
    constants::{EARTH_RADIUS, SCENE_LAYER},
    plugins::gpu_compute::eci_position_to_map,
    resources::{
        capture_log::{LogEvent, LogLevel},
        capture_plans::{CapturePlanLibrary, CaptureSphereRadius, CompiledCapturePhase},
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
    rso_speed_m_s: f64,
    rso_altitude_m: f64,
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
    captures: Query<&CaptureComponent>,
    capture_sphere_radius: Res<CaptureSphereRadius>,
    mut readouts: Query<(&mut Text, &CaptureTelemetryReadout)>,
    orbitals: Res<OrbitalCache>,
    mut log: MessageWriter<LogEvent>,
    mut prev_status: Local<String>,
) {
    for (mut text, readout) in &mut readouts {
        let Some(metrics) = capture_metrics(
            &bodies,
            readout.rso_entity,
            readout.reference_entity,
            &orbitals,
        ) else {
            text.0 = format!(
                "{}\nWaiting for live capture telemetry...",
                readout.rso_label
            );
            continue;
        };

        let capture_status = match readout
            .rso_entity
            .and_then(|entity| captures.get(entity).ok())
        {
            Some(capture) => format!("Engaged ({})", capture.current_phase),
            None => "Idle".to_string(),
        };

        if capture_status != *prev_status {
            log.write(LogEvent {
                level: LogLevel::Info,
                source: "capture",
                message: format!("Capture status → {capture_status}"),
            });
            *prev_status = capture_status.clone();
        }

        let inside_capture_sphere = if metrics.range_m <= capture_sphere_radius.radius as f64 {
            "Yes"
        } else {
            "No"
        };

        text.0 = format!(
            concat!(
                "RSO: {}\n",
                "Capture status: {}\n",
                "Range to tether root: {:.2} m\n",
                "Relative speed: {:.2} m/s\n",
                "Closing rate: {:.2} m/s\n",
                "Inside capture sphere: {}\n",
                "Capture sphere radius: {:.2} m\n",
                "RSO altitude: {:.1} m\n",
                "RSO speed: {:.2} m/s"
            ),
            readout.rso_label,
            capture_status,
            metrics.range_m,
            metrics.relative_speed_m_s,
            metrics.closing_speed_m_s,
            inside_capture_sphere,
            capture_sphere_radius.radius,
            metrics.rso_altitude_m,
            metrics.rso_speed_m_s,
        );
    }
}

pub fn update_capture_guidance(
    bodies: Query<(RigidBodyQueryReadOnly, Has<RigidBodyDisabled>)>,
    captures: Query<&CaptureComponent>,
    capture_plans: Res<CapturePlanLibrary>,
    capture_sphere_radius: Res<CaptureSphereRadius>,
    mut readouts: Query<(&mut Text, &CaptureGuidanceReadout)>,
    orbitals: Res<OrbitalCache>,
) {
    for (mut text, readout) in &mut readouts {
        let current_metrics = capture_metrics(
            &bodies,
            readout.rso_entity,
            readout.reference_entity,
            &orbitals,
        );
        let current_range = current_metrics.as_ref().map(|metrics| metrics.range_m);
        let current_rel_speed = current_metrics
            .as_ref()
            .map(|metrics| metrics.relative_speed_m_s);

        if let Some(capture) = readout
            .rso_entity
            .and_then(|entity| captures.get(entity).ok())
        {
            let Some(plan) = capture_plans.compiled_plans.get(&capture.plan_id) else {
                text.0 = format!("Active capture plan `{}` is not loaded.", capture.plan_id);
                continue;
            };

            let Some(phase) = plan.phase(&capture.current_phase) else {
                text.0 = format!(
                    "Current phase `{}` was not found in plan `{}`.",
                    capture.current_phase, capture.plan_id
                );
                continue;
            };

            let mut body = String::new();
            let time_in_phase = capture.phase_elapsed_time_s.max(0.0);
            let plan_display_name = capture_plans
                .plans
                .get(&capture.plan_id)
                .map(|p| p.name.as_str())
                .unwrap_or(capture.plan_id.as_str());

            writeln!(body, "RSO: {}", readout.rso_label).unwrap();
            writeln!(body, "Plan: {}", plan_display_name).unwrap();
            writeln!(body, "Current phase: {}", capture.current_phase).unwrap();
            writeln!(body, "Time in phase: {:.1} s", time_in_phase).unwrap();
            writeln!(
                body,
                "Capture sphere radius: {:.2} m",
                capture_sphere_radius.radius
            )
            .unwrap();
            writeln!(body).unwrap();

            append_phase_parameters(&mut body, phase);
            writeln!(body).unwrap();

            append_transitions(&mut body, phase, current_range, current_rel_speed, true);

            text.0 = body.trim_end().to_string();
            continue;
        }

        let Some(plan) = capture_plans.compiled_plans.get(&readout.plan_id) else {
            text.0 = format!("Capture plan `{}` is not loaded.", readout.plan_id);
            continue;
        };

        let Some(initial_phase) = plan.phases.first() else {
            text.0 = format!(
                "Capture plan `{}` does not define any phases.",
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
        writeln!(body, "RSO: {}", readout.rso_label).unwrap();
        writeln!(body, "Status: Idle").unwrap();
        writeln!(body, "Plan: {}", plan_display_name).unwrap();
        writeln!(body, "Initial phase: {}", initial_phase.id).unwrap();
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
            initial_phase,
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

        let world_position = eci_position_to_map(Vec3::new(
            (params[0] + rb.position.0.x) as f32,
            (params[1] + rb.position.0.y) as f32,
            (params[2] + rb.position.0.z) as f32,
        ));

        if let Ok(viewport_position) = cam.world_to_viewport(cam_transform, world_position) {
            node.top = Val::Px(viewport_position.y);
            node.left = Val::Px(viewport_position.x);
        }
    }
}

fn capture_metrics(
    bodies: &Query<(RigidBodyQueryReadOnly, Has<RigidBodyDisabled>)>,
    rso_entity: Option<Entity>,
    reference_entity: Option<Entity>,
    orbital_cache: &Res<OrbitalCache>,
) -> Option<CaptureMetrics> {
    let rso_entity = rso_entity?;
    let reference_entity = reference_entity?;

    let Ok((rso_rb, rso_disabled)) = bodies.get(rso_entity) else {
        return None;
    };
    let Ok((reference_rb, reference_disabled)) = bodies.get(reference_entity) else {
        return None;
    };

    let Some(rso_true) = orbital_cache.eci_states.get(&rso_entity) else {
        return None;
    };
    let Some(reference_true) = orbital_cache.eci_states.get(&reference_entity) else {
        return None;
    };

    let rso_position = world_position(&rso_true, rso_rb.position.0, rso_disabled);
    let reference_position =
        world_position(&reference_true, reference_rb.position.0, reference_disabled);
    let relative_position = rso_position - reference_position;

    let rso_velocity = world_velocity(&rso_true, rso_rb.linear_velocity.0, rso_disabled);
    let reference_velocity = world_velocity(
        &reference_true,
        reference_rb.linear_velocity.0,
        reference_disabled,
    );
    let relative_velocity = rso_velocity - reference_velocity;

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
        rso_speed_m_s: rso_velocity.length(),
        rso_altitude_m: rso_position.length() - EARTH_RADIUS as f64,
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

fn append_phase_parameters(body: &mut String, phase: &CompiledCapturePhase) {
    writeln!(body, "Phase parameters").unwrap();
    let p = &phase.parameters;
    writeln!(body, "- max_velocity: {:.4}", p.max_velocity).unwrap();
    writeln!(body, "- max_force: {:.4}", p.max_force).unwrap();
    if let Some(sr) = p.shrink_rate {
        writeln!(body, "- shrink_rate: {:.4}", sr).unwrap();
    }
}

fn append_transitions(
    body: &mut String,
    phase: &CompiledCapturePhase,
    current_range: Option<f64>,
    current_rel_speed: Option<f64>,
    active_capture: bool,
) {
    if active_capture {
        writeln!(body, "Possible transitions").unwrap();
    } else {
        writeln!(body, "Upcoming transitions").unwrap();
    }

    if phase.transitions.is_empty() {
        writeln!(body, "- none").unwrap();
        return;
    }

    for t in &phase.transitions {
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
        if let Some(limit) = t.straightness_less_than {
            conditions.push(format_condition("straightness", "<", limit, "", None));
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
