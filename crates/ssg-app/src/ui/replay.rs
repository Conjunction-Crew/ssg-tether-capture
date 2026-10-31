//! Playing a recording (P7, S5): the Recordings window, the replay mode of the sim
//! screen (`SimState::Replay`) and the egui Transport window.
//!
//! Playback only reads the recording's files (design R6): the replay bodies are
//! visual only (no avian step, no sim commands, no recorder), and every shown
//! state comes from [`Player`], which interpolates origin-aware between frames.

use std::path::{Path, PathBuf};

use bevy::ecs::system::SystemParam;
use bevy::math::DVec3;
use bevy::pbr::{Atmosphere, AtmosphereSettings};
use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, block_on, futures_lite::future};
use bevy_egui::{EguiContexts, egui};

use crate::render::camera::{CameraTarget, set_map_view};
use crate::render::layers::{MAP_LAYER, SCENE_LAYER};
use crate::ui::events::UiEvent;
use crate::ui::recording::{WINDOW_POS, megabytes};
use ssg_sim::capture::plan::CapturePlan;
use ssg_sim::project::manifest::BuiltWith;
use ssg_sim::project::recording::{
    RECORDING_PARTIAL_SUFFIX, RECORDING_TRANSITIONS_FILE, RecordingBodyRole, RecordingManifest,
    read_recording_file,
};
use ssg_sim::project::{WorkingDirectory, format_utc, parse_utc};
use ssg_sim::recording::BodyFrame;
use ssg_sim::recording::files::{list_recording_dirs, read_recording};
use ssg_sim::recording::player::{FrameView, PLAYBACK_SPEEDS, Player};
use ssg_sim::session::{SimState, UiScreen};
use ssg_sim::tether::{DebrisBody, TetherShape};
use ssg_sim::time::{FIXED_HZ, PHYS_DT};

use bevy::camera::visibility::RenderLayers;

/// Sphere radius of the tether's root and tail, and the rope radius the interior
/// nodes derive theirs from; the values `spawn_tether` uses.
const END_SPHERE_RADIUS_M: f64 = 0.50;
const ROPE_RADIUS_M: f64 = 0.25;

/// A body of the replay scene, by its index in the recording's body table.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReplayBody {
    pub index: usize,
}

/// A recording opened for playback.
#[derive(Resource, Debug)]
pub struct ReplaySession {
    pub name: String,
    pub player: Player,
    pub roles: Vec<RecordingBodyRole>,
    /// The visual of each body: `None` for the target (the debris model).
    pub shapes: Vec<Option<TetherShape>>,
    /// `(step, epoch_utc)` of every frame, for the epoch readout.
    pub epochs: Vec<(u64, String)>,
    /// How the recording's build differs from this one, one phrase each.
    pub differences: Vec<String>,
}

/// One row of the Recordings window.
#[derive(Debug, Clone, PartialEq)]
pub struct RecordingEntry {
    pub name: String,
    pub dir: PathBuf,
    pub duration_s: f64,
    pub bodies: usize,
    pub size_bytes: u64,
    pub differences: Vec<String>,
    /// Why the recording cannot be opened, when it cannot.
    pub error: Option<String>,
}

/// Set by Exit replay: the sim screen is left for Home and entered again on the
/// next frame, as a restart does. Entering `UiScreen::Sim` again from itself
/// would run its OnExit/OnEnter but despawn nothing (`DespawnOnExit` skips a
/// transition to the same state): two cameras, two Earths.
#[derive(Resource, Default)]
pub struct ReturnToSim(pub bool);

/// The Recordings window's list and the load in flight.
#[derive(Resource, Default)]
pub struct RecordingsList {
    pub entries: Vec<RecordingEntry>,
    /// Whether `entries` is up to date; Refresh clears it.
    pub scanned: bool,
    pub loading: Option<(String, Task<Result<ReplaySession, String>>)>,
    pub error: Option<String>,
}

// ─── Reading recordings ───────────────────────────────────────────────────────

