//! The Recording window (P7, S5): Record / Stop, the recorder's status, the
//! projected size at the caps and the per-project recording settings, and the
//! app's adaptation of the per-update step cap (F-46, R9).
//!
//! The recorder itself lives in `ssg_sim::recording`; the window only sends it
//! [`RecorderCommand`]s and reads its state. The sink that writes
//! `<project>/recordings/<name>/` is installed when the bodies are spawned.

use std::path::Path;

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use crate::ui::egui::body_label;
use ssg_sim::capture::plan::{CapturePlan, CapturePlanLibrary};
use ssg_sim::orbit::selection::OrbitalSelectionState;
use ssg_sim::project::export::sanitize_file_stem;
use ssg_sim::project::manifest::{
    MANIFEST_FILE, ProjectManifest, RecordingBlock, read_manifest_file, save_manifest,
};
use ssg_sim::project::recording::RecordingSource;
use ssg_sim::project::{SelectedProject, WorkingDirectory};
use ssg_sim::recording::files::{FileSink, RecordingContext};
use ssg_sim::recording::recorder::{
    Recorder, RecorderCommand, RecorderEvent, RecorderState, RecordingSettings,
};
use ssg_sim::recording::{BYTES_PER_BODY_FRAME, RecordingBodies, RecordingSink, StartTrigger};
use ssg_sim::session::PauseState;
use ssg_sim::telemetry::{LogEvent, LogLevel};
use ssg_sim::time::{FIXED_HZ, StepCap, WorldTime};

/// The most notes the window keeps (the recorder's last messages).
const NOTE_LIMIT: usize = 6;

/// The Recording window's state besides the recorder's own.
#[derive(Resource, Debug, Default)]
pub struct RecordingPanel {
    /// The settings being edited; `None` until the window first draws in a session.
    pub draft: Option<RecordingSettings>,
    /// The recorder's last messages, oldest first.
    pub notes: Vec<String>,
    /// The outcome of the last Save.
    pub saved: Option<Result<(), String>>,
    /// The recorder has warned about the running recording (80 % of a cap).
    pub warned: bool,
}

impl RecordingPanel {
    fn note(&mut self, text: String) {
        self.notes.push(text);
        if self.notes.len() > NOTE_LIMIT {
            self.notes.remove(0);
        }
    }
}

/// What the window shows about the recorder.
#[derive(Debug, Clone, PartialEq)]
pub struct PanelStatus {
    pub recording: bool,
    /// Sim seconds from the first frame to the last written one.
    pub duration_s: f64,
    pub frames: u64,
    pub bytes: u64,
    /// The size the recording would reach at the duration or size cap, whichever
    /// is first.
    pub projected_bytes: u64,
    /// 80 % of the duration or size cap reached: the recorder has warned, or the
    /// frames written so far are already there.
    pub near_cap: bool,
}

/// `<plan-id>-<UTC stamp>` (A-8); `stamp` is the caller's wall-clock stamp.
pub(crate) fn recording_name(plan_id: &str, stamp: &str) -> String {
    format!("{}-{stamp}", sanitize_file_stem(plan_id))
}

/// The wall-clock stamp of a recording name, `YYYYMMDDTHHMMSSZ` (UTC). The app,
/// not the sim, reads the clock (F-06).
fn utc_stamp() -> String {
    chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string()
}

/// The size of a recording of `bodies` bodies when a cap stops it: frames at the
/// duration cap (`⌈max_duration_s · 64 / K⌉ + 1`) or as many as fit `max_size_mb`.
pub(crate) fn projected_bytes(settings: &RecordingSettings, bodies: usize) -> u64 {
    let frame_bytes = BYTES_PER_BODY_FRAME * bodies as u64;
    if frame_bytes == 0 {
        return 0;
    }
    let steps = (settings.max_duration_s * FIXED_HZ).ceil().max(0.0) as u64;
    let by_duration =
        (steps / u64::from(settings.keyframe_interval_steps.max(1)) + 1) * frame_bytes;
    by_duration.min((settings.max_size_mb * 1e6) as u64)
}

pub(crate) fn panel_status(
    recorder: &Recorder,
    bytes: u64,
    settings: &RecordingSettings,
    bodies: usize,
    warned: bool,
) -> PanelStatus {
    let projected_bytes = projected_bytes(settings, bodies);
    match &recorder.state {
        RecorderState::Idle => PanelStatus {
            recording: false,
            duration_s: 0.0,
            frames: 0,
            bytes: 0,
            projected_bytes,
            near_cap: false,
        },
        RecorderState::Recording(active) => {
            let duration_s = (active.last_frame_step - active.start_step) as f64 / FIXED_HZ;
            PanelStatus {
                recording: true,
                duration_s,
                frames: active.frames,
                bytes,
                projected_bytes,
                near_cap: warned
                    || duration_s * 5.0 >= settings.max_duration_s * 4.0
                    || bytes as f64 * 5.0 >= settings.max_size_mb * 1e6 * 4.0,
            }
        }
    }
}

