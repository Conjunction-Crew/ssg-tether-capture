//! The recorder state machine (R4, R5): manual start and stop, the automatic
//! start on a phase, the automatic stops, the caps and the refusals. The frame
//! tap runs in [`SimStep::Record`](crate::SimStep::Record) after every fixed step;
//! the commands are applied in `Last`, after the step's sim commands.

use avian3d::prelude::{AngularVelocity, LinearVelocity, Position, Rotation};
use bevy::ecs::message::{MessageCursor, Messages};
use bevy::ecs::system::SystemParam;
use bevy::math::DVec3;
use bevy::prelude::*;

use crate::capture::plan::{CapturePlanLibrary, PhaseEnd};
use crate::capture::{CaptureComponent, CaptureEnded, CaptureSphereRadius, CaptureTransition};
use crate::floating_origin::OriginReset;
use crate::orbit::cache::OrbitalCache;
use crate::recording::inputs::{RecordedCommand, SimInputLog};
use crate::recording::{
    BYTES_PER_BODY_FRAME, BodyFrame, EndReason, Frame, RecordedTransition, RecordingBodies,
    RecordingEnd, RecordingHeader, RecordingSink, StartTrigger,
};
use crate::telemetry::{LogEvent, LogLevel};
use crate::time::{FIXED_HZ, PHYS_DT, WorldTime};

/// The per-project recording settings (the `recording` block of `project.json`,
/// R3); every value has the design default (E2, Q16, A-5).
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct RecordingSettings {
    /// Start a recording when the capture enters this phase.
    pub auto_start_phase: Option<String>,
    /// Stop `stop_after_end_s` after the capture enters an end phase.
    pub auto_stop_on_end: bool,
    pub stop_after_end_s: f64,
    /// K: a frame every this many steps.
    pub keyframe_interval_steps: u32,
    pub max_duration_s: f64,
    pub max_size_mb: f64,
    pub max_tether_bodies: u32,
}

impl Default for RecordingSettings {
    fn default() -> Self {
        Self {
            auto_start_phase: None,
            auto_stop_on_end: true,
            stop_after_end_s: 60.0,
            keyframe_interval_steps: 32,
            max_duration_s: 1800.0,
            max_size_mb: 100.0,
            max_tether_bodies: 200,
        }
    }
}

/// Commands from the app or the CLI.
#[derive(Message, Debug, Clone, PartialEq)]
pub enum RecorderCommand {
    Start { name: String },
    Stop,
}

/// What the recorder did; each also is a log line (source `recording`).
#[derive(Message, Debug, Clone, PartialEq)]
pub enum RecorderEvent {
    Started {
        step: u64,
        trigger: StartTrigger,
    },
    Stopped {
        end: RecordingEnd,
    },
    /// 80 % of the duration or the size cap.
    Warning(String),
    Refused(String),
}

/// The running recording.
#[derive(Debug, Clone, PartialEq)]
pub struct ActiveRecording {
    pub start_step: u64,
    pub last_frame_step: u64,
    pub frames: u64,
    /// The step and outcome of the first `CaptureEnded` at or after the start.
    pub end_event: Option<(u64, PhaseEnd)>,
    // The settings taken at the start.
    interval: u64,
    max_steps: u64,
    max_bytes: u64,
    stop_after_end_steps: Option<u64>,
    // Resets since the last frame.
    shift_m: DVec3,
    shift_mps: DVec3,
    warned_duration: bool,
    warned_size: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub enum RecorderState {
    #[default]
    Idle,
    Recording(ActiveRecording),
}

#[derive(Resource, Debug, Default)]
pub struct Recorder {
    pub state: RecorderState,
    /// Shared by the tap and the `Last` system, so each `CaptureEnded` is seen once.
    ended: MessageCursor<CaptureEnded>,
    /// The [`SimInputLog`] entries already checked for an automatic start.
    inputs_seen: usize,
}

impl Recorder {
    pub fn is_recording(&self) -> bool {
        matches!(self.state, RecorderState::Recording(_))
    }
}

/// Reads the state the frames hold; never writes.
#[derive(SystemParam)]
pub struct FrameReader<'w, 's> {
    bodies: Option<Res<'w, RecordingBodies>>,
    states: Query<
        'w,
        's,
        (
            &'static Position,
            &'static Rotation,
            &'static LinearVelocity,
            &'static AngularVelocity,
        ),
    >,
    captures: Query<'w, 's, &'static CaptureComponent>,
    cache: Option<Res<'w, OrbitalCache>>,
    radius: Option<Res<'w, CaptureSphereRadius>>,
    world_time: Option<Res<'w, WorldTime>>,
}

impl FrameReader<'_, '_> {
    /// The last completed fixed step.
    fn step(&self) -> Option<u64> {
        let t = self.world_time.as_ref()?;
        Some(((t.epoch - t.start_epoch) / PHYS_DT).round() as u64)
    }