fn dir_size(dir: &Path) -> u64 {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| entry.metadata().ok())
        .filter(|meta| meta.is_file())
        .map(|meta| meta.len())
        .sum()
}

/// The rows for the finished recordings under `recordings_dir`, by name; `.partial`
/// folders are not listed (R8). A folder whose manifest cannot be read is listed
/// with its error.
pub(crate) fn scan_recordings(recordings_dir: &Path) -> Vec<RecordingEntry> {
    let running = BuiltWith::current();
    list_recording_dirs(recordings_dir)
        .into_iter()
        .filter(|dir| {
            !dir.file_name()
                .is_some_and(|name| name.to_string_lossy().ends_with(RECORDING_PARTIAL_SUFFIX))
        })
        .map(|dir| {
            let name = dir
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let size_bytes = dir_size(&dir);
            let report = read_recording_file(&dir);
            match report.and_then(|r| r.manifest.ok_or(r.errors).ok()) {
                Some(manifest) => RecordingEntry {
                    duration_s: manifest.end_step.map_or(0.0, |end| {
                        end.saturating_sub(manifest.start_step) as f64 / FIXED_HZ
                    }),
                    bodies: manifest.bodies.len(),
                    differences: manifest.built_with.differences(&running),
                    error: None,
                    name,
                    dir,
                    size_bytes,
                },
                None => RecordingEntry {
                    error: Some(format!("{MANIFEST_NAME} cannot be read")),
                    duration_s: 0.0,
                    bodies: 0,
                    differences: Vec::new(),
                    name,
                    dir,
                    size_bytes,
                },
            }
        })
        .collect()
}

const MANIFEST_NAME: &str = ssg_sim::project::recording::RECORDING_MANIFEST_FILE;

/// The shape of each body as `spawn_tether` builds it for `plan`: spheres at the
/// root and the tail, cylinders between, and no shape for the target (it shows
/// the debris model).
pub(crate) fn replay_shapes(
    plan: &CapturePlan,
    roles: &[RecordingBodyRole],
) -> Vec<Option<TetherShape>> {
    let device = plan.device.clone().unwrap_or_default();
    let length = device.tether_length_m();
    let spacing = device.segment_spacing_m().max(1e-3);
    let interior = ((length - 2.0 * END_SPHERE_RADIUS_M) / spacing).max(0.0) as u32;
    let node_length = if interior > 0 {
        (length - 2.0 * END_SPHERE_RADIUS_M) / f64::from(interior)
    } else {
        spacing
    };
    roles
        .iter()
        .map(|role| match role {
            RecordingBodyRole::Target => None,
            RecordingBodyRole::TetherRoot | RecordingBodyRole::TetherTail => {
                Some(TetherShape::Sphere {
                    radius: END_SPHERE_RADIUS_M,
                })
            }
            RecordingBodyRole::TetherNode => Some(TetherShape::Cylinder {
                radius: ROPE_RADIUS_M / 8.0,
                length: node_length,
            }),
        })
        .collect()
}

/// Reads the recording in `dir` for playback.
pub(crate) fn load_replay_session(dir: &Path) -> Result<ReplaySession, String> {
    let recording = read_recording(dir).map_err(|errors| errors.join("; "))?;
    let has_transitions = dir.join(RECORDING_TRANSITIONS_FILE).is_file();
    let player = Player::from_recording(&recording, has_transitions)
        .ok_or_else(|| "the recording has no frames".to_string())?;
    let manifest: &RecordingManifest = &recording.manifest;
    let mut roles: Vec<_> = manifest.bodies.iter().map(|b| (b.index, b.role)).collect();
    roles.sort_by_key(|(index, _)| *index);
    let roles: Vec<RecordingBodyRole> = roles.into_iter().map(|(_, role)| role).collect();
    Ok(ReplaySession {
        name: dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        shapes: replay_shapes(&manifest.plan, &roles),
        epochs: recording
            .frames
            .iter()
            .map(|f| (f.line.step, f.line.epoch_utc.clone()))
            .collect(),
        differences: manifest.built_with.differences(&BuiltWith::current()),
        roles,
        player,
    })
}

