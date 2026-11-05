use std::fmt::Write;

use avian3d::prelude::{RigidBodyDisabled, RigidBodyQueryReadOnly};
use bevy::{math::DVec3, prelude::*};

use brahe::Epoch;
use ssg_sim::capture::plan::{CapturePlanLibrary, CompiledCaptureState};
use ssg_sim::capture::{CaptureComponent, CaptureSphereRadius};
use ssg_sim::orbit::EARTH_EQUATORIAL_RADIUS_M;
use ssg_sim::orbit::cache::OrbitalCache;
use ssg_sim::orbit::lvlh::LvlhFrame;
use ssg_sim::physics_bubble::body_eci_state;
use ssg_sim::project::{SelectedProject, format_utc};
use ssg_sim::session::PauseState;
use ssg_sim::telemetry::{LogEvent, LogLevel};
use ssg_sim::time::WorldTime;

use crate::ui::form::{UnitSystem, show};
use crate::ui::settings::Settings;

/// What the sim sidebar shows as labels (E1): the `update_*` systems write the
/// strings every frame and keep their log side effects.
#[derive(Resource, Debug, Clone)]
pub struct HudReadouts {
    pub target_entity: Option<Entity>,
    pub reference_entity: Option<Entity>,
    pub target_label: String,
    pub plan_id: String,
    pub time_warp: String,
    pub sim_time: String,
    pub telemetry: String,
    pub guidance: String,
}

impl Default for HudReadouts {
    fn default() -> Self {
        Self {
            target_entity: None,
            reference_entity: None,
            target_label: String::new(),
            plan_id: String::new(),
            time_warp: "1x".to_string(),
            sim_time: String::new(),
            telemetry: "Waiting for live capture telemetry...".to_string(),
            guidance: "Waiting for capture plan telemetry...".to_string(),
        }
    }
}

/// Points the readouts at this session's bodies and plan and resets their text
/// (on entering the sim screen and again when the bodies exist, on Start).
pub fn init_hud_readouts(
    mut readouts: ResMut<HudReadouts>,
    selected_project: Res<SelectedProject>,
    capture_plan_lib: Res<CapturePlanLibrary>,
    orbital_cache: Res<OrbitalCache>,
) {
    let plan_id = selected_project.project_id.as_deref().unwrap_or("");
    let tether_name = capture_plan_lib
        .plans
        .get(plan_id)
        .map(|p| p.tether.as_str())
        .unwrap_or("");
    *readouts = HudReadouts {
        target_entity: orbital_cache.debris.get("Satellite1").copied(),
        reference_entity: orbital_cache
            .tethers
            .get(tether_name)
            .and_then(|v| v.first().copied()),
        target_label: String::from("Satellite1"),
        plan_id: plan_id.to_string(),
        ..default()
    };
}

pub(crate) struct CaptureMetrics {
    pub(crate) range_m: f64,
    pub(crate) relative_speed_m_s: f64,
    pub(crate) closing_speed_m_s: f64,
    pub(crate) target_speed_m_s: f64,
    pub(crate) target_altitude_m: f64,
}

// unused today; the P3 component/resource audit decides
#[allow(dead_code)]
const MAP_LABEL_BOX_SIZE_PX: f32 = 30.0;

// unused today; the P3 component/resource audit decides
#[allow(dead_code)]
fn position_label_at_viewport_center(node: &mut Node, center: Vec2, size_px: f32) {
    let half_size = size_px * 0.5;

    node.left = Val::Px(center.x - half_size);
    node.top = Val::Px(center.y - half_size);
    node.width = Val::Px(size_px);
    node.height = Val::Px(size_px);
}

/// `2026-11-04T01:52:08Z`: an epoch as the sidebar shows it, in whole seconds.
pub(crate) fn utc_seconds(epoch: Epoch) -> String {
    let text = format_utc(epoch);
    let seconds = text.split('.').next().unwrap_or(&text);
    format!("{seconds}Z")
}