/// The settings as a `recording` block that states every key.
pub(crate) fn settings_to_block(settings: &RecordingSettings) -> RecordingBlock {
    RecordingBlock {
        auto_start_phase: settings.auto_start_phase.clone(),
        auto_stop_on_end: Some(settings.auto_stop_on_end),
        stop_after_end_s: Some(settings.stop_after_end_s),
        keyframe_interval_steps: Some(settings.keyframe_interval_steps),
        max_duration_s: Some(settings.max_duration_s),
        max_size_mb: Some(settings.max_size_mb),
        max_tether_bodies: Some(settings.max_tether_bodies),
    }
}

/// Writes `settings` as the `recording` block of `<dir>/project.json`. `Err` when
/// the folder has no usable manifest or the block would not validate; the file is
/// not touched then.
pub(crate) fn save_recording_settings(
    dir: &Path,
    settings: &RecordingSettings,
) -> Result<(), String> {
    let report = read_manifest_file(dir)
        .ok_or_else(|| format!("{MANIFEST_FILE}: no such file in {}", dir.display()))?;
    let mut manifest = report.manifest.ok_or_else(|| report.errors.join("; "))?;
    manifest.recording = Some(settings_to_block(settings));
    let errors = manifest.validate();
    if !errors.is_empty() {
        return Err(errors.join("; "));
    }
    save_manifest(dir, &manifest).map_err(|e| format!("could not write {MANIFEST_FILE}: {e}"))
}

/// Everything the file sink needs to know about the session.
pub(crate) fn recording_context(
    dir: &Path,
    plan_id: &str,
    plan: CapturePlan,
    project: Option<ProjectManifest>,
    settings: RecordingSettings,
    target_name: String,
    stamp: &str,
) -> RecordingContext {
    RecordingContext {
        recordings_dir: dir.join("recordings"),
        default_name: recording_name(plan_id, stamp),
        source: RecordingSource::App,
        plan_id: plan_id.to_string(),
        plan,
        project,
        settings,
        target_name,
    }
}

/// `OnEnter(SimState::Running)`: the recorder's settings from `project.json`'s
/// `recording` block and a sink writing `<project>/recordings/`.
pub fn install_recording_sink(
    mut commands: Commands,
    working_directory: Res<WorkingDirectory>,
    selected_project: Res<SelectedProject>,
    capture_plan_lib: Res<CapturePlanLibrary>,
    selection: Res<OrbitalSelectionState>,
    mut log: MessageWriter<LogEvent>,
) {
    let dir = Path::new(&working_directory.path);
    let manifest = read_manifest_file(dir).and_then(|report| report.manifest);
    let settings =
        RecordingSettings::from_block(manifest.as_ref().and_then(|m| m.recording.as_ref()));
    commands.insert_resource(settings.clone());
    let Some((plan_id, plan)) = selected_project.project_id.as_deref().and_then(|id| {
        capture_plan_lib
            .plans
            .get(id)
            .map(|plan| (id, plan.clone()))
    }) else {
        return;
    };
    if let Some(warning) = manifest
        .as_ref()
        .and_then(|m| m.recording.as_ref())
        .and_then(|block| block.auto_start_warning(&plan))
    {
        log.write(LogEvent {
            level: LogLevel::Warn,
            source: "recording",
            message: warning,
        });
    }
    let sink = FileSink::new(recording_context(
        dir,
        plan_id,
        plan,
        manifest,
        settings,
        body_label(&selection, false),
        &utc_stamp(),
    ));
    commands.insert_resource(RecordingSink(Box::new(sink)));
}

/// A new session starts with an empty window.
pub fn reset_recording_panel(mut panel: ResMut<RecordingPanel>) {
    *panel = RecordingPanel::default();
}