// ─── Transport ────────────────────────────────────────────────────────────────

/// What a click in the Transport window asks the player to do.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum TransportAction {
    JumpToStart,
    StepBack,
    TogglePlay,
    StepForward,
    JumpToEnd,
    SetReverse(bool),
    SetSpeed(f64),
    Seek(f64),
}

/// Applies a transport click. Jumps pause; step snaps to the next or previous
/// frame (never an interpolated state); play at the end of the direction of play
/// starts again from the other end.
pub(crate) fn apply_transport(player: &mut Player, action: &TransportAction) {
    match action {
        TransportAction::JumpToStart => {
            player.playing = false;
            player.seek(player.start_step() as f64);
        }
        TransportAction::JumpToEnd => {
            player.playing = false;
            player.seek(player.end_step() as f64);
        }
        TransportAction::StepBack => player.step(-1),
        TransportAction::StepForward => player.step(1),
        TransportAction::TogglePlay => {
            if player.playing {
                player.playing = false;
            } else {
                if !player.reverse && player.cursor() >= player.end_step() as f64 {
                    player.seek(player.start_step() as f64);
                } else if player.reverse && player.cursor() <= player.start_step() as f64 {
                    player.seek(player.end_step() as f64);
                }
                player.playing = true;
            }
        }
        TransportAction::SetReverse(reverse) => player.reverse = *reverse,
        TransportAction::SetSpeed(speed) => player.speed = *speed,
        TransportAction::Seek(step) => player.seek(*step),
    }
}

/// The epoch at `cursor`, from the frame at or before it.
pub(crate) fn epoch_at(epochs: &[(u64, String)], cursor: f64) -> Option<String> {
    let at = epochs.partition_point(|(step, _)| *step as f64 <= cursor);
    let (step, text) = epochs.get(at.checked_sub(1)?)?;
    let mut epoch = parse_utc(text)?;
    epoch += (cursor - *step as f64) * PHYS_DT;
    Some(format_utc(epoch))
}

/// Range and speed of the target relative to the tether root, as the live
/// telemetry shows them (the target is body 0, the root body 1).
pub(crate) fn relative_state(bodies: &[BodyFrame]) -> Option<(f64, f64)> {
    let (target, root) = (bodies.first()?, bodies.get(1)?);
    Some((
        (target.position - root.position).length(),
        (target.velocity - root.velocity).length(),
    ))
}

// ─── The replay scene ─────────────────────────────────────────────────────────

fn vec3(v: DVec3) -> Vec3 {
    v.as_vec3()
}

/// Writes the bodies' states of `view` into the replay bodies' transforms.
pub(crate) fn apply_view(view: &FrameView, bodies: &mut Query<(&ReplayBody, &mut Transform)>) {
    for (body, mut transform) in bodies {
        if let Some(state) = view.bodies.get(body.index) {
            transform.translation = vec3(state.position);
            transform.rotation = state.rotation.as_quat().normalize();
        }
    }
}

/// The body the camera follows in a replay: the tether root, as in the live sim
/// (the anchor, Q15), or the debris.
pub(crate) fn followed_body(roles: &[RecordingBodyRole], debris: bool) -> Option<usize> {
    let role = if debris {
        RecordingBodyRole::Target
    } else {
        RecordingBodyRole::TetherRoot
    };
    roles.iter().position(|r| *r == role)
}