/// `Sim time: <UTC> (T+h:mm:ss)`, elapsed since the start epoch.
pub(crate) fn sim_time_text(epoch: Epoch, start_epoch: Epoch) -> String {
    let elapsed_s = (epoch - start_epoch).max(0.0) as u64;
    let (hours, minutes, seconds) = (elapsed_s / 3600, elapsed_s / 60 % 60, elapsed_s % 60);
    format!(
        "Sim time: {} (T+{hours}:{minutes:02}:{seconds:02})",
        utc_seconds(epoch)
    )
}

/// The warp actually running, `{n}x` or `Paused` (F-46, OQ2): the steps the last
/// `FixedUpdate` ran, never more than the request. Before the first fixed step
/// (effective 0) it shows the request. Also writes the sim time.
pub fn update_time_warp_readout(
    mut readouts: ResMut<HudReadouts>,
    world_time: Res<WorldTime>,
    pause: Res<State<PauseState>>,
) {
    let requested = world_time.multiplier;
    let effective = world_time.effective_multiplier;
    readouts.time_warp = if *pause.get() == PauseState::Paused {
        "Paused".to_string()
    } else if effective == 0 {
        format!("{requested}x")
    } else {
        format!("{}x", effective.min(requested))
    };
    readouts.sim_time = sim_time_text(world_time.epoch, world_time.start_epoch);
}

