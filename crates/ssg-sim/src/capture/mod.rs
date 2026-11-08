//! Capture: the per-step capture controller and its state machine.

pub mod plan;

mod commands;
mod state_machine;

#[cfg(debug_assertions)]
use avian3d::prelude::RigidBody;
use avian3d::{
    math::PI,
    prelude::{
        DistanceJoint, Forces, LinearVelocity, Position, RigidBodyDisabled, RigidBodyQuery,
        Rotation, WriteRigidBodyForces,
    },
};
use bevy::{ecs::system::SystemParam, log::debug_once, math::DVec3, prelude::*};
use brahe::Epoch;

use crate::SimSet;
use crate::SimStep;
use crate::capture::plan::{
    Actuation, CapturePlanLibrary, CompiledCapturePlan, CompiledCaptureStateParameters,
    CompiledCaptureTransition, PhaseEnd, build_capture_component,
};
use crate::orbit::cache::OrbitalCache;
use crate::orbit::lvlh::LvlhFrame;
use crate::physics_bubble::body_eci_state;
use crate::recording::inputs::SimInputLog;
use crate::telemetry::{DataCollection, EphemerisSample, LogEvent, LogLevel};
use crate::tether::chain_from_joints;
use crate::time::{ManualPhysics, PHYS_DT, WorldTime};

// Component to add to an entity to attempt a capture.
// There should only ever be 0 or 1 entity with this component at a time.
#[derive(Component, Debug, Clone)]
pub struct CaptureComponent {
    pub plan_id: String,
    pub current_state: String,
    pub state_enter_time_s: f64,
    pub state_elapsed_time_s: f64,
    /// The capture-sphere radius when the current phase was entered, in m: the
    /// initial radius at the capture start, the current radius at every phase entry
    /// (before that step's shrink). A phase with `velocity_scales_with_sphere`
    /// scales its speed limit by the current radius over this one.
    pub sphere_radius_at_entry_m: f64,
}

/// The capture-sphere radius at the start of every capture (F-11).
pub const INITIAL_CAPTURE_SPHERE_RADIUS_M: f64 = 25.0;

/// The radius the capture sphere stops shrinking at when the plan does not set
/// `capture_sphere.min_radius_m`.
pub const MIN_CAPTURE_SPHERE_RADIUS_M: f64 = 0.1;

/// Seconds the force bound takes to rise from 0 to the phase's `max_force_n`, at
/// the capture start and at every phase entry (F-31, Q10 row 8). A direction
/// reversal does not restart it (C2).
pub const FORCE_RAMP_S: f64 = 1.0;

/// The relative width of the hysteresis band on the controller's three branch
/// thresholds (the `0.8 R` inner edge, the `R` outer edge and the velocity kill),
/// so a body at a threshold does not flip branch every step (F-31). The phase
/// transitions have no band.
///
/// This is the relative width for the two range edges; for the velocity kill the
/// width is at least one step's Δv of the pushed body at the phase's force
/// (`2 × max_force_n × PHYS_DT / m`), because at 2 N on the 2 kg root a 2 % band
/// of a 0.5 m/s limit (0.010 m/s) is narrower than one step (0.0156 m/s) and the
/// branch toggled on 56–97 % of the terminal-phase steps (design D7).
pub const BRANCH_DEAD_BAND: f64 = 0.02;

/// The height above or below the circling plane through the target, in m, at
/// which a phase's `hold_plane` component toward the plane reaches full weight
/// (one unit beside the branch direction); proportional below it (design D16).
pub const PLANE_HOLD_BAND_M: f64 = 1.0;

/// The circle law (design D18): the radius error about a body's sphere, in m, at
/// which its radial term reaches full weight; proportional below it.
pub const RADIUS_HOLD_BAND_M: f64 = 2.0;

/// The circle law (design D18): the radial speed against the sphere's own shrink,
/// in m/s, at which its rate term reaches full weight; proportional below it.
pub const RADIAL_RATE_BAND_MPS: f64 = 0.1;

/// The tail's lag law (design D18): the error in the chord from the root to the
/// tail, in m, at which the tail's tangent weight saturates.
pub const SPACING_BAND_M: f64 = 3.0;

/// The tail's lag law (design D18): the difference in the ends' speeds along their
/// forward tangents, in m/s, at which the rate term saturates the tail's tangent
/// weight.
pub const SPACING_RATE_BAND_MPS: f64 = 0.3;