/// `OnEnter(SimState::Replay)`: one visual-only body per recorded body. The
/// target carries the debris marker (the renderer attaches its model), the tether
/// bodies their [`TetherShape`]; nothing of the physics. The camera follows the
/// tether root.
pub fn spawn_replay_bodies(
    mut commands: Commands,
    session: Res<ReplaySession>,
    mut log: MessageWriter<ssg_sim::telemetry::LogEvent>,
) {
    let view = session.player.view();
    let followed = followed_body(&session.roles, false).or(followed_body(&session.roles, true));
    for (index, shape) in session.shapes.iter().enumerate() {
        let state = view.bodies.get(index);
        let mut entity = commands.spawn((
            ReplayBody { index },
            DespawnOnExit(SimState::Replay),
            RenderLayers::layer(SCENE_LAYER),
            Transform {
                translation: state.map_or(Vec3::ZERO, |s| vec3(s.position)),
                rotation: state.map_or(Quat::IDENTITY, |s| s.rotation.as_quat().normalize()),
                ..default()
            },
        ));
        match shape {
            Some(shape) => {
                entity.insert(*shape);
            }
            None => {
                entity.insert(DebrisBody {
                    model_path: "models/broken_satellite.glb".to_string(),
                });
            }
        }
        if followed == Some(index) {
            entity.insert(CameraTarget);
        }
    }
    log.write(ssg_sim::telemetry::LogEvent {
        level: ssg_sim::telemetry::LogLevel::Info,
        source: "replay",
        message: format!("Replaying recording {}", session.name),
    });
}

/// Moves the cursor by the wall time since the last frame (the real clock, not
/// the sim's virtual one, whose step is clamped).
pub fn advance_replay(time: Res<Time<Real>>, mut session: ResMut<ReplaySession>) {
    let dt = f64::from(time.delta_secs()).min(0.25);
    session.player.advance(dt);
}

pub fn show_replay_frame(
    session: Res<ReplaySession>,
    mut bodies: Query<(&ReplayBody, &mut Transform)>,
) {
    apply_view(&session.player.view(), &mut bodies);
}

/// `OnEnter(SimState::Replay)`: a replay opens in the detail view, where its
/// bodies are (the setup screen leaves the camera on the map).
pub fn show_replay_in_scene_view(
    camera: Single<(&mut RenderLayers, &mut Atmosphere, &mut AtmosphereSettings), With<Camera3d>>,
) {
    let (render_layers, atmosphere, atmosphere_settings) = camera.into_inner();
    if render_layers.intersects(&RenderLayers::layer(MAP_LAYER)) {
        set_map_view(render_layers, atmosphere, atmosphere_settings);
    }
}

pub fn remove_replay_session(mut commands: Commands) {
    commands.remove_resource::<ReplaySession>();
}

/// Leaves the replay for a fresh sim screen in its setup state ([`ReturnToSim`]).
pub(crate) fn exit_replay(
    next_sim_state: &mut NextState<SimState>,
    next_screen: &mut NextState<UiScreen>,
    pending: &mut ReturnToSim,
) {
    next_sim_state.set(SimState::Setup);
    next_screen.set(UiScreen::Home);
    pending.0 = true;
}

/// On Home after Exit replay: back to the sim screen.
pub fn return_to_sim(
    mut pending: ResMut<ReturnToSim>,
    mut next_screen: ResMut<NextState<UiScreen>>,
) {
    if std::mem::take(&mut pending.0) {
        next_screen.set(UiScreen::Sim);
    }
}

// ─── Windows ──────────────────────────────────────────────────────────────────

/// A new session starts with a fresh list.
pub fn reset_recordings_list(mut list: ResMut<RecordingsList>) {
    *list = RecordingsList::default();
}

/// Lists `<project>/recordings/` once per session (and on Refresh).
pub fn scan_recordings_system(
    working_directory: Res<WorkingDirectory>,
    mut list: ResMut<RecordingsList>,
) {
    if list.scanned {
        return;
    }
    list.entries = scan_recordings(&Path::new(&working_directory.path).join("recordings"));
    list.scanned = true;
}

/// Finishes a load: the session opens in replay mode, or the error shows in the window.
pub fn poll_open_recording(
    mut commands: Commands,
    mut list: ResMut<RecordingsList>,
    mut next_sim_state: ResMut<NextState<SimState>>,
) {
    let Some((_, task)) = list.loading.as_mut() else {
        return;
    };
    let Some(result) = block_on(future::poll_once(task)) else {
        return;
    };
    list.loading = None;
    match result {
        Ok(session) => {
            list.error = None;
            commands.insert_resource(session);
            next_sim_state.set(SimState::Replay);
        }
        Err(error) => list.error = Some(error),
    }
}