/// Keeps the recorder's messages for the window.
pub fn collect_recorder_events(
    mut events: MessageReader<RecorderEvent>,
    mut panel: ResMut<RecordingPanel>,
) {
    for event in events.read() {
        match event {
            RecorderEvent::Started { .. } | RecorderEvent::Stopped { .. } => panel.warned = false,
            RecorderEvent::Warning(_) => panel.warned = true,
            RecorderEvent::Refused(_) => {}
        }
        let text = match event {
            RecorderEvent::Started { step, trigger } => format!(
                "Recording started at step {step} ({})",
                match trigger {
                    StartTrigger::Manual => "manual",
                    StartTrigger::Phase => "phase",
                }
            ),
            RecorderEvent::Stopped { end } => format!(
                "Recording stopped at step {}: {:?}",
                end.end_step, end.reason
            ),
            RecorderEvent::Warning(text) => format!("Warning: {text}"),
            RecorderEvent::Refused(text) => format!("Refused: {text}"),
        };
        panel.note(text);
    }
}

/// What a click in the window asks for.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum PanelAction {
    Record,
    Stop,
    Save,
}

/// Applies a click: Record and Stop become [`RecorderCommand`]s, Save writes the
/// draft to `project.json` and installs it. Returns the commands to send.
pub(crate) fn apply_panel_action(
    action: &PanelAction,
    panel: &mut RecordingPanel,
    settings: &mut RecordingSettings,
    plan_id: Option<&str>,
    project_dir: &Path,
    stamp: &str,
) -> Option<RecorderCommand> {
    match action {
        PanelAction::Record => Some(RecorderCommand::Start {
            name: recording_name(plan_id.unwrap_or("recording"), stamp),
        }),
        PanelAction::Stop => Some(RecorderCommand::Stop),
        PanelAction::Save => {
            if let Some(draft) = &panel.draft {
                let result = save_recording_settings(project_dir, draft);
                if result.is_ok() {
                    *settings = draft.clone();
                }
                panel.saved = Some(result);
            }
            None
        }
    }
}

/// Where the Recording, Recordings and Transport windows open: right of the
/// Space Catalog panel (12 px in, 340 px wide, from about 116 px down).
pub(crate) const WINDOW_POS: egui::Pos2 = egui::Pos2::new(368.0, 128.0);

pub(crate) fn megabytes(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / 1e6)
}

pub fn recording_window(
    mut contexts: EguiContexts,
    recorder: Res<Recorder>,
    sink: Res<RecordingSink>,
    bodies: Option<Res<RecordingBodies>>,
    mut settings: ResMut<RecordingSettings>,
    mut panel: ResMut<RecordingPanel>,
    selected_project: Res<SelectedProject>,
    capture_plan_lib: Res<CapturePlanLibrary>,
    working_directory: Res<WorkingDirectory>,
    mut commands: MessageWriter<RecorderCommand>,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    let body_count = bodies.as_ref().map_or(0, |b| b.roles.len());
    let status = panel_status(
        &recorder,
        sink.0.bytes(),
        &settings,
        body_count,
        panel.warned,
    );
    let phases: Vec<String> = selected_project
        .project_id
        .as_deref()
        .and_then(|id| capture_plan_lib.plans.get(id))
        .map(|plan| plan.phases.iter().map(|p| p.id.clone()).collect())
        .unwrap_or_default();
    if panel.draft.is_none() {
        panel.draft = Some(settings.clone());
    }
    let mut action = None;
    let available_rect = ctx.available_rect();
    egui::Window::new("Recording")
        .default_pos(WINDOW_POS)
        .constrain_to(available_rect)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                if status.recording {
                    if ui.button("⏹ Stop").clicked() {
                        action = Some(PanelAction::Stop);
                    }
                    ui.colored_label(egui::Color32::LIGHT_RED, "● recording");
                } else if ui.button("⏺ Record").clicked() {
                    action = Some(PanelAction::Record);
                }
            });
            egui::Grid::new("recording_status").show(ui, |ui| {
                ui.label("Duration");
                ui.label(format!(
                    "{:.1} s of {:.0} s",
                    status.duration_s, settings.max_duration_s
                ));
                ui.end_row();
                ui.label("Frames");
                ui.label(status.frames.to_string());
                ui.end_row();
                ui.label("Size");
                ui.label(format!(
                    "{} of {:.0} MB",
                    megabytes(status.bytes),
                    settings.max_size_mb
                ));
                ui.end_row();
                ui.label("Projected at the cap");
                ui.label(format!(
                    "{} ({body_count} bodies, a frame every {} steps)",
                    megabytes(status.projected_bytes),
                    settings.keyframe_interval_steps
                ));
                ui.end_row();
            });
            if status.near_cap {
                ui.colored_label(
                    egui::Color32::YELLOW,
                    "80 % of the duration or size cap reached",
                );
            }
            for note in panel.notes.iter().rev() {
                ui.label(note);
            }
            ui.separator();
            ui.collapsing("Settings", |ui| {
                let Some(draft) = panel.draft.as_mut() else {
                    return;
                };
                ui.horizontal(|ui| {
                    ui.label("Start at phase");
                    let selected = draft
                        .auto_start_phase
                        .clone()
                        .unwrap_or_else(|| "(manual only)".to_string());
                    egui::ComboBox::from_id_salt("recording_auto_start")
                        .selected_text(selected)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut draft.auto_start_phase, None, "(manual only)");
                            for phase in &phases {
                                ui.selectable_value(
                                    &mut draft.auto_start_phase,
                                    Some(phase.clone()),
                                    phase,
                                );
                            }
                        });
                });
                ui.checkbox(&mut draft.auto_stop_on_end, "Stop after the end event");
                ui.horizontal(|ui| {
                    ui.label("Seconds after the end");
                    ui.add(egui::DragValue::new(&mut draft.stop_after_end_s).range(1.0..=3600.0));
                });
                ui.horizontal(|ui| {
                    ui.label("Frame every (steps)");
                    ui.add(
                        egui::DragValue::new(&mut draft.keyframe_interval_steps).range(1..=4096),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label("Longest (s)");
                    ui.add(egui::DragValue::new(&mut draft.max_duration_s).range(1.0..=86_400.0));
                });
                ui.horizontal(|ui| {
                    ui.label("Largest (MB)");
                    ui.add(egui::DragValue::new(&mut draft.max_size_mb).range(1.0..=100_000.0));
                });
                ui.horizontal(|ui| {
                    ui.label("Most tether bodies");
                    ui.add(egui::DragValue::new(&mut draft.max_tether_bodies).range(1..=1000));
                });
                if ui.button("Save to project.json").clicked() {
                    action = Some(PanelAction::Save);
                }
                match &panel.saved {
                    Some(Ok(())) => {
                        ui.label("Saved; applies to the next recording.");
                    }
                    Some(Err(e)) => {
                        ui.colored_label(egui::Color32::LIGHT_RED, e);
                    }
                    None => {}
                }
            });
        });
    if let Some(action) = action {
        let command = apply_panel_action(
            &action,
            &mut panel,
            &mut settings,
            selected_project.project_id.as_deref(),
            Path::new(&working_directory.path),
            &utc_stamp(),
        );
        if let Some(command) = command {
            commands.write(command);
        }
    }
}