/// The tail's lag law (design D18): the largest half-angle of arc, in rad (75°),
/// the held chord spans on the tail's sphere, so a chord longer than a small
/// sphere allows is held at 150° of arc behind the root.
pub const SPACING_MAX_HALF_ANGLE: f64 = 75.0_f64.to_radians();

/// The debris body's axis the controller circles the root about: the axis the
/// model's solar arrays span (their booms), so the root circles in the bus's X–Y
/// plane between the arrays and the rope wraps the bus. The debris keeps the
/// identity rotation and the sim has no attitude model, so this is the sim's (ECI)
/// Z at spawn for the whole run: the root circles in a plane parallel to the
/// equatorial plane, not the orbit plane. The axis and the collider boxes in
/// `tether.rs` are fitted to one model and apply to every target in P7.5; a
/// per-target key is P9's (OQ3).
pub const CAPTURE_CIRCLING_AXIS: DVec3 = DVec3::Z;

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
    /// Inside the inner edge (`0.8 R`), with the dead band. Drives no force in a
    /// phase with a shrink rate, where the circle law replaces the branches (design
    /// D18); kept as the branch state.
    pub inner: bool,
    /// Outside the outer edge (`R`), with the dead band. Drives no force in a phase
    /// with a shrink rate (design D18); kept as the branch state.
    pub outer: bool,
    /// Above the velocity limit, with the dead band. Drives no force since the
    /// proportional limiter (design D16); kept as the branch state.
    pub kill: bool,
}