pub fn recordings_window(mut contexts: EguiContexts, mut list: ResMut<RecordingsList>) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    let mut open: Option<PathBuf> = None;
    let mut refresh = false;
    let available_rect = ctx.available_rect();
    egui::Window::new("Recordings")
        .default_pos(WINDOW_POS)
        .constrain_to(available_rect)
        .show(ctx, |ui| {
            if ui.button("Refresh").clicked() {
                refresh = true;
            }
            if list.entries.is_empty() {
                ui.label("No recordings in this project.");
            }
            egui::Grid::new("recordings_list")
                .striped(true)
                .show(ui, |ui| {
                    for entry in &list.entries {
                        ui.label(&entry.name);
                        match &entry.error {
                            Some(error) => {
                                ui.colored_label(egui::Color32::LIGHT_RED, error);
                            }
                            None => {
                                ui.label(format!(
                                    "{:.0} s · {} bodies · {}",
                                    entry.duration_s,
                                    entry.bodies,
                                    megabytes(entry.size_bytes)
                                ));
                                if !entry.differences.is_empty() {
                                    ui.colored_label(
                                        egui::Color32::YELLOW,
                                        format!("built with {}", entry.differences.join("; ")),
                                    );
                                }
                                if ui
                                    .add_enabled(list.loading.is_none(), egui::Button::new("Open"))
                                    .clicked()
                                {
                                    open = Some(entry.dir.clone());
                                }
                            }
                        }
                        ui.end_row();
                    }
                });
            if let Some((name, _)) = &list.loading {
                ui.label(format!("Loading {name}…"));
            }
            if let Some(error) = &list.error {
                ui.colored_label(egui::Color32::LIGHT_RED, error);
            }
        });
    if refresh {
        list.scanned = false;
    }
    if let Some(dir) = open {
        let name = dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let task = AsyncComputeTaskPool::get().spawn(async move { load_replay_session(&dir) });
        list.loading = Some((name, task));
    }
}