    fn step_of(&self, epoch: brahe::Epoch) -> u64 {
        self.world_time
            .as_ref()
            .map_or(0, |t| ((epoch - t.start_epoch) / PHYS_DT).round() as u64)
    }

    fn target(&self) -> Option<Entity> {
        self.bodies.as_ref()?.entities.first().copied()
    }

    /// The frame at the end of `step`; `None` if a body or the clock is missing.
    fn frame(&self, step: u64, shift_m: DVec3, shift_mps: DVec3) -> Option<Frame> {
        let bodies = self.bodies.as_ref()?;
        let world_time = self.world_time.as_ref()?;
        let states = bodies
            .entities
            .iter()
            .map(|&entity| {
                let (p, r, v, w) = self.states.get(entity).ok()?;
                Some(BodyFrame {
                    position: p.0,
                    rotation: r.0,
                    velocity: v.0,
                    angular_velocity: w.0,
                })
            })
            .collect::<Option<Vec<_>>>()?;
        // The tether root (index 1) is the anchor, the frame origin (Q15).
        let anchor = bodies.entities.get(1)?;
        Some(Frame {
            step,
            epoch: world_time.epoch,
            phase: self
                .captures
                .get(bodies.entities[0])
                .ok()
                .map(|c| c.current_state.clone()),
            capture_sphere_radius_m: self.radius.as_ref().map_or(0.0, |r| r.radius),
            frame_origin_eci: self
                .cache
                .as_ref()
                .and_then(|c| c.eci_states.get(anchor).copied()),
            origin_shift_m: shift_m,
            origin_shift_mps: shift_mps,
            bodies: states,
        })
    }
}

/// The recorder's messages and log lines.
#[derive(SystemParam)]
pub struct RecorderOutput<'w> {
    events: MessageWriter<'w, RecorderEvent>,
    log: MessageWriter<'w, LogEvent>,
}

impl RecorderOutput<'_> {
    fn emit(&mut self, level: LogLevel, event: RecorderEvent, message: String) {
        self.log.write(LogEvent {
            level,
            source: "recording",
            message,
        });
        self.events.write(event);
    }
}

/// Everything a recorder action needs, borrowed from one system's parameters.
struct Ctx<'a, 'w, 's, 'o> {
    recorder: &'a mut Recorder,
    sink: &'a mut RecordingSink,
    reader: &'a FrameReader<'w, 's>,
    out: &'a mut RecorderOutput<'o>,
}