pub fn update_capture_telemetry(
    bodies: Query<(RigidBodyQueryReadOnly, Has<RigidBodyDisabled>)>,
    capture_targets: Query<&CaptureComponent>,
    (capture_sphere_radius, settings): (Res<CaptureSphereRadius>, Res<Settings>),
    mut readouts: ResMut<HudReadouts>,
    orbitals: Res<OrbitalCache>,
    mut log: MessageWriter<LogEvent>,
    mut prev_status: Local<String>,
) {
    let target_entity = readouts.target_entity;
    let Some(metrics) =
        capture_metrics(&bodies, target_entity, readouts.reference_entity, &orbitals)
    else {
        readouts.telemetry = format!(
            "{}\nWaiting for live capture telemetry...",
            readouts.target_label
        );
        return;
    };

    let capture_status = match target_entity.and_then(|entity| capture_targets.get(entity).ok()) {
        Some(capture) => format!("Engaged ({})", capture.current_state),
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

    readouts.telemetry = telemetry_text(
        &readouts.target_label,
        &capture_status,
        &metrics,
        capture_sphere_radius.radius,
        settings.unit_system,
    );
}

/// The telemetry lines, in `units`.
pub(crate) fn telemetry_text(
    target_label: &str,
    capture_status: &str,
    metrics: &CaptureMetrics,
    capture_sphere_radius_m: f64,
    units: UnitSystem,
) -> String {
    let inside_capture_sphere = if metrics.range_m <= capture_sphere_radius_m {
        "Yes"
    } else {
        "No"
    };
    format!(
        concat!(
            "Target: {}\n",
            "Capture status: {}\n",
            "Range to tether root: {}\n",
            "Relative speed: {}\n",
            "Closing rate: {}\n",
            "Inside capture sphere: {}\n",
            "Capture sphere radius: {}\n",
            "Target altitude: {}\n",
            "Target speed: {}"
        ),
        target_label,
        capture_status,
        show(units.length(metrics.range_m), 2),
        show(units.speed(metrics.relative_speed_m_s), 2),
        show(units.speed(metrics.closing_speed_m_s), 2),
        inside_capture_sphere,
        show(units.length(capture_sphere_radius_m), 2),
        show(units.length(metrics.target_altitude_m), 1),
        show(units.speed(metrics.target_speed_m_s), 2),
    )
}

pub fn update_capture_guidance(
    bodies: Query<(RigidBodyQueryReadOnly, Has<RigidBodyDisabled>)>,
    capture_targets: Query<&CaptureComponent>,
    capture_plans: Res<CapturePlanLibrary>,
    capture_sphere_radius: Res<CaptureSphereRadius>,
    mut readouts: ResMut<HudReadouts>,
    orbitals: Res<OrbitalCache>,
    settings: Res<Settings>,
) {
    let units = settings.unit_system;
    let sphere_radius = show(units.length(capture_sphere_radius.radius), 2);
    let target_entity = readouts.target_entity;
    let plan_id = readouts.plan_id.clone();
    let current_metrics =
        capture_metrics(&bodies, target_entity, readouts.reference_entity, &orbitals);
    let current_range = current_metrics.as_ref().map(|metrics| metrics.range_m);
    let current_rel_speed = current_metrics
        .as_ref()
        .map(|metrics| metrics.relative_speed_m_s);

    if let Some(capture) = target_entity.and_then(|entity| capture_targets.get(entity).ok()) {
        let Some(plan) = capture_plans.compiled_plans.get(&capture.plan_id) else {
            readouts.guidance = format!("Active capture plan `{}` is not loaded.", capture.plan_id);
            return;
        };

        let Some(state) = plan.state(&capture.current_state) else {
            readouts.guidance = format!(
                "Current phase `{}` was not found in plan `{}`.",
                capture.current_state, capture.plan_id
            );
            return;
        };

        let mut body = String::new();
        let time_in_state = capture.state_elapsed_time_s.max(0.0);
        let plan_display_name = capture_plans
            .plans
            .get(&capture.plan_id)
            .map(|p| p.name.as_str())
            .unwrap_or(capture.plan_id.as_str());

        writeln!(body, "Target: {}", readouts.target_label).unwrap();
        writeln!(body, "Plan: {}", plan_display_name).unwrap();
        writeln!(body, "Current phase: {}", capture.current_state).unwrap();
        writeln!(body, "Time in phase: {:.1} s", time_in_state).unwrap();
        writeln!(body, "Capture sphere radius: {sphere_radius}").unwrap();
        writeln!(body).unwrap();

        append_state_parameters(&mut body, state, units);
        writeln!(body).unwrap();

        append_transitions(
            &mut body,
            state,
            current_range,
            current_rel_speed,
            true,
            units,
        );

        readouts.guidance = body.trim_end().to_string();
        return;
    }

    let Some(plan) = capture_plans.compiled_plans.get(&plan_id) else {
        readouts.guidance = format!("Capture plan `{}` is not loaded.", plan_id);
        return;
    };

    let Some(initial_state) = plan.states.first() else {
        readouts.guidance = format!("Capture plan `{}` does not define any phases.", plan_id);
        return;
    };

    let mut body = String::new();
    let plan_display_name = capture_plans
        .plans
        .get(&plan_id)
        .map(|p| p.name.as_str())
        .unwrap_or(plan_id.as_str());
    writeln!(body, "Target: {}", readouts.target_label).unwrap();
    writeln!(body, "Status: Idle").unwrap();
    writeln!(body, "Plan: {}", plan_display_name).unwrap();
    writeln!(body, "Initial phase: {}", initial_state.id).unwrap();
    writeln!(body, "Capture sphere radius: {sphere_radius}").unwrap();
    writeln!(body).unwrap();
    writeln!(body, "Press Capture to start this plan.").unwrap();
    writeln!(body).unwrap();

    append_transitions(
        &mut body,
        initial_state,
        current_range,
        current_rel_speed,
        false,
        units,
    );

    readouts.guidance = body.trim_end().to_string();
}

/// Range, speeds and altitude of the capture target relative to the reference
/// body. The relative velocity is measured in the target's LVLH frame (F-35).
pub(crate) fn capture_metrics(
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

    let target_state = body_eci_state(
        orbital_cache,
        target_entity,
        target_rb.position.0,
        target_rb.linear_velocity.0,
        target_disabled,
    )?;
    let reference_state = body_eci_state(
        orbital_cache,
        reference_entity,
        reference_rb.position.0,
        reference_rb.linear_velocity.0,
        reference_disabled,
    )?;
    let position = |state: &nalgebra::Vector6<f64>| DVec3::new(state[0], state[1], state[2]);
    let velocity = |state: &nalgebra::Vector6<f64>| DVec3::new(state[3], state[4], state[5]);

    let target_position = position(&target_state);
    let relative_position = target_position - position(&reference_state);
    let target_velocity = velocity(&target_state);
    let eci_relative_velocity = target_velocity - velocity(&reference_state);
    // In the target's LVLH frame: the same `target - reference` convention for the
    // position and the velocity difference (F-35).
    let relative_velocity = match LvlhFrame::from_eci(&target_state) {
        Some(frame) => frame.relative_velocity_in_frame(relative_position, eci_relative_velocity),
        None => eci_relative_velocity,
    };

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
        // Above the WGS-84 equatorial radius, not the render radius (F-28).
        target_altitude_m: target_position.length() - EARTH_EQUATORIAL_RADIUS_M,
    })
}