/// The camera's view and the body it follows, for the Transport window.
#[derive(SystemParam)]
pub struct ReplayCamera<'w, 's> {
    layers: Query<'w, 's, &'static RenderLayers, With<Camera3d>>,
    bodies: Query<'w, 's, (Entity, &'static ReplayBody, Has<CameraTarget>)>,
    ui_events: MessageWriter<'w, UiEvent>,
    commands: Commands<'w, 's>,
}

impl ReplayCamera<'_, '_> {
    /// Moves the camera to the body at `index`.
    fn follow(&mut self, index: usize) {
        for (entity, body, followed) in &self.bodies {
            if followed && body.index != index {
                self.commands.entity(entity).remove::<CameraTarget>();
            } else if !followed && body.index == index {
                self.commands.entity(entity).insert(CameraTarget);
            }
        }
    }
}

pub fn transport_window(
    mut contexts: EguiContexts,
    mut session: ResMut<ReplaySession>,
    mut next_sim_state: ResMut<NextState<SimState>>,
    mut next_screen: ResMut<NextState<UiScreen>>,
    mut return_to_sim: ResMut<ReturnToSim>,
    mut camera: ReplayCamera,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    let on_map = camera
        .layers
        .iter()
        .any(|layers| layers.intersects(&RenderLayers::layer(MAP_LAYER)));
    let debris = followed_body(&session.roles, true);
    let on_debris = camera
        .bodies
        .iter()
        .any(|(_, body, followed)| followed && Some(body.index) == debris);
    let mut follow = None;
    let view = session.player.view();
    let epoch = epoch_at(&session.epochs, view.step);
    let (start, end) = (
        session.player.start_step() as f64,
        session.player.end_step() as f64,
    );
    let mut actions = Vec::new();
    let mut exit = false;
    let mut cursor = view.step;
    let (playing, reverse, speed) = (
        session.player.playing,
        session.player.reverse,
        session.player.speed,
    );
    let available_rect = ctx.available_rect();
    egui::Window::new("Transport")
        .default_pos(WINDOW_POS)
        .constrain_to(available_rect)
        .show(ctx, |ui| {
            ui.label(format!("Recording {}", session.name));
            if !session.differences.is_empty() {
                ui.colored_label(
                    egui::Color32::YELLOW,
                    format!("built with {}", session.differences.join("; ")),
                );
            }
            ui.horizontal(|ui| {
                if ui.button("⏮").on_hover_text("Jump to the start").clicked() {
                    actions.push(TransportAction::JumpToStart);
                }
                if ui.button("◀").on_hover_text("Previous frame").clicked() {
                    actions.push(TransportAction::StepBack);
                }
                if ui
                    .button(if playing { "⏸" } else { "▶" })
                    .on_hover_text("Play / pause")
                    .clicked()
                {
                    actions.push(TransportAction::TogglePlay);
                }
                if ui.button("▶|").on_hover_text("Next frame").clicked() {
                    actions.push(TransportAction::StepForward);
                }
                if ui.button("⏭").on_hover_text("Jump to the end").clicked() {
                    actions.push(TransportAction::JumpToEnd);
                }
                let mut reversed = reverse;
                if ui.checkbox(&mut reversed, "Reverse").changed() {
                    actions.push(TransportAction::SetReverse(reversed));
                }
            });
            ui.horizontal(|ui| {
                ui.label("Speed");
                egui::ComboBox::from_id_salt("replay_speed")
                    .selected_text(format!("{speed}×"))
                    .show_ui(ui, |ui| {
                        for candidate in PLAYBACK_SPEEDS {
                            if ui
                                .selectable_label(speed == candidate, format!("{candidate}×"))
                                .clicked()
                            {
                                actions.push(TransportAction::SetSpeed(candidate));
                            }
                        }
                    });
            });
            ui.spacing_mut().slider_width = 320.0;
            if ui
                .add(egui::Slider::new(&mut cursor, start..=end).show_value(false))
                .changed()
            {
                actions.push(TransportAction::Seek(cursor));
            }
            egui::Grid::new("replay_readouts").show(ui, |ui| {
                ui.label("Step");
                ui.label(format!("{:.0} ({:.0} – {:.0})", view.step, start, end));
                ui.end_row();
                ui.label("Epoch");
                ui.label(epoch.unwrap_or_else(|| "?".to_string()));
                ui.end_row();
                ui.label("Bodies");
                ui.label(session.roles.len().to_string());
                ui.end_row();
                ui.label("Phase");
                ui.label(view.phase.clone().unwrap_or_else(|| "none".to_string()));
                ui.end_row();
                if let Some((range, rel_speed)) = relative_state(&view.bodies) {
                    ui.label("Range to tether root");
                    ui.label(format!("{range:.2} m"));
                    ui.end_row();
                    ui.label("Relative speed");
                    ui.label(format!("{rel_speed:.2} m/s"));
                    ui.end_row();
                }
                ui.label("Capture sphere radius");
                ui.label(format!("{:.2} m", view.radius));
                ui.end_row();
            });
            if view.interpolated {
                ui.colored_label(egui::Color32::LIGHT_BLUE, "interpolated between frames");
            }
            ui.horizontal(|ui| {
                // The sidebar (and its view button) is gone in a replay.
                if ui
                    .button(if on_map { "Detail view" } else { "Map view" })
                    .on_hover_text("M")
                    .clicked()
                {
                    camera.ui_events.write(UiEvent::ToggleMapView);
                }
                if ui
                    .button(if on_debris {
                        "Follow tether root"
                    } else {
                        "Follow debris"
                    })
                    .clicked()
                {
                    follow = followed_body(&session.roles, !on_debris);
                }
                if ui.button("Exit replay").clicked() {
                    exit = true;
                }
            });
        });
    for action in &actions {
        apply_transport(&mut session.player, action);
    }
    if let Some(index) = follow {
        camera.follow(index);
    }
    if exit {
        exit_replay(&mut next_sim_state, &mut next_screen, &mut return_to_sim);
    }
}
