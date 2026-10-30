//! Recording (P7): the session's input log, the read-only frame tap and the
//! recorder state machine. A recording holds full-state frames of every recorded
//! body every K steps and at its first and last step, the capture transitions
//! inside it, and the session's input log from step 0 (design R1). Frames go to a
//! [`FrameSink`]; this module has an in-memory one, the file sink writes the
//! `recordings/<name>/` layout.
//!
//! The recorder never writes a physics component, never inserts a component on a
//! body, uses no change-detection filter and no `Res<Time>`, and keys everything
//! by fixed step, never by update (R5, C8): a run with a recording is
//! bit-identical to the run without.

pub mod inputs;
pub mod recorder;

use std::sync::{Arc, Mutex, PoisonError};

use avian3d::prelude::DistanceJoint;
use bevy::math::{DQuat, DVec3};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use brahe::Epoch;
use nalgebra::Vector6;

use crate::capture::plan::CapturePlanLibrary;
use crate::orbit::cache::OrbitalCache;
use crate::project::SelectedProject;
use crate::project::recording::RecordingBodyRole;
use crate::session::{SimState, UiScreen};
use crate::{SimSet, SimStep};
use inputs::{SimInputLog, reset_sim_input_log};
use recorder::{
    Recorder, RecorderCommand, RecorderEvent, RecordingSettings, apply_recorder_commands,
    end_recording_on_exit, record_step, reset_recorder,
};

/// Bytes of one body in one frame of the keyframe table: `step` (8), `body` (4)
/// and 13 doubles (R3).
pub const BYTES_PER_BODY_FRAME: u64 = 116;

/// One body's end-of-step state, in the bubble frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BodyFrame {
    pub position: DVec3,
    pub rotation: DQuat,
    pub velocity: DVec3,
    pub angular_velocity: DVec3,
}

/// The state at the end of fixed step `step` (R1, R3).
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    pub step: u64,
    pub epoch: Epoch,
    /// The capture phase of the target; `None` before a capture starts.
    pub phase: Option<String>,
    pub capture_sphere_radius_m: f64,
    /// The frame origin's ECI state, `OrbitalCache.eci_states[anchor]` (C7);
    /// `None` before the anchor has a propagator.
    pub frame_origin_eci: Option<Vector6<f64>>,
    /// The sum of the floating-origin resets after the previous frame up to and
    /// including this step (C2); zero in the first frame.
    pub origin_shift_m: DVec3,
    pub origin_shift_mps: DVec3,
    /// One entry per body, in [`RecordingBodies`] order.
    pub bodies: Vec<BodyFrame>,
}

/// A capture transition inside the segment (C5).
#[derive(Debug, Clone, PartialEq)]
pub struct RecordedTransition {
    pub step: u64,
    pub epoch: Epoch,
    pub from: String,
    pub to: String,
    pub reason: String,
}

/// What started a recording.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartTrigger {
    Manual,
    /// The capture entered `RecordingSettings::auto_start_phase`.
    Phase,
}

/// Why a recording ended (R3 `end_reason`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndReason {
    Manual,
    EndSuccess,
    EndAbort,
    MaxDuration,
    MaxSize,
    SessionEnd,
}

/// What a sink is told when a recording starts.
#[derive(Debug, Clone, PartialEq)]
pub struct RecordingHeader {
    /// The caller's name (`RecorderCommand::Start`); `None` for an automatic start.
    pub name: Option<String>,
    pub start_step: u64,
    pub keyframe_interval_steps: u32,
    pub trigger: StartTrigger,
    /// The phase at the start for [`StartTrigger::Phase`].
    pub start_phase: Option<String>,
    pub roles: Vec<RecordingBodyRole>,
}

/// What a sink is told when a recording ends: the last frame's step and the reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordingEnd {
    pub end_step: u64,
    pub reason: EndReason,
}

/// Where a recording goes. Called outside nothing the step reads.
pub trait FrameSink: Send + Sync {
    fn begin(&mut self, header: &RecordingHeader);
    fn frame(&mut self, frame: &Frame);
    fn transition(&mut self, transition: &RecordedTransition);
    /// Bytes of the current recording so far (checked against `max_size_mb`).
    fn bytes(&self) -> u64;
    fn finish(&mut self, end: &RecordingEnd);
}

/// The sink the recorder writes to; [`MemorySink`] unless the caller installs another.
#[derive(Resource)]
pub struct RecordingSink(pub Box<dyn FrameSink>);

impl Default for RecordingSink {
    fn default() -> Self {
        Self(Box::new(MemorySink::default()))
    }
}

/// One recording held by a [`MemorySink`].
#[derive(Debug, Clone, PartialEq)]
pub struct MemorySegment {
    pub header: RecordingHeader,
    pub frames: Vec<Frame>,
    pub transitions: Vec<RecordedTransition>,
    /// `None` while the recording runs.
    pub end: Option<RecordingEnd>,
}

/// Keeps every recording in memory. Clones share the segments, so a test keeps a
/// clone and installs the other.
#[derive(Debug, Clone, Default)]
pub struct MemorySink {
    segments: Arc<Mutex<Vec<MemorySegment>>>,
}