// ─── The app's step cap (F-46, R9) ────────────────────────────────────────────

/// A frame longer than this at the cap lowers it.
const SLOW_FRAME_S: f32 = 0.050;
/// A frame shorter than this raises the cap while the requested warp exceeds it.
const FAST_FRAME_S: f32 = 0.025;
/// The cap changes at most this often (wall seconds).
const CAP_CHANGE_PERIOD_S: f32 = 0.5;
pub const MIN_STEP_CAP: u32 = 8;
pub const MAX_STEP_CAP: u32 = 512;

/// The cap after a frame of `frame_s` wall seconds that ran `effective` of the
/// `requested` steps under `cap`: halved (not below 8) after a frame over 50 ms
/// that ran at the cap, doubled (not above 512) after a frame under 25 ms while the
/// request exceeds the cap; otherwise unchanged. The caller applies it at most
/// once per 0.5 s.
pub(crate) fn next_step_cap(cap: u32, requested: u32, effective: u32, frame_s: f32) -> u32 {
    if frame_s > SLOW_FRAME_S && effective >= cap {
        (cap / 2).max(MIN_STEP_CAP)
    } else if frame_s < FAST_FRAME_S && requested > cap {
        cap.saturating_mul(2).min(MAX_STEP_CAP)
    } else {
        cap
    }
}

/// Adapts [`StepCap`] to the machine between updates (`FixedUpdate` reads it once
/// at its top, so a change never lands inside a run of steps).
pub fn adapt_step_cap(
    time: Res<Time<Real>>,
    world_time: Res<WorldTime>,
    pause: Res<State<PauseState>>,
    mut cap: ResMut<StepCap>,
    mut since_change_s: Local<f32>,
) {
    *since_change_s += time.delta_secs();
    if *pause.get() == PauseState::Paused || *since_change_s < CAP_CHANGE_PERIOD_S {
        return;
    }
    let next = next_step_cap(
        cap.0,
        world_time.multiplier,
        world_time.effective_multiplier,
        time.delta_secs(),
    );
    if next != cap.0 {
        cap.0 = next;
        *since_change_s = 0.0;
    }
}

/// A session starts at the default cap.
pub fn reset_step_cap(mut cap: ResMut<StepCap>) {
    *cap = StepCap::default();
}
