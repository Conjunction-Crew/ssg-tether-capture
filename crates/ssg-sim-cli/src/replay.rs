//! `ssg-sim replay <recording-dir>` (P7, design R7): checks a recording's files,
//! then re-simulates it from step 0 with its scenario, its plan and its input log
//! and compares every frame's 13 columns of every body bit for bit.
//!
//! The driver reproduces the session's timing: the update that spawns the bodies
//! runs k fixed steps (the `SessionStart` entry, C1: `WorldTime.multiplier = k`
//! and `StepCap = max(64, k)`, or paused for k = 0); after it every update runs one
//! step, and each input is applied after exactly the step it was recorded after
//! (an input applied in an update that ran no step gets a paused update). The
//! determinism contract makes this equal to the recorded run however many steps
//! its updates ran.

use std::path::Path;

use avian3d::prelude::{AngularVelocity, LinearVelocity, Position, Rotation};
use bevy::prelude::*;
use ssg_sim::capture::SimCommand;
use ssg_sim::capture::plan::CapturePlanLibrary;
use ssg_sim::orbit::selection::OrbitalSelectionState;
use ssg_sim::project::SelectedProject;
use ssg_sim::project::manifest::BuiltWith;
use ssg_sim::project::recording::{RecordingBodyRole, RecordingSource};
use ssg_sim::recording::files::{
    KeyframeRow, LoadedRecording, RecordedFrame, check_recording, read_recording,
};
use ssg_sim::recording::inputs::{RecordedCommand, RecordedInput};
use ssg_sim::recording::{BodyFrame, RecordingBodies};
use ssg_sim::session::{PauseState, SimState, UiScreen};
use ssg_sim::time::{FIXED_HZ, MAX_STEPS_PER_UPDATE, PHYS_DT, StepCap, WorldTime};

use crate::args::ReplayArgs;
use crate::scenario::sim_app;

/// Exit 1's message (C9): `built_with.target` equality does not pin the C math
/// library.
pub const SAME_BUILD_DIFFERS: &str =
    "same target and version: a defect, or a different C library or toolchain (F-48)";

/// Exit 3's message.
pub const OTHER_BUILD_DIFFERS: &str = "the recording was made by another target or version: \
     re-simulation differs across platforms and versions (F-48)";

/// The first differing body of a frame.
struct Difference {
    step: u64,
    body: usize,
    position_m: f64,
    velocity_mps: f64,
}

/// Frames compared so far.
struct Comparison<'a> {
    frames: &'a [RecordedFrame],
    roles: Vec<RecordingBodyRole>,
    next: usize,
    identical: usize,
    first: Option<Difference>,
    /// The largest |Δposition| and its frame step and body.
    largest: Option<(f64, u64, usize)>,
}

impl Comparison<'_> {
    /// Compares the frame at the app's current step, if there is one.
    fn check(&mut self, app: &App, bodies: &RecordingBodies) -> Result<(), String> {
        let step = current_step(app);
        while let Some(frame) = self.frames.get(self.next) {
            if frame.line.step > step {
                return Ok(());
            }
            if frame.line.step < step {
                return Err(format!(
                    "the re-simulation passed the frame at step {} (it is at step {step})",
                    frame.line.step
                ));
            }
            self.next += 1;
            let world = app.world();
            let mut same = true;
            for (index, (&entity, recorded)) in
                bodies.entities.iter().zip(&frame.bodies).enumerate()
            {
                let state = body_state(world, entity)
                    .ok_or_else(|| format!("body {index} has no state at step {step}"))?;
                if bits(&state) == bits(recorded) {
                    continue;
                }
                same = false;
                let position_m = (state.position - recorded.position).length();
                let velocity_mps = (state.velocity - recorded.velocity).length();
                if self.first.is_none() {
                    self.first = Some(Difference {
                        step,
                        body: index,
                        position_m,
                        velocity_mps,
                    });
                }
                if self
                    .largest
                    .is_none_or(|(largest, ..)| position_m > largest)
                {
                    self.largest = Some((position_m, step, index));
                }
            }
            if same {
                self.identical += 1;
            }
        }
        Ok(())
    }
}