impl MemorySink {
    /// The recordings so far, oldest first.
    pub fn segments(&self) -> Vec<MemorySegment> {
        self.lock().clone()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<MemorySegment>> {
        self.segments.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn with_current(&self, f: impl FnOnce(&mut MemorySegment)) {
        if let Some(segment) = self.lock().last_mut() {
            f(segment);
        }
    }
}

impl FrameSink for MemorySink {
    fn begin(&mut self, header: &RecordingHeader) {
        self.lock().push(MemorySegment {
            header: header.clone(),
            frames: Vec::new(),
            transitions: Vec::new(),
            end: None,
        });
    }

    fn frame(&mut self, frame: &Frame) {
        self.with_current(|s| s.frames.push(frame.clone()));
    }

    fn transition(&mut self, transition: &RecordedTransition) {
        self.with_current(|s| s.transitions.push(transition.clone()));
    }

    fn bytes(&self) -> u64 {
        self.lock().last().map_or(0, |s| {
            s.frames.len() as u64 * s.header.roles.len() as u64 * BYTES_PER_BODY_FRAME
        })
    }

    fn finish(&mut self, end: &RecordingEnd) {
        self.with_current(|s| s.end = Some(*end));
    }
}

/// The recorded bodies (R5): index 0 the target, then the tether root, the
/// interior nodes in spawn order and the tail. A resource built at spawn, never a
/// component on a body (C8).
#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct RecordingBodies {
    pub entities: Vec<Entity>,
    pub roles: Vec<RecordingBodyRole>,
}

impl RecordingBodies {
    /// Bodies other than the target.
    pub fn tether_bodies(&self) -> usize {
        self.roles
            .iter()
            .filter(|role| **role != RecordingBodyRole::Target)
            .count()
    }
}

/// Builds [`RecordingBodies`] from the bodies `spawn_selected_bodies` spawned:
/// the target `OrbitalCache.debris["Satellite1"]`, then the tether from its root
/// `OrbitalCache.tethers[plan.tether][0]` along the chain of distance joints
/// (spawn order) to the tail. `OrbitalCache.tethers` holds only the root and the
/// tail, so the interior nodes come from the joints (read only).
pub fn build_recording_bodies(
    mut commands: Commands,
    cache: Res<OrbitalCache>,
    selected_project: Res<SelectedProject>,
    capture_plan_lib: Res<CapturePlanLibrary>,
    joints: Query<&DistanceJoint>,
    mut input_log: ResMut<SimInputLog>,
) {
    // The tether name `spawn_tether` uses.
    let tether_name = selected_project
        .project_id
        .as_deref()
        .and_then(|id| capture_plan_lib.plans.get(id))
        .map_or("Tether1", |plan| plan.tether.as_str());
    let (Some(&target), Some(&root)) = (
        cache.debris.get("Satellite1"),
        cache
            .tethers
            .get(tether_name)
            .and_then(|nodes| nodes.first()),
    ) else {
        return;
    };
    let next: HashMap<Entity, Entity> = joints.iter().map(|j| (j.body1, j.body2)).collect();
    let mut tether = vec![root];
    while let Some(&node) = next.get(tether.last().expect("the root is in the chain")) {
        if tether.contains(&node) {
            break;
        }
        tether.push(node);
    }
    let mut bodies = RecordingBodies {
        entities: vec![target],
        roles: vec![RecordingBodyRole::Target],
    };
    let last = tether.len() - 1;
    for (i, node) in tether.into_iter().enumerate() {
        bodies.entities.push(node);
        bodies.roles.push(match i {
            0 => RecordingBodyRole::TetherRoot,
            i if i == last => RecordingBodyRole::TetherTail,
            _ => RecordingBodyRole::TetherNode,
        });
    }
    input_log.target = Some(target);
    commands.insert_resource(bodies);
}

fn remove_recording_bodies(mut commands: Commands) {
    commands.remove_resource::<RecordingBodies>();
}

pub struct RecordingPlugin;

impl Plugin for RecordingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SimInputLog>()
            .init_resource::<Recorder>()
            .init_resource::<RecordingSettings>()
            .init_resource::<RecordingSink>()
            .add_message::<RecorderCommand>()
            .add_message::<RecorderEvent>()
            .add_systems(
                OnEnter(UiScreen::Sim),
                (reset_sim_input_log, reset_recorder).before(SimSet::SessionInit),
            )
            .add_systems(
                OnEnter(SimState::Running),
                build_recording_bodies.after(SimSet::SpawnBodies),
            )
            .add_systems(
                OnExit(UiScreen::Sim),
                (end_recording_on_exit, remove_recording_bodies).chain(),
            )
            .add_systems(
                crate::time::ManualPhysics,
                record_step.in_set(SimStep::Record),
            )
            .add_systems(
                Last,
                apply_recorder_commands
                    .after(SimSet::ApplyCommands)
                    .run_if(in_state(UiScreen::Sim)),
            );
    }
}