impl Ctx<'_, '_, '_, '_> {
    /// Starts a recording at `step` with a frame there, or refuses (R4).
    fn start(
        &mut self,
        settings: &RecordingSettings,
        step: u64,
        name: Option<String>,
        trigger: StartTrigger,
        start_phase: Option<String>,
    ) {
        let refuse = |out: &mut RecorderOutput, why: String| {
            out.emit(
                LogLevel::Warn,
                RecorderEvent::Refused(why.clone()),
                format!("Recording refused: {why}"),
            );
        };
        if self.recorder.is_recording() {
            return refuse(self.out, "a recording is already running".into());
        }
        let Some(bodies) = self.reader.bodies.as_ref() else {
            return refuse(self.out, "no bodies to record".into());
        };
        let tether = bodies.tether_bodies();
        if tether > settings.max_tether_bodies as usize {
            return refuse(
                self.out,
                format!(
                    "{tether} tether bodies; at most {} may be recorded",
                    settings.max_tether_bodies
                ),
            );
        }
        let body_count = bodies.entities.len() as u64;
        let frame_bytes = body_count * BYTES_PER_BODY_FRAME;
        let max_bytes = (settings.max_size_mb * 1e6) as u64;
        if frame_bytes > max_bytes {
            return refuse(self.out, "max_size_mb is smaller than one frame".into());
        }
        let Some(frame) = self.reader.frame(step, DVec3::ZERO, DVec3::ZERO) else {
            return refuse(self.out, "a recorded body has no state".into());
        };
        let interval = u64::from(settings.keyframe_interval_steps.max(1));
        let max_steps = (settings.max_duration_s * FIXED_HZ).ceil() as u64;
        self.sink.0.begin(&RecordingHeader {
            name,
            start_step: step,
            keyframe_interval_steps: interval as u32,
            trigger,
            start_phase,
            roles: bodies.roles.clone(),
        });
        self.sink.0.frame(&frame);
        self.recorder.state = RecorderState::Recording(ActiveRecording {
            start_step: step,
            last_frame_step: step,
            frames: 1,
            end_event: None,
            interval,
            max_steps,
            max_bytes,
            stop_after_end_steps: settings
                .auto_stop_on_end
                .then(|| (settings.stop_after_end_s * FIXED_HZ).round() as u64),
            shift_m: DVec3::ZERO,
            shift_mps: DVec3::ZERO,
            warned_duration: false,
            warned_size: false,
        });
        // Projected size at the duration cap (R8).
        let projected_mb = (max_steps / interval + 1) as f64 * frame_bytes as f64 / 1e6;
        self.out.emit(
            LogLevel::Info,
            RecorderEvent::Started { step, trigger },
            format!(
                "Recording started at step {step}: {body_count} bodies, a frame every {interval} \
                 steps, {projected_mb:.1} MB at the {:.0} s cap",
                settings.max_duration_s
            ),
        );
    }

    /// Writes a frame unless it would take the recording over `max_size_mb`
    /// (then `false`).
    fn push_frame(&mut self, frame: Frame) -> bool {
        let RecorderState::Recording(active) = &mut self.recorder.state else {
            return false;
        };
        let frame_bytes = frame.bodies.len() as u64 * BYTES_PER_BODY_FRAME;
        let bytes = self.sink.0.bytes() + frame_bytes;
        if bytes > active.max_bytes {
            return false;
        }
        self.sink.0.frame(&frame);
        active.last_frame_step = frame.step;
        active.frames += 1;
        active.shift_m = DVec3::ZERO;
        active.shift_mps = DVec3::ZERO;
        if !active.warned_size && bytes * 5 >= active.max_bytes * 4 {
            active.warned_size = true;
            let message = format!("Recording at 80 % of its size cap ({bytes} bytes)");
            self.out.emit(
                LogLevel::Warn,
                RecorderEvent::Warning(message.clone()),
                message,
            );
        }
        true
    }