fn body_state(world: &World, entity: Entity) -> Option<BodyFrame> {
    Some(BodyFrame {
        position: world.get::<Position>(entity)?.0,
        rotation: world.get::<Rotation>(entity)?.0,
        velocity: world.get::<LinearVelocity>(entity)?.0,
        angular_velocity: world.get::<AngularVelocity>(entity)?.0,
    })
}

fn bits(state: &BodyFrame) -> [u64; 13] {
    KeyframeRow::new(0, 0, state).values.map(f64::to_bits)
}

fn current_step(app: &App) -> u64 {
    let t = app.world().resource::<WorldTime>();
    ((t.epoch - t.start_epoch) / PHYS_DT).round() as u64
}

fn role_name(role: RecordingBodyRole) -> &'static str {
    match role {
        RecordingBodyRole::Target => "target",
        RecordingBodyRole::TetherRoot => "tether_root",
        RecordingBodyRole::TetherNode => "tether_node",
        RecordingBodyRole::TetherTail => "tether_tail",
    }
}

/// k from the first input; a recording without it is re-simulated with k = 1
/// when the CLI wrote it (the CLI always runs 1), and refused otherwise (R7).
fn steps_before_propagators(
    inputs: &[RecordedInput],
    source: Option<RecordingSource>,
) -> Result<u32, String> {
    match inputs.first().map(|input| &input.command) {
        Some(RecordedCommand::SessionStart {
            steps_before_propagators,
        }) => Ok(*steps_before_propagators),
        _ if source == Some(RecordingSource::Cli) => Ok(1),
        _ => Err(
            "inputs.jsonl has no `session_start` entry and the recording's source is \
             not `cli`: the spawn-update step count is unknown, so it cannot be re-simulated"
                .to_string(),
        ),
    }
}

/// Checks and re-simulates the recording; prints the outcome and returns the exit
/// code.
pub fn replay(args: &ReplayArgs) -> u8 {
    let dir = &args.recording_dir;
    let check = check_recording(dir, &dir.display().to_string());
    for warning in &check.warnings {
        eprintln!("ssg-sim: warning: {warning}");
    }
    if !check.errors.is_empty() {
        for error in &check.errors {
            eprintln!("ssg-sim: error: {error}");
        }
        return 2;
    }
    let recording = match read_recording(dir) {
        Ok(recording) => recording,
        Err(errors) => {
            for error in errors {
                eprintln!("ssg-sim: error: {error}");
            }
            return 2;
        }
    };
    let manifest = &recording.manifest;
    let (first, last) = (
        recording.frames[0].line.step,
        recording.frames[recording.frames.len() - 1].line.step,
    );
    println!(
        "recording {}: {} bodies, {} frames, steps {first}–{last} ({:.2} s), a frame every {} steps",
        dir.display(),
        manifest.bodies.len(),
        recording.frames.len(),
        (last - first) as f64 / FIXED_HZ,
        manifest.keyframe_interval_steps
    );
    let differences = manifest.built_with.differences(&BuiltWith::current());
    if differences.is_empty() {
        println!("built with: this build");
    } else {
        println!("built with: {}", differences.join("; "));
    }
    if args.frames_only {
        println!("frames only: the files agree; not re-simulated");
        return 0;
    }
    let k = match steps_before_propagators(&recording.inputs, manifest.source) {
        Ok(k) => k,
        Err(message) => {
            eprintln!("ssg-sim: {message}");
            return 2;
        }
    };
    let comparison = match resimulate(&recording, k, &args.assets) {
        Ok(comparison) => comparison,
        Err(message) => {
            eprintln!("ssg-sim: cannot re-simulate: {message}");
            return 2;
        }
    };
    println!(
        "re-simulated steps 0–{last} with k = {k}: frames: {}, identical: {}",
        recording.frames.len(),
        comparison.identical
    );
    let Some(diff) = &comparison.first else {
        return 0;
    };
    println!(
        "first difference: frame step {}, body {} ({}), |Δposition| {:e} m, |Δvelocity| {:e} m/s",
        diff.step,
        diff.body,
        role_name(comparison.roles[diff.body]),
        diff.position_m,
        diff.velocity_mps
    );
    if let Some((largest, step, body)) = comparison.largest {
        println!("largest |Δposition|: {largest:e} m (frame step {step}, body {body})");
    }
    if differences.is_empty() {
        println!("{SAME_BUILD_DIFFERS}");
        1
    } else {
        println!("{OTHER_BUILD_DIFFERS}");
        3
    }
}

