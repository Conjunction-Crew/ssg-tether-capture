//! Playback of a recording (P7, design R6): a cursor over the recorded steps, the
//! state at the cursor, play, pause, step, seek, speeds and reverse. Pure: it reads
//! the frames and the transitions and never runs physics, so playback is the same
//! on every platform.
//!
//! Between two frames A and B the player interpolates origin-aware (C2): it first
//! moves A into B's frame (every body's position and linear velocity minus B's
//! `origin_shift_m` / `origin_shift_mps`, A's `frame_origin_eci` plus the shift),
//! then interpolates positions and velocities linearly and rotations by normalised
//! linear interpolation. The phase is exact (from the transitions, C5).

use bevy::math::{DQuat, DVec3};

use crate::recording::BodyFrame;
use crate::recording::files::{LoadedRecording, RecordedFrame, TransitionLine};
use crate::time::FIXED_HZ;

/// The playback speeds, in sim seconds per wall second (R6).
pub const PLAYBACK_SPEEDS: [f64; 10] = [0.125, 0.25, 0.5, 1.0, 2.0, 4.0, 8.0, 16.0, 32.0, 64.0];

/// The state shown at a cursor.
#[derive(Debug, Clone, PartialEq)]
pub struct FrameView {
    /// The cursor (a sim step).
    pub step: f64,
    /// One entry per body, in the recording's body order.
    pub bodies: Vec<BodyFrame>,
    /// `false` at a frame, `true` between two frames.
    pub interpolated: bool,
    pub phase: Option<String>,
    pub radius: f64,
    /// The frame origin's ECI state; `None` when a frame it comes from has none.
    pub frame_origin_eci: Option<[f64; 6]>,
}

/// A recording being played. The cursor stays inside `[first frame, last frame]`.
#[derive(Debug, Clone)]
pub struct Player {
    frames: Vec<RecordedFrame>,
    /// `None` for a recording without `transitions.jsonl`.
    transitions: Option<Vec<TransitionLine>>,
    cursor: f64,
    pub playing: bool,
    /// Sim seconds per wall second (one of [`PLAYBACK_SPEEDS`] in the app).
    pub speed: f64,
    pub reverse: bool,
}

impl Player {
    /// A player at the first frame, paused, at 1×. `None` without frames. Frames
    /// must be in step order (as `read_recording` returns them).
    pub fn new(
        frames: Vec<RecordedFrame>,
        transitions: Option<Vec<TransitionLine>>,
    ) -> Option<Self> {
        let cursor = frames.first()?.line.step as f64;
        Some(Self {
            frames,
            transitions,
            cursor,
            playing: false,
            speed: 1.0,
            reverse: false,
        })
    }

    /// A player for a recording read from its folder; `has_transitions` says whether
    /// it had `transitions.jsonl` (P7 always writes it).
    pub fn from_recording(recording: &LoadedRecording, has_transitions: bool) -> Option<Self> {
        Self::new(
            recording.frames.clone(),
            has_transitions.then(|| recording.transitions.clone()),
        )
    }

    /// The first frame's step.
    pub fn start_step(&self) -> u64 {
        self.frames[0].line.step
    }

    /// The last frame's step.
    pub fn end_step(&self) -> u64 {
        self.frames[self.frames.len() - 1].line.step
    }

    pub fn cursor(&self) -> f64 {
        self.cursor
    }

    /// Moves the cursor to `step`, clamped to the recording.
    pub fn seek(&mut self, step: f64) {
        let (lo, hi) = (self.start_step() as f64, self.end_step() as f64);
        self.cursor = if step.is_nan() {
            lo
        } else {
            step.clamp(lo, hi)
        };
    }

    /// Advances a playing cursor by `wall_seconds × speed` sim seconds, backwards
    /// when `reverse`; play stops at either end.
    pub fn advance(&mut self, wall_seconds: f64) {
        if !self.playing {
            return;
        }
        let steps = wall_seconds * self.speed * FIXED_HZ;
        let target = if self.reverse {
            self.cursor - steps
        } else {
            self.cursor + steps
        };
        self.seek(target);
        let at_end = if self.reverse {
            self.cursor <= self.start_step() as f64
        } else {
            self.cursor >= self.end_step() as f64
        };
        if at_end {
            self.playing = false;
        }
    }