    /// Frames, caps and automatic stops at the end of `step`.
    fn after_step(&mut self, step: u64) {
        let RecorderState::Recording(active) = &mut self.recorder.state else {
            return;
        };
        let elapsed = step - active.start_step;
        if !active.warned_duration && elapsed * 5 >= active.max_steps * 4 {
            active.warned_duration = true;
            let message = format!("Recording at 80 % of its duration cap (step {step})");
            self.out.emit(
                LogLevel::Warn,
                RecorderEvent::Warning(message.clone()),
                message,
            );
        }
        let stop = if elapsed >= active.max_steps {
            Some(EndReason::MaxDuration)
        } else {
            match (active.end_event, active.stop_after_end_steps) {
                (Some((at, outcome)), Some(delay)) if step >= at + delay => {
                    Some(end_reason(outcome))
                }
                _ => None,
            }
        };
        if let Some(reason) = stop {
            self.stop(step, reason);
        } else if elapsed > 0 && elapsed.is_multiple_of(active.interval) {
            let (shift_m, shift_mps) = (active.shift_m, active.shift_mps);
            let written = self
                .reader
                .frame(step, shift_m, shift_mps)
                .is_some_and(|frame| self.push_frame(frame));
            if !written {
                self.finish(EndReason::MaxSize);
            }
        }
    }

    /// Ends the recording with a last frame at `step` (when it fits and it is not
    /// already one).
    fn stop(&mut self, step: u64, reason: EndReason) {
        let RecorderState::Recording(active) = &self.recorder.state else {
            return;
        };
        if active.last_frame_step != step {
            let (shift_m, shift_mps) = (active.shift_m, active.shift_mps);
            if let Some(frame) = self.reader.frame(step, shift_m, shift_mps)
                && !self.push_frame(frame)
            {
                return self.finish(EndReason::MaxSize);
            }
        }
        self.finish(reason);
    }

    /// Ends the recording at its last frame.
    fn finish(&mut self, reason: EndReason) {
        let RecorderState::Recording(active) = std::mem::take(&mut self.recorder.state) else {
            return;
        };
        let end = RecordingEnd {
            end_step: active.last_frame_step,
            reason,
        };
        self.sink.0.finish(&end);
        self.out.emit(
            LogLevel::Info,
            RecorderEvent::Stopped { end },
            format!(
                "Recording stopped at step {} ({reason:?}): {} frames",
                end.end_step, active.frames
            ),
        );
    }

    /// Notes the first `CaptureEnded` at or after the start.
    fn read_ended(&mut self, messages: &Messages<CaptureEnded>) {
        let events: Vec<(u64, PhaseEnd)> = self
            .recorder
            .ended
            .read(messages)
            .map(|e| (self.reader.step_of(e.epoch), e.outcome))
            .collect();
        if let RecorderState::Recording(active) = &mut self.recorder.state
            && active.end_event.is_none()
        {
            active.end_event = events.into_iter().find(|(at, _)| *at >= active.start_step);
        }
    }
}

/// The frame tap: runs after every fixed step, reads the step's end state and the
/// step's resets, transitions and end events (R5).
pub fn record_step(
    mut recorder: ResMut<Recorder>,
    settings: Res<RecordingSettings>,
    mut sink: ResMut<RecordingSink>,
    reader: FrameReader,
    (mut transitions, mut resets, ended): (
        MessageReader<CaptureTransition>,
        MessageReader<OriginReset>,
        Res<Messages<CaptureEnded>>,
    ),
    mut out: RecorderOutput,
) {
    let Some(step) = reader.step() else {
        return;
    };
    let (mut reset_m, mut reset_mps) = (DVec3::ZERO, DVec3::ZERO);
    for reset in resets.read() {
        reset_m += reset.com_r;
        reset_mps += reset.com_v;
    }
    let target = reader.target();
    let transitions: Vec<CaptureTransition> = transitions
        .read()
        .filter(|t| Some(t.entity) == target)
        .cloned()
        .collect();
    let mut ctx = Ctx {
        recorder: &mut recorder,
        sink: &mut sink,
        reader: &reader,
        out: &mut out,
    };
    if let Some(phase) = &settings.auto_start_phase
        && transitions.iter().any(|t| &t.to == phase)
    {
        ctx.start(
            &settings,
            step,
            None,
            StartTrigger::Phase,
            Some(phase.clone()),
        );
    }
    if let RecorderState::Recording(active) = &mut ctx.recorder.state {
        // A reset in the start step is already in the first frame.
        if step > active.start_step {
            active.shift_m += reset_m;
            active.shift_mps += reset_mps;
        }
        for t in &transitions {
            ctx.sink.0.transition(&RecordedTransition {
                step: reader.step_of(t.epoch),
                epoch: t.epoch,
                from: t.from.clone(),
                to: t.to.clone(),
                reason: t.reason.clone(),
            });
        }
    }
    ctx.read_ended(&ended);
    ctx.after_step(step);
}