/// Runs the recorded session from step 0 to the last frame and compares each
/// frame as it is reached.
fn resimulate<'a>(
    recording: &'a LoadedRecording,
    k: u32,
    assets: &Path,
) -> Result<Comparison<'a>, String> {
    let manifest = &recording.manifest;
    let project = manifest
        .project
        .as_ref()
        .ok_or("the manifest has no `project` (the scenario at the start)")?;
    let scenario = project
        .sim_inputs()
        .map_err(|errors| format!("the recording's project: {}", errors.join("; ")))?;
    let mut app = sim_app(assets, scenario.start_epoch);
    app.world_mut()
        .resource_mut::<CapturePlanLibrary>()
        .insert_plan(manifest.plan_id.clone(), manifest.plan.clone());
    app.world_mut().resource_mut::<SelectedProject>().project_id = Some(manifest.plan_id.clone());
    {
        let mut selection = app.world_mut().resource_mut::<OrbitalSelectionState>();
        selection.target = Some(scenario.target);
        selection.chaser = Some(scenario.chaser);
    }

    app.world_mut()
        .resource_mut::<NextState<UiScreen>>()
        .set(UiScreen::Sim);
    app.update();
    // The update that spawns the bodies runs k steps before `init_orbitals` builds
    // the propagators on the next update (C1).
    if k == 0 {
        app.world_mut()
            .resource_mut::<NextState<PauseState>>()
            .set(PauseState::Paused);
    } else {
        app.world_mut().resource_mut::<WorldTime>().multiplier = k;
        app.insert_resource(StepCap(k.max(MAX_STEPS_PER_UPDATE)));
    }
    app.world_mut()
        .resource_mut::<NextState<SimState>>()
        .set(SimState::Running);
    app.update();
    if current_step(&app) != u64::from(k) {
        return Err(format!(
            "the spawn update ran {} steps, not k = {k}",
            current_step(&app)
        ));
    }
    if k == 0 {
        app.world_mut()
            .resource_mut::<NextState<PauseState>>()
            .set(PauseState::Running);
    } else {
        app.world_mut().resource_mut::<WorldTime>().multiplier = 1;
        app.insert_resource(StepCap::default());
    }
    let bodies = app
        .world()
        .get_resource::<RecordingBodies>()
        .cloned()
        .ok_or("the scenario spawned no bodies")?;
    let roles: Vec<RecordingBodyRole> = manifest.bodies.iter().map(|b| b.role).collect();
    if bodies.roles != roles {
        return Err(format!(
            "the scenario spawned {} bodies; the recording has {}",
            bodies.roles.len(),
            roles.len()
        ));
    }
    let mut comparison = Comparison {
        frames: &recording.frames,
        roles,
        next: 0,
        identical: 0,
        first: None,
        largest: None,
    };
    comparison.check(&app, &bodies)?;

    for input in recording.inputs.iter().skip(1) {
        let RecordedCommand::StartCapture { target, plan_id } = &input.command else {
            return Err(format!(
                "a second `session_start` entry after step {}",
                input.after_step
            ));
        };
        if *target != RecordingBodyRole::Target {
            return Err(format!(
                "an input after step {} starts a capture of a {}; only the target can be captured",
                input.after_step,
                role_name(*target)
            ));
        }
        let after = input.after_step;
        while current_step(&app) + 1 < after {
            app.update();
            comparison.check(&app, &bodies)?;
        }
        let step = current_step(&app);
        if step > after {
            return Err(format!(
                "the input after step {after} comes after one at step {step}: the inputs are not \
                 ordered"
            ));
        }
        app.world_mut().write_message(SimCommand::StartCapture {
            target: Some(bodies.entities[0]),
            plan_id: plan_id.clone(),
        });
        if step == after {
            // Applied in an update that ran no step.
            app.world_mut()
                .resource_mut::<NextState<PauseState>>()
                .set(PauseState::Paused);
            app.update();
            app.world_mut()
                .resource_mut::<NextState<PauseState>>()
                .set(PauseState::Running);
        } else {
            app.update();
        }
        comparison.check(&app, &bodies)?;
    }
    let last = recording.frames[recording.frames.len() - 1].line.step;
    while current_step(&app) < last {
        app.update();
        comparison.check(&app, &bodies)?;
    }
    Ok(comparison)
}