    /// Pauses and moves to the next (`+1`) or previous (`-1`) frame, never to an
    /// interpolated state; stays at either end.
    pub fn step(&mut self, direction: i32) {
        self.playing = false;
        let next = if direction >= 0 {
            self.frames
                .iter()
                .map(|f| f.line.step as f64)
                .find(|&s| s > self.cursor)
        } else {
            self.frames
                .iter()
                .rev()
                .map(|f| f.line.step as f64)
                .find(|&s| s < self.cursor)
        };
        if let Some(step) = next {
            self.cursor = step;
        }
    }

    /// The state at the player's cursor.
    pub fn view(&self) -> FrameView {
        self.state_at(self.cursor)
    }

    /// The state at `cursor` (clamped to the recording): the frame there, or the
    /// origin-aware interpolation of the frames around it.
    pub fn state_at(&self, cursor: f64) -> FrameView {
        let (lo, hi) = (self.start_step() as f64, self.end_step() as f64);
        let cursor = if cursor.is_nan() {
            lo
        } else {
            cursor.clamp(lo, hi)
        };
        // The first frame after the cursor; the cursor lies in [before, after).
        let after = self
            .frames
            .partition_point(|f| (f.line.step as f64) <= cursor);
        let a = &self.frames[after - 1];
        if a.line.step as f64 == cursor || after == self.frames.len() {
            return FrameView {
                step: cursor,
                bodies: a.bodies.clone(),
                interpolated: false,
                phase: self.phase_at(cursor, a),
                radius: a.line.capture_sphere_radius_m,
                frame_origin_eci: a.line.frame_origin_eci,
            };
        }
        let b = &self.frames[after];
        let (sa, sb) = (a.line.step as f64, b.line.step as f64);
        let t = (cursor - sa) / (sb - sa);
        let shift_m = DVec3::from_array(b.line.origin_shift_m);
        let shift_mps = DVec3::from_array(b.line.origin_shift_mps);
        let bodies = a
            .bodies
            .iter()
            .zip(&b.bodies)
            .map(|(a, b)| BodyFrame {
                position: (a.position - shift_m).lerp(b.position, t),
                rotation: nlerp(a.rotation, b.rotation, t),
                velocity: (a.velocity - shift_mps).lerp(b.velocity, t),
                angular_velocity: a.angular_velocity.lerp(b.angular_velocity, t),
            })
            .collect();
        let frame_origin_eci = match (a.line.frame_origin_eci, b.line.frame_origin_eci) {
            (Some(oa), Some(ob)) => {
                let shift = [shift_m.to_array(), shift_mps.to_array()].concat();
                Some(std::array::from_fn(|i| {
                    let from = oa[i] + shift[i];
                    from + (ob[i] - from) * t
                }))
            }
            _ => None,
        };
        FrameView {
            step: cursor,
            bodies,
            interpolated: true,
            phase: self.phase_at(cursor, a),
            radius: self.radius_between(cursor, a, b, t),
            frame_origin_eci,
        }
    }

    /// The `to` of the last transition at or before `cursor`, else the first
    /// frame's phase; without `transitions.jsonl`, the phase of `before`.
    fn phase_at(&self, cursor: f64, before: &RecordedFrame) -> Option<String> {
        let Some(transitions) = &self.transitions else {
            return before.line.phase.clone();
        };
        transitions
            .iter()
            .rev()
            .find(|t| t.step as f64 <= cursor)
            .map_or_else(|| self.frames[0].line.phase.clone(), |t| Some(t.to.clone()))
    }

    /// The sphere radius between frames `a` and `b`: linear when no transition lies
    /// in `(a, b]`, else the radius of the frame on the cursor's side of the
    /// transition; without `transitions.jsonl`, the radius of `a`.
    fn radius_between(&self, cursor: f64, a: &RecordedFrame, b: &RecordedFrame, t: f64) -> f64 {
        let (ra, rb) = (
            a.line.capture_sphere_radius_m,
            b.line.capture_sphere_radius_m,
        );
        let Some(transitions) = &self.transitions else {
            return ra;
        };
        match transitions
            .iter()
            .find(|tr| tr.step > a.line.step && tr.step <= b.line.step)
        {
            None => ra + (rb - ra) * t,
            Some(first) if cursor < first.step as f64 => ra,
            Some(_) => rb,
        }
    }
}

/// Normalised linear interpolation along the shorter arc.
fn nlerp(a: DQuat, b: DQuat, t: f64) -> DQuat {
    let b = if a.dot(b) < 0.0 { -b } else { b };
    (a * (1.0 - t) + b * t).normalize()
}