/// Applies [`RecorderCommand`]s and the automatic start at a `StartCapture` whose
/// first phase is `auto_start_phase`, at the current step (`Last`, after the sim
/// commands).
pub fn apply_recorder_commands(
    mut recorder: ResMut<Recorder>,
    settings: Res<RecordingSettings>,
    mut sink: ResMut<RecordingSink>,
    reader: FrameReader,
    (mut commands, ended, input_log, library): (
        MessageReader<RecorderCommand>,
        Res<Messages<CaptureEnded>>,
        Res<SimInputLog>,
        Res<CapturePlanLibrary>,
    ),
    mut out: RecorderOutput,
) {
    let Some(step) = reader.step() else {
        return;
    };
    let new_inputs = &input_log.entries[recorder.inputs_seen.min(input_log.entries.len())..];
    let auto_start = settings.auto_start_phase.as_ref().filter(|phase| {
        new_inputs.iter().any(|input| match &input.command {
            RecordedCommand::StartCapture { plan_id, .. } => library
                .plans
                .get(plan_id)
                .and_then(|plan| plan.phases.first())
                .is_some_and(|first| &first.id == *phase),
            RecordedCommand::SessionStart { .. } => false,
        })
    });
    let auto_start = auto_start.cloned();
    recorder.inputs_seen = input_log.entries.len();
    let mut ctx = Ctx {
        recorder: &mut recorder,
        sink: &mut sink,
        reader: &reader,
        out: &mut out,
    };
    for command in commands.read() {
        match command {
            RecorderCommand::Start { name } => {
                ctx.start(
                    &settings,
                    step,
                    Some(name.clone()),
                    StartTrigger::Manual,
                    None,
                );
            }
            RecorderCommand::Stop => ctx.stop(step, EndReason::Manual),
        }
    }
    if let Some(phase) = auto_start {
        ctx.start(&settings, step, None, StartTrigger::Phase, Some(phase));
    }
    ctx.read_ended(&ended);
    // Only the end-event stop can fall due here (an end phase entered at the start).
    if let RecorderState::Recording(active) = &ctx.recorder.state
        && let (Some((at, outcome)), Some(delay)) = (active.end_event, active.stop_after_end_steps)
        && step >= at + delay
    {
        ctx.stop(step, end_reason(outcome));
    }
}

fn end_reason(outcome: PhaseEnd) -> EndReason {
    match outcome {
        PhaseEnd::Success => EndReason::EndSuccess,
        PhaseEnd::Abort => EndReason::EndAbort,
    }
}

/// Leaving the sim screen ends a running recording (`session_end`), with a last
/// frame when the bodies are still there.
pub fn end_recording_on_exit(
    mut recorder: ResMut<Recorder>,
    mut sink: ResMut<RecordingSink>,
    reader: FrameReader,
    mut out: RecorderOutput,
) {
    let step = reader.step();
    let mut ctx = Ctx {
        recorder: &mut recorder,
        sink: &mut sink,
        reader: &reader,
        out: &mut out,
    };
    match step {
        Some(step) => ctx.stop(step, EndReason::SessionEnd),
        None => ctx.finish(EndReason::SessionEnd),
    }
}

/// A new session starts idle.
pub(crate) fn reset_recorder(mut recorder: ResMut<Recorder>) {
    recorder.state = RecorderState::Idle;
    recorder.inputs_seen = 0;
}