impl CaptureForce {
    /// Updates the three branch states for this step's range `rel_r_len` and speed
    /// `rel_v_len` with the dead band: the inner edge is entered below
    /// `0.8 R (1 − band)` and left above `0.8 R (1 + band)`, the outer edge entered
    /// above `R (1 + band)` and left below `R (1 − band)`, the kill entered above
    /// `max_velocity` and left below `max_velocity − kill_band` (an absolute
    /// width, see [`BRANCH_DEAD_BAND`]).
    fn update_branches(
        &mut self,
        rel_r_len: f64,
        rel_v_len: f64,
        capture_radius: f64,
        max_velocity: f64,
        kill_band: f64,
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
            rel_v_len >= max_velocity - kill_band
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

/// The resources [`capture_state_machine_update`] writes besides the bodies.
#[derive(SystemParam)]
pub struct CaptureOutputs<'w> {
    capture_sphere_radius: ResMut<'w, CaptureSphereRadius>,
    data_collection: ResMut<'w, DataCollection>,
}

/// The read-only data [`capture_state_machine_update`] reads.
#[derive(SystemParam)]
pub struct CaptureInputs<'w, 's> {
    capture_plan_lib: Res<'w, CapturePlanLibrary>,
    orbital_cache: Res<'w, OrbitalCache>,
    world_time: Res<'w, WorldTime>,
    disabled: Query<'w, 's, (), With<RigidBodyDisabled>>,
    /// The tether's joints, walked for `tether_straightness` only.
    joints: Query<'w, 's, &'static DistanceJoint>,
}

/// The `closing_rate_mps` condition's quantity (P9 S3, A-4): the rate at which the
/// range `|rel_r|` shrinks, `rel_r · rel_v / |rel_r|`, with `rel_r` from the body to
/// the target and `rel_v` the body's velocity relative to it; positive while
/// closing. It does not depend on the frame of `rel_v` (`rel_r · (ω × rel_r) = 0`).
/// `None` at a range of 0.
pub(crate) fn closing_rate(rel_r: DVec3, rel_v: DVec3) -> Option<f64> {
    let range = rel_r.length();
    (range > 0.0).then(|| rel_r.dot(rel_v) / range)
}

/// The `tether_straightness` condition's quantity (P9 S3, E4 (b)): the largest
/// distance of a body of `chain` (positions in chain order, root first, tail last)
/// from the root→tail line, divided by the root-to-tail distance; 0 = straight.
/// `None` when the root and the tail are within 1e-6 m.
pub(crate) fn tether_straightness(chain: &[DVec3]) -> Option<f64> {
    let (&root, &tail) = (chain.first()?, chain.last()?);
    let chord = tail - root;
    let length = chord.length();
    if length < 1e-6 {
        return None;
    }
    let axis = chord / length;
    let farthest = chain
        .iter()
        .map(|&body| {
            let offset = body - root;
            (offset - axis * offset.dot(axis)).length()
        })
        .fold(0.0, f64::max);
    Some(farthest / length)
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

/// The unit forward tangent of a body at `rel_r` (from the body to the target)
/// circling about `up`: `up × rel_r`, or `X × rel_r` on the axis.
pub(crate) fn forward_tangent(rel_r: DVec3, up: DVec3) -> DVec3 {
    let tangent_axis = if rel_r.cross(up).length_squared() > 1e-6 {
        up
    } else {
        DVec3::X
    };
    tangent_axis.cross(rel_r).normalize_or_zero()
}

/// The tail's lag law (design D18): the weight of the tail's forward tangent term
/// that holds the chord from the root to the tail at `spacing` behind the root.
/// `root` and `tail` are each end's `(rel_r, rel_v)` (from the end to the target;
/// the LVLH velocity), `tail_radius` the tail's sphere.
///
/// With `a`, `b` the root's and the tail's outward radials projected on the
/// circling plane, `θ = atan2(up · (a × b), a · b)` is the tail's angle behind the
/// root (positive behind), `c_lag = 2 R_t sin(θ / 2)` its chord, the held chord
/// `c* = min(spacing, 2 R_t sin(SPACING_MAX_HALF_ANGLE))` and `Δv` the tail's speed
/// along its forward tangent less the root's along its own:
/// `w = clamp((c_lag − c*) / SPACING_BAND_M − Δv / SPACING_RATE_BAND_MPS, −1, 1)`.
/// The weight does not depend on the radius error: the tail swings to its place
/// while it closes on its sphere.
pub(crate) fn tail_lag_weight(
    root: (DVec3, DVec3),
    tail: (DVec3, DVec3),
    up: DVec3,
    tail_radius: f64,
    spacing: f64,
) -> f64 {
    let ((root_r, root_v), (tail_r, tail_v)) = (root, tail);
    let (out_root, out_tail) = (-root_r, -tail_r);
    let a = out_root - up * out_root.dot(up);
    let b = out_tail - up * out_tail.dot(up);
    let theta = up.dot(a.cross(b)).atan2(a.dot(b));
    let chord = 2.0 * tail_radius * (theta / 2.0).sin();
    let held = spacing.min(2.0 * tail_radius * SPACING_MAX_HALF_ANGLE.sin());
    let dv = tail_v.dot(forward_tangent(tail_r, up)) - root_v.dot(forward_tangent(root_r, up));
    ((chord - held) / SPACING_BAND_M - dv / SPACING_RATE_BAND_MPS).clamp(-1.0, 1.0)
}

/// The circle law (design D18): the composed force direction, before the plane term
/// and the limiter, of a body at `rel_r` (from the body to the target) with LVLH
/// velocity `rel_v` on a sphere of `radius` shrinking at `shrink_rate` m/s, with the
/// body's tangent term `tangent_term` (the forward or counter tangent, or the lag
/// law's weighted one).
///
/// `w_r = clamp((|rel_r| − radius) / RADIUS_HOLD_BAND_M, −1, 1)` holds the radius,
/// `w_v = clamp(v_out / RADIAL_RATE_BAND_MPS, −1, 1)` damps the outward speed
/// measured against the sphere's own shrink (`v_out = −rel_v · r̂_in + shrink_rate`),
/// and the tangent term fades in as the radius error vanishes (`1 − |w_r|`), at full
/// weight under the lag law: `t × w_t + r̂_in × clamp(w_r + w_v, −1, 1)`.
pub(crate) fn circle_law(
    rel_r: DVec3,
    rel_v: DVec3,
    radius: f64,
    shrink_rate: f64,
    tangent_term: DVec3,
    lag: bool,
) -> DVec3 {
    let r_in = rel_r.normalize_or_zero();
    let w_r = ((rel_r.length() - radius) / RADIUS_HOLD_BAND_M).clamp(-1.0, 1.0);
    let v_out = -rel_v.dot(r_in) + shrink_rate;
    let w_v = (v_out / RADIAL_RATE_BAND_MPS).clamp(-1.0, 1.0);
    let w_t = if lag { 1.0 } else { 1.0 - w_r.abs() };
    tangent_term * w_t + r_in * (w_r + w_v).clamp(-1.0, 1.0)
}

pub use commands::{SimCommand, SimCommandInputs, apply_sim_commands};
#[cfg(debug_assertions)]
pub use state_machine::assert_finite_state;
pub use state_machine::capture_state_machine_update;
#[cfg(test)]
pub(crate) use state_machine::{TransitionInputs, transition_satisfied};

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