/// The phase's limits. Metric keeps rc.3's text (the unit is in the key or left
/// out); imperial names its unit on each line.
pub(crate) fn append_state_parameters(
    body: &mut String,
    state: &CompiledCaptureState,
    units: UnitSystem,
) {
    let speed = |m_s: f64| match units {
        UnitSystem::Metric => format!("{m_s:.4}"),
        UnitSystem::Imperial => show(units.speed(m_s), 4),
    };
    writeln!(body, "Phase parameters").unwrap();
    let p = &state.parameters;
    writeln!(body, "- max_velocity: {}", speed(p.max_velocity)).unwrap();
    match units {
        UnitSystem::Metric => writeln!(body, "- max_force_n (bound): {:.4}", p.max_force),
        UnitSystem::Imperial => {
            writeln!(
                body,
                "- max_force_lbf (bound): {:.4}",
                units.force(p.max_force).0
            )
        }
    }
    .unwrap();
    if let Some(sr) = p.shrink_rate {
        writeln!(body, "- shrink_rate: {}", speed(sr)).unwrap();
    }
}

pub(crate) fn append_transitions(
    body: &mut String,
    state: &CompiledCaptureState,
    current_range: Option<f64>,
    current_rel_speed: Option<f64>,
    active_capture: bool,
    units: UnitSystem,
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

    let distance = |m: f64| show(units.length(m), 2);
    let speed = |m_s: f64| show(units.speed(m_s), 2);
    for t in &state.transitions {
        let mut conditions = Vec::new();
        if let Some(limit) = t.distance_less_than {
            conditions.push(format_condition(
                "distance",
                "<",
                limit,
                current_range,
                distance,
            ));
        }
        if let Some(limit) = t.distance_greater_than {
            conditions.push(format_condition(
                "distance",
                ">",
                limit,
                current_range,
                distance,
            ));
        }
        if let Some(limit) = t.relative_velocity_less_than {
            conditions.push(format_condition(
                "relative velocity",
                "<",
                limit,
                current_rel_speed,
                speed,
            ));
        }
        if let Some(limit) = t.relative_velocity_greater_than {
            conditions.push(format_condition(
                "relative velocity",
                ">",
                limit,
                current_rel_speed,
                speed,
            ));
        }
        if conditions.is_empty() {
            writeln!(body, "- {} when conditions are met", t.to).unwrap();
        } else {
            writeln!(body, "- {} when {}", t.to, conditions.join(" and ")).unwrap();
        }
    }
}

/// Readiness compares the metric values; `display` only formats them.
fn format_condition(
    label: &str,
    comparator: &str,
    threshold: f64,
    current_value: Option<f64>,
    display: impl Fn(f64) -> String,
) -> String {
    let threshold_text = display(threshold);
    if let Some(current) = current_value {
        let ready = match comparator {
            "<" => current < threshold,
            ">" => current > threshold,
            _ => false,
        };
        let readiness = if ready { "ready" } else { "waiting" };
        format!(
            "{label} {comparator} {threshold_text} (current {}, {readiness})",
            display(current)
        )
    } else {
        format!("{label} {comparator} {threshold_text}")
    }
}
