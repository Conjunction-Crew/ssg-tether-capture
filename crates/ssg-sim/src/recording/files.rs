//! The recording files (P7, design R3): the file sink that writes
//! `recordings/<name>/` (as `<name>.partial/`, renamed when the recording ends),
//! the readers for each file and the checks `ssg-sim validate` runs (R8).
//!
//! ```text
//! recordings/<name>/
//!   manifest.json      RecordingManifest v1
//!   inputs.jsonl       the session's inputs from step 0 (SessionStart first)
//!   keyframes.parquet  one row per body per frame, 64 frames per row group
//!   frames.jsonl       one line per frame
//!   transitions.jsonl  one line per capture transition in the segment
//!   log.jsonl          the segment's log events
//! ```
//!
//! The keyframe table is Parquet 60 through its low-level writer: PLAIN encoding,
//! no dictionary and no codec, so every DOUBLE is stored as its 8 bytes and the
//! table is exact. JSON numbers are written in Rust's shortest round-trip form.
//! There is no CSV fallback: a short last row group is written as such.

use std::fs::{self, File};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use bevy::math::{DQuat, DVec3};
use parquet::basic::{Compression, Encoding};
use parquet::column::reader::ColumnReader;
use parquet::data_type::{DoubleType, Int32Type, Int64Type};
use parquet::file::properties::{EnabledStatistics, WriterProperties};
use parquet::file::reader::{FileReader, SerializedFileReader};
use parquet::file::writer::SerializedFileWriter;
use parquet::schema::parser::parse_message_type;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::capture::plan::CapturePlan;
use crate::project::export::sanitize_file_stem;
use crate::project::format_utc;
use crate::project::manifest::{BuiltWith, ProjectManifest};
use crate::project::recording::{
    RECORDING_FRAMES_FILE, RECORDING_INPUTS_FILE, RECORDING_KEYFRAMES_PARQUET, RECORDING_LOG_FILE,
    RECORDING_MANIFEST_FILE, RECORDING_PARTIAL_SUFFIX, RECORDING_SCHEMA_VERSION,
    RECORDING_TRANSITIONS_FILE, RecordingBody, RecordingBodyRole, RecordingEndReason,
    RecordingLimits, RecordingManifest, RecordingSource, RecordingStartTrigger,
    read_recording_file, save_recording_manifest,
};
use crate::recording::inputs::{RecordedCommand, RecordedInput};
use crate::recording::recorder::RecordingSettings;
use crate::recording::{
    BYTES_PER_BODY_FRAME, BodyFrame, EndReason, Frame, FrameSink, RecordedTransition, RecordingEnd,
    RecordingHeader, StartTrigger,
};
use crate::telemetry::LogEntry;

/// Frames in one row group of the keyframe table; the last row group holds the
/// remaining frames.
pub const ROW_GROUP_FRAMES: usize = 64;

/// The keyframe table's columns after `step` and `body`, in file order: one body's
/// position, rotation (x, y, z, w), linear and angular velocity.
pub const KEYFRAME_VALUE_COLUMNS: [&str; 13] = [
    "position_m_x",
    "position_m_y",
    "position_m_z",
    "rotation_x",
    "rotation_y",
    "rotation_z",
    "rotation_w",
    "velocity_mps_x",
    "velocity_mps_y",
    "velocity_mps_z",
    "angular_velocity_radps_x",
    "angular_velocity_radps_y",
    "angular_velocity_radps_z",
];

const KEYFRAME_SCHEMA: &str = "message keyframe {
    REQUIRED INT64 step;
    REQUIRED INT32 body;
    REQUIRED DOUBLE position_m_x;
    REQUIRED DOUBLE position_m_y;
    REQUIRED DOUBLE position_m_z;
    REQUIRED DOUBLE rotation_x;
    REQUIRED DOUBLE rotation_y;
    REQUIRED DOUBLE rotation_z;
    REQUIRED DOUBLE rotation_w;
    REQUIRED DOUBLE velocity_mps_x;
    REQUIRED DOUBLE velocity_mps_y;
    REQUIRED DOUBLE velocity_mps_z;
    REQUIRED DOUBLE angular_velocity_radps_x;
    REQUIRED DOUBLE angular_velocity_radps_y;
    REQUIRED DOUBLE angular_velocity_radps_z;
}";

fn parquet_error(e: impl std::fmt::Display) -> String {
    format!("{RECORDING_KEYFRAMES_PARQUET}: {e}")
}

// ─── The keyframe table ───────────────────────────────────────────────────────

/// One row of the keyframe table: a body's end-of-step state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KeyframeRow {
    pub step: i64,
    pub body: i32,
    pub values: [f64; 13],
}

impl KeyframeRow {
    /// The row of `body` at `step`.
    pub fn new(step: u64, body: usize, state: &BodyFrame) -> Self {
        let (p, r, v, w) = (
            state.position,
            state.rotation,
            state.velocity,
            state.angular_velocity,
        );
        Self {
            step: step as i64,
            body: body as i32,
            values: [
                p.x, p.y, p.z, r.x, r.y, r.z, r.w, v.x, v.y, v.z, w.x, w.y, w.z,
            ],
        }
    }

    /// The body's state.
    pub fn state(&self) -> BodyFrame {
        let v = &self.values;
        BodyFrame {
            position: DVec3::new(v[0], v[1], v[2]),
            rotation: DQuat::from_xyzw(v[3], v[4], v[5], v[6]),
            velocity: DVec3::new(v[7], v[8], v[9]),
            angular_velocity: DVec3::new(v[10], v[11], v[12]),
        }
    }
}

/// Writes `keyframes.parquet`: [`ROW_GROUP_FRAMES`] frames per row group, the
/// remaining frames in a last, shorter one.
pub struct KeyframeWriter {
    writer: SerializedFileWriter<BufWriter<File>>,
    steps: Vec<i64>,
    bodies: Vec<i32>,
    columns: [Vec<f64>; 13],
    frames_buffered: usize,
    rows: u64,
}

impl KeyframeWriter {
    /// Creates the file at `path`.
    pub fn create(path: &Path) -> Result<Self, String> {
        let schema = Arc::new(parse_message_type(KEYFRAME_SCHEMA).map_err(parquet_error)?);
        let properties = Arc::new(
            WriterProperties::builder()
                .set_dictionary_enabled(false)
                .set_encoding(Encoding::PLAIN)
                .set_compression(Compression::UNCOMPRESSED)
                .set_statistics_enabled(EnabledStatistics::None)
                .build(),
        );
        let file =
            File::create(path).map_err(|e| format!("cannot create {}: {e}", path.display()))?;
        let writer = SerializedFileWriter::new(BufWriter::new(file), schema, properties)
            .map_err(parquet_error)?;
        Ok(Self {
            writer,
            steps: Vec::new(),
            bodies: Vec::new(),
            columns: Default::default(),
            frames_buffered: 0,
            rows: 0,
        })
    }

    /// Adds one frame (one row per body, in body order); writes a row group once
    /// [`ROW_GROUP_FRAMES`] frames are buffered.
    pub fn push_frame(&mut self, step: u64, bodies: &[BodyFrame]) -> Result<(), String> {
        for (index, state) in bodies.iter().enumerate() {
            let row = KeyframeRow::new(step, index, state);
            self.steps.push(row.step);
            self.bodies.push(row.body);
            for (column, value) in self.columns.iter_mut().zip(row.values) {
                column.push(value);
            }
        }
        self.frames_buffered += 1;
        if self.frames_buffered == ROW_GROUP_FRAMES {
            self.flush_group()?;
        }
        Ok(())
    }

    fn flush_group(&mut self) -> Result<(), String> {
        if self.steps.is_empty() {
            return Ok(());
        }
        let mut group = self.writer.next_row_group().map_err(parquet_error)?;
        let mut index = 0;
        while let Some(mut column) = group.next_column().map_err(parquet_error)? {
            match index {
                0 => column
                    .typed::<Int64Type>()
                    .write_batch(&self.steps, None, None),
                1 => column
                    .typed::<Int32Type>()
                    .write_batch(&self.bodies, None, None),
                n => column
                    .typed::<DoubleType>()
                    .write_batch(&self.columns[n - 2], None, None),
            }
            .map_err(parquet_error)?;
            column.close().map_err(parquet_error)?;
            index += 1;
        }
        group.close().map_err(parquet_error)?;
        self.rows += self.steps.len() as u64;
        self.steps.clear();
        self.bodies.clear();
        self.columns.iter_mut().for_each(Vec::clear);
        self.frames_buffered = 0;
        Ok(())
    }

    /// Writes the last (short) row group and the footer; the rows in the table.
    pub fn finish(mut self) -> Result<u64, String> {
        self.flush_group()?;
        self.writer.close().map_err(parquet_error)?;
        Ok(self.rows)
    }
}

/// Reads `keyframes.parquet` row group by row group and hands every row to `each`;
/// the number of row groups. `Err` when the file is not a keyframe table.
pub fn read_keyframes(path: &Path, mut each: impl FnMut(KeyframeRow)) -> Result<usize, String> {
    let file = File::open(path).map_err(|e| format!("cannot open {}: {e}", path.display()))?;
    let reader = SerializedFileReader::new(file).map_err(parquet_error)?;
    let schema = reader.metadata().file_metadata().schema_descr_ptr();
    let expected: Vec<&str> = ["step", "body"]
        .into_iter()
        .chain(KEYFRAME_VALUE_COLUMNS)
        .collect();
    let found: Vec<&str> = schema.columns().iter().map(|c| c.name()).collect();
    if found != expected {
        return Err(parquet_error(format!(
            "columns are [{}], expected [{}]",
            found.join(", "),
            expected.join(", ")
        )));
    }
    let groups = reader.num_row_groups();
    for g in 0..groups {
        let group = reader.get_row_group(g).map_err(parquet_error)?;
        let n = group.metadata().num_rows() as usize;
        let mut steps: Vec<i64> = Vec::with_capacity(n);
        let mut bodies: Vec<i32> = Vec::with_capacity(n);
        let mut columns: Vec<Vec<f64>> = (0..13).map(|_| Vec::with_capacity(n)).collect();
        for index in 0..expected.len() {
            let read = match group.get_column_reader(index).map_err(parquet_error)? {
                ColumnReader::Int64ColumnReader(mut r) => r.read_records(n, None, None, &mut steps),
                ColumnReader::Int32ColumnReader(mut r) => {
                    r.read_records(n, None, None, &mut bodies)
                }
                ColumnReader::DoubleColumnReader(mut r) => {
                    r.read_records(n, None, None, &mut columns[index - 2])
                }
                _ => {
                    return Err(parquet_error(format!(
                        "column {index} has an unexpected type"
                    )));
                }
            }
            .map_err(parquet_error)?;
            if read.0 != n {
                return Err(parquet_error(format!(
                    "row group {g}: column {} has {} of {n} rows",
                    expected[index], read.0
                )));
            }
        }
        for i in 0..n {
            each(KeyframeRow {
                step: steps[i],
                body: bodies[i],
                values: std::array::from_fn(|c| columns[c][i]),
            });
        }
    }
    Ok(groups)
}

// ─── The JSON Lines files ─────────────────────────────────────────────────────

/// A line of `frames.jsonl` (R3): the frame's scalars.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct FrameLine {
    pub step: u64,
    pub epoch_utc: String,
    /// The target's capture phase; `null` before a capture starts.
    pub phase: Option<String>,
    pub capture_sphere_radius_m: f64,
    /// The frame origin's ECI state; `null` before the anchor has a propagator.
    pub frame_origin_eci: Option<[f64; 6]>,
    pub origin_shift_m: [f64; 3],
    pub origin_shift_mps: [f64; 3],
}

impl FrameLine {
    /// The line of `frame`.
    pub fn new(frame: &Frame) -> Self {
        Self {
            step: frame.step,
            epoch_utc: format_utc(frame.epoch),
            phase: frame.phase.clone(),
            capture_sphere_radius_m: frame.capture_sphere_radius_m,
            frame_origin_eci: frame
                .frame_origin_eci
                .map(|state| std::array::from_fn(|i| state[i])),
            origin_shift_m: frame.origin_shift_m.to_array(),
            origin_shift_mps: frame.origin_shift_mps.to_array(),
        }
    }
}

/// A line of `transitions.jsonl` (R3).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct TransitionLine {
    pub step: u64,
    pub epoch_utc: String,
    pub from: String,
    pub to: String,
    pub reason: String,
}

impl TransitionLine {
    /// The line of `transition`.
    pub fn new(transition: &RecordedTransition) -> Self {
        Self {
            step: transition.step,
            epoch_utc: format_utc(transition.epoch),
            from: transition.from.clone(),
            to: transition.to.clone(),
            reason: transition.reason.clone(),
        }
    }
}

fn write_line<T: Serialize>(out: &mut impl Write, value: &T) -> Result<(), String> {
    serde_json::to_writer(&mut *out, value).map_err(|e| e.to_string())?;
    out.write_all(b"\n").map_err(|e| e.to_string())
}

/// Reads a JSON Lines file: one `T` per non-blank line, `Err` naming the file and
/// line otherwise. Line endings are LF or CR LF.
pub fn read_jsonl<T: DeserializeOwned>(path: &Path) -> Result<Vec<T>, String> {
    let name = path
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let file = File::open(path).map_err(|e| format!("{name}: cannot open: {e}"))?;
    let mut values = Vec::new();
    for (index, line) in BufReader::new(file).lines().enumerate() {
        let line = line.map_err(|e| format!("{name}: line {}: {e}", index + 1))?;
        if line.trim().is_empty() {
            continue;
        }
        values.push(
            serde_json::from_str(&line).map_err(|e| format!("{name}: line {}: {e}", index + 1))?,
        );
    }
    Ok(values)
}

fn write_jsonl<T: Serialize>(path: &Path, values: &[T]) -> Result<(), String> {
    let file = File::create(path).map_err(|e| format!("cannot create {}: {e}", path.display()))?;
    let mut out = BufWriter::new(file);
    for value in values {
        write_line(&mut out, value)?;
    }
    out.flush().map_err(|e| e.to_string())
}

// ─── The file sink ────────────────────────────────────────────────────────────

/// What the sink knows about the run besides the frames.
#[derive(Debug, Clone)]
pub struct RecordingContext {
    /// The folder `recordings/` of the project; created on demand.
    pub recordings_dir: PathBuf,
    /// The name of a recording that has none of its own (an automatic start).
    pub default_name: String,
    pub source: RecordingSource,
    pub plan_id: String,
    pub plan: CapturePlan,
    /// The scenario as the run used it.
    pub project: Option<ProjectManifest>,
    /// The settings in force (the manifest records their limits).
    pub settings: RecordingSettings,
    /// The display name of the target body.
    pub target_name: String,
}

/// A recording the sink wrote.
#[derive(Debug, Clone, PartialEq)]
pub struct WrittenRecording {
    pub dir: PathBuf,
    pub frames: u64,
    pub bodies: usize,
    pub end: RecordingEnd,
    /// The size of the keyframe table in bytes.
    pub keyframes_bytes: u64,
}

struct Active {
    header: RecordingHeader,
    partial: PathBuf,
    done: PathBuf,
    keyframes: Option<KeyframeWriter>,
    frames: Option<BufWriter<File>>,
    transitions: Vec<TransitionLine>,
    logs: Vec<LogEntry>,
    inputs: Vec<RecordedInput>,
    start_epoch_utc: Option<String>,
    frame_count: u64,
    /// The first error; later frames are dropped.
    error: Option<String>,
}

#[derive(Default)]
struct Inner {
    context: Option<RecordingContext>,
    current: Option<Active>,
    results: Vec<Result<WrittenRecording, String>>,
}

/// Writes recordings to disk. Clones share the state, so the caller keeps one to
/// read the [`results`](Self::results) after the run.
#[derive(Clone)]
pub struct FileSink {
    inner: Arc<Mutex<Inner>>,
}

impl FileSink {
    /// A sink writing under `context.recordings_dir`.
    pub fn new(context: RecordingContext) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                context: Some(context),
                ..Inner::default()
            })),
        }
    }

    /// One entry per recording that ended, oldest first: what was written, or why
    /// it could not be.
    pub fn results(&self) -> Vec<Result<WrittenRecording, String>> {
        self.lock().results.clone()
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Active {
    fn fail(&mut self, result: Result<(), String>) {
        if let Err(e) = result {
            self.error.get_or_insert(e);
        }
    }
}

/// The first of `name`, `name-2`, `name-3`, … that has no folder yet.
fn free_name(dir: &Path, name: &str) -> String {
    let taken = |n: &str| {
        dir.join(n).exists() || dir.join(format!("{n}{RECORDING_PARTIAL_SUFFIX}")).exists()
    };
    if !taken(name) {
        return name.to_string();
    }
    (2..)
        .map(|i| format!("{name}-{i}"))
        .find(|n| !taken(n))
        .expect("an unbounded range has a free name")
}

fn body_names(roles: &[RecordingBodyRole], target: &str) -> Vec<RecordingBody> {
    let mut nodes = 0;
    roles
        .iter()
        .enumerate()
        .map(|(index, &role)| RecordingBody {
            index: index as u32,
            role,
            name: match role {
                RecordingBodyRole::Target => target.to_string(),
                RecordingBodyRole::TetherRoot => "Tether root".to_string(),
                RecordingBodyRole::TetherNode => {
                    nodes += 1;
                    format!("Tether node {nodes}")
                }
                RecordingBodyRole::TetherTail => "Tether tail".to_string(),
            },
        })
        .collect()
}

impl From<EndReason> for RecordingEndReason {
    fn from(reason: EndReason) -> Self {
        match reason {
            EndReason::Manual => Self::Manual,
            EndReason::EndSuccess => Self::EndSuccess,
            EndReason::EndAbort => Self::EndAbort,
            EndReason::MaxDuration => Self::MaxDuration,
            EndReason::MaxSize => Self::MaxSize,
            EndReason::SessionEnd => Self::SessionEnd,
        }
    }
}

impl From<StartTrigger> for RecordingStartTrigger {
    fn from(trigger: StartTrigger) -> Self {
        match trigger {
            StartTrigger::Manual => Self::Manual,
            StartTrigger::Phase => Self::Phase,
        }
    }
}

impl Inner {
    fn begin(&mut self, header: &RecordingHeader) {
        let Some(context) = &self.context else {
            return;
        };
        let dir = &context.recordings_dir;
        let stem = sanitize_file_stem(header.name.as_deref().unwrap_or(&context.default_name));
        // A recording the caller named never overwrites another.
        let name = if header.name.is_some() {
            stem
        } else {
            free_name(dir, &stem)
        };
        let done = dir.join(&name);
        let partial = dir.join(format!("{name}{RECORDING_PARTIAL_SUFFIX}"));
        let mut active = Active {
            header: header.clone(),
            partial: partial.clone(),
            done: done.clone(),
            keyframes: None,
            frames: None,
            transitions: Vec::new(),
            logs: Vec::new(),
            inputs: Vec::new(),
            start_epoch_utc: None,
            frame_count: 0,
            error: None,
        };
        let opened = (|| {
            if done.exists() || partial.exists() {
                return Err(format!("recording `{name}` already exists"));
            }
            fs::create_dir_all(&partial)
                .map_err(|e| format!("cannot create {}: {e}", partial.display()))?;
            let table = KeyframeWriter::create(&partial.join(RECORDING_KEYFRAMES_PARQUET))?;
            let frames = File::create(partial.join(RECORDING_FRAMES_FILE))
                .map_err(|e| format!("cannot create {RECORDING_FRAMES_FILE}: {e}"))?;
            Ok((table, BufWriter::new(frames)))
        })();
        match opened {
            Ok((table, frames)) => {
                active.keyframes = Some(table);
                active.frames = Some(frames);
            }
            Err(e) => active.error = Some(e),
        }
        self.current = Some(active);
    }

    fn frame(&mut self, frame: &Frame) {
        let Some(active) = self.current.as_mut().filter(|a| a.error.is_none()) else {
            return;
        };
        active
            .start_epoch_utc
            .get_or_insert_with(|| format_utc(frame.epoch));
        active.frame_count += 1;
        let result = active
            .keyframes
            .as_mut()
            .map(|table| table.push_frame(frame.step, &frame.bodies))
            .unwrap_or(Ok(()))
            .and_then(|()| match active.frames.as_mut() {
                Some(out) => write_line(out, &FrameLine::new(frame))
                    .map_err(|e| format!("{RECORDING_FRAMES_FILE}: {e}")),
                None => Ok(()),
            });
        active.fail(result);
    }

    fn finish(&mut self, end: &RecordingEnd) {
        let (Some(mut active), Some(context)) = (self.current.take(), self.context.as_ref()) else {
            return;
        };
        if active.error.is_none() {
            let result = active.write_rest(context, end);
            active.fail(result);
        }
        self.results.push(match active.error {
            Some(e) => Err(e),
            None => {
                let keyframes_bytes = fs::metadata(active.done.join(RECORDING_KEYFRAMES_PARQUET))
                    .map_or(0, |m| m.len());
                Ok(WrittenRecording {
                    dir: active.done,
                    frames: active.frame_count,
                    bodies: active.header.roles.len(),
                    end: *end,
                    keyframes_bytes,
                })
            }
        });
    }
}

impl Active {
    /// Closes the keyframe table and `frames.jsonl`, writes the other files and the
    /// manifest, and renames the folder.
    fn write_rest(&mut self, context: &RecordingContext, end: &RecordingEnd) -> Result<(), String> {
        if let Some(table) = self.keyframes.take() {
            table.finish()?;
        }
        if let Some(mut frames) = self.frames.take() {
            frames
                .flush()
                .map_err(|e| format!("{RECORDING_FRAMES_FILE}: {e}"))?;
        }
        write_jsonl(&self.partial.join(RECORDING_INPUTS_FILE), &self.inputs)?;
        write_jsonl(
            &self.partial.join(RECORDING_TRANSITIONS_FILE),
            &self.transitions,
        )?;
        write_jsonl(&self.partial.join(RECORDING_LOG_FILE), &self.logs)?;
        let header = &self.header;
        let settings = &context.settings;
        let manifest = RecordingManifest {
            schema_version: RECORDING_SCHEMA_VERSION,
            built_with: BuiltWith::current(),
            plan_id: context.plan_id.clone(),
            plan: context.plan.clone(),
            start_step: header.start_step,
            start_epoch_utc: self
                .start_epoch_utc
                .clone()
                .ok_or("the recording has no frame")?,
            keyframe_interval_steps: header.keyframe_interval_steps,
            bodies: body_names(&header.roles, &context.target_name),
            limits: RecordingLimits {
                max_duration_s: settings.max_duration_s,
                max_tether_bodies: settings.max_tether_bodies,
                max_size_mb: Some(settings.max_size_mb),
            },
            project: context.project.clone(),
            source: Some(context.source),
            end_step: Some(end.end_step),
            end_reason: Some(end.reason.into()),
            start_trigger: Some(header.trigger.into()),
            start_phase: header.start_phase.clone(),
        };
        let errors = manifest.validate();
        if !errors.is_empty() {
            return Err(format!("the manifest is not valid: {}", errors.join("; ")));
        }
        save_recording_manifest(&self.partial, &manifest)
            .map_err(|e| format!("cannot write {RECORDING_MANIFEST_FILE}: {e}"))?;
        fs::rename(&self.partial, &self.done).map_err(|e| {
            format!(
                "cannot rename {} to {}: {e}",
                self.partial.display(),
                self.done.display()
            )
        })
    }
}

impl FrameSink for FileSink {
    fn begin(&mut self, header: &RecordingHeader) {
        self.lock().begin(header);
    }

    fn frame(&mut self, frame: &Frame) {
        self.lock().frame(frame);
    }

    fn transition(&mut self, transition: &RecordedTransition) {
        if let Some(active) = self.lock().current.as_mut() {
            active.transitions.push(TransitionLine::new(transition));
        }
    }

    fn log(&mut self, entry: &LogEntry) {
        if let Some(active) = self.lock().current.as_mut() {
            active.logs.push(entry.clone());
        }
    }

    fn inputs(&mut self, inputs: &[RecordedInput]) {
        if let Some(active) = self.lock().current.as_mut() {
            active.inputs = inputs.to_vec();
        }
    }

    fn bytes(&self) -> u64 {
        self.lock().current.as_ref().map_or(0, |a| {
            a.frame_count * a.header.roles.len() as u64 * BYTES_PER_BODY_FRAME
        })
    }

    fn finish(&mut self, end: &RecordingEnd) {
        self.lock().finish(end);
    }
}

// ─── Reading a recording ──────────────────────────────────────────────────────

/// One frame as read from the files.
#[derive(Debug, Clone, PartialEq)]
pub struct RecordedFrame {
    pub line: FrameLine,
    /// One entry per body, in manifest order.
    pub bodies: Vec<BodyFrame>,
}

/// A recording read from its folder.
#[derive(Debug, Clone, PartialEq)]
pub struct LoadedRecording {
    pub manifest: RecordingManifest,
    pub inputs: Vec<RecordedInput>,
    pub frames: Vec<RecordedFrame>,
    pub transitions: Vec<TransitionLine>,
}

/// Reads the recording in `dir`. `inputs.jsonl` and `transitions.jsonl` may be
/// missing (an empty list); the manifest, `keyframes.parquet` and `frames.jsonl`
/// may not, and must agree. `Err` carries every message.
pub fn read_recording(dir: &Path) -> Result<LoadedRecording, Vec<String>> {
    let report = read_recording_file(dir).ok_or_else(|| {
        vec![format!(
            "{RECORDING_MANIFEST_FILE}: no such file in {}",
            dir.display()
        )]
    })?;
    let manifest = report.manifest.ok_or(report.errors)?;
    let optional = |name: &str| dir.join(name).is_file();
    let inputs = if optional(RECORDING_INPUTS_FILE) {
        read_jsonl(&dir.join(RECORDING_INPUTS_FILE)).map_err(|e| vec![e])?
    } else {
        Vec::new()
    };
    let transitions = if optional(RECORDING_TRANSITIONS_FILE) {
        read_jsonl(&dir.join(RECORDING_TRANSITIONS_FILE)).map_err(|e| vec![e])?
    } else {
        Vec::new()
    };
    let lines: Vec<FrameLine> =
        read_jsonl(&dir.join(RECORDING_FRAMES_FILE)).map_err(|e| vec![e])?;
    let bodies = manifest.bodies.len();
    let mut table: Vec<KeyframeRow> = Vec::new();
    read_keyframes(&dir.join(RECORDING_KEYFRAMES_PARQUET), |row| {
        table.push(row)
    })
    .map_err(|e| vec![e])?;
    if table.len() != bodies * lines.len() {
        return Err(vec![format!(
            "{RECORDING_KEYFRAMES_PARQUET} has {} rows; {bodies} bodies × {} frames in \
             {RECORDING_FRAMES_FILE} would be {}",
            table.len(),
            lines.len(),
            bodies * lines.len()
        )]);
    }
    let mut frames = Vec::with_capacity(lines.len());
    for (line, rows) in lines.into_iter().zip(table.chunks(bodies.max(1))) {
        if let Some(row) = rows
            .iter()
            .enumerate()
            .find(|(i, row)| row.step != line.step as i64 || row.body != *i as i32)
            .map(|(_, row)| row)
        {
            return Err(vec![format!(
                "{RECORDING_KEYFRAMES_PARQUET}: row (step {}, body {}) does not belong to frame {} \
                 of {RECORDING_FRAMES_FILE}",
                row.step, row.body, line.step
            )]);
        }
        frames.push(RecordedFrame {
            bodies: rows.iter().map(KeyframeRow::state).collect(),
            line,
        });
    }
    Ok(LoadedRecording {
        manifest,
        inputs,
        frames,
        transitions,
    })
}

// ─── The checks of `ssg-sim validate` (R8) ───────────────────────────────────

/// What checking one recording folder found.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RecordingCheck {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    /// One line about a recording that has no errors.
    pub summary: Option<String>,
}

/// Checks the recording folder `dir`, named `label` in the messages (R8): the
/// manifest is valid; `keyframes.parquet` is there with `rows = bodies × frames`,
/// strictly increasing steps and `(step − start_step)` a multiple of K except in
/// the last frame; `frames.jsonl` has the table's steps; `inputs.jsonl` starts with
/// `SessionStart` (missing: a warning when the recording's `source` is `cli`, an
/// error otherwise) and is ordered by step; `transitions.jsonl` is ordered and
/// inside `[start_step, end_step]`.
pub fn check_recording(dir: &Path, label: &str) -> RecordingCheck {
    let mut check = RecordingCheck::default();
    let at = |file: &str| format!("[{label}/{file}]");
    let Some(report) = read_recording_file(dir) else {
        check
            .errors
            .push(format!("{} missing", at(RECORDING_MANIFEST_FILE)));
        return check;
    };
    let Some(manifest) = report.manifest else {
        check.errors.extend(report.errors.iter().map(|e| {
            e.replacen(
                &format!("[{RECORDING_MANIFEST_FILE}]"),
                &at(RECORDING_MANIFEST_FILE),
                1,
            )
        }));
        return check;
    };
    let bodies = manifest.bodies.len() as u64;
    let interval = u64::from(manifest.keyframe_interval_steps.max(1));

    // The keyframe table: the step of each frame (it starts at body 0), in file order.
    let mut table_steps: Vec<i64> = Vec::new();
    let mut rows = 0u64;
    let table = dir.join(RECORDING_KEYFRAMES_PARQUET);
    if !table.is_file() {
        check
            .errors
            .push(format!("{} missing", at(RECORDING_KEYFRAMES_PARQUET)));
    } else {
        let read = read_keyframes(&table, |row| {
            rows += 1;
            if row.body == 0 {
                table_steps.push(row.step);
            }
        });
        match read {
            Err(e) => check
                .errors
                .push(format!("{} {e}", at(RECORDING_KEYFRAMES_PARQUET))),
            Ok(_) => check_table(
                &manifest,
                &table_steps,
                rows,
                bodies,
                interval,
                &at,
                &mut check,
            ),
        }
    }

    // frames.jsonl
    match read_jsonl::<FrameLine>(&dir.join(RECORDING_FRAMES_FILE)) {
        Err(e) => check
            .errors
            .push(format!("{} {e}", at(RECORDING_FRAMES_FILE))),
        Ok(lines) => {
            let steps: Vec<i64> = lines.iter().map(|l| l.step as i64).collect();
            if table.is_file() && check.errors.is_empty() && steps != table_steps {
                check.errors.push(format!(
                    "{} has {} frames; the keyframe table has {}, or their steps differ",
                    at(RECORDING_FRAMES_FILE),
                    steps.len(),
                    table_steps.len()
                ));
            }
        }
    }

    // inputs.jsonl
    let inputs_path = dir.join(RECORDING_INPUTS_FILE);
    let inputs = if inputs_path.is_file() {
        match read_jsonl::<RecordedInput>(&inputs_path) {
            Ok(inputs) => Some(inputs),
            Err(e) => {
                check
                    .errors
                    .push(format!("{} {e}", at(RECORDING_INPUTS_FILE)));
                None
            }
        }
    } else {
        Some(Vec::new())
    };
    if let Some(inputs) = inputs {
        if !matches!(
            inputs.first().map(|i| &i.command),
            Some(RecordedCommand::SessionStart { .. })
        ) {
            let text = format!(
                "{} does not start with a `session_start` entry (the spawn-update step count)",
                at(RECORDING_INPUTS_FILE)
            );
            if manifest.source == Some(RecordingSource::Cli) {
                check.warnings.push(format!("{text}; k = 1 is assumed"));
            } else {
                check.errors.push(text);
            }
        }
        if inputs.windows(2).any(|w| w[1].after_step < w[0].after_step) {
            check.errors.push(format!(
                "{} is not ordered by step",
                at(RECORDING_INPUTS_FILE)
            ));
        }
    }

    // transitions.jsonl
    let transitions_path = dir.join(RECORDING_TRANSITIONS_FILE);
    if !transitions_path.is_file() {
        check.warnings.push(format!(
            "{} missing (playback falls back to the phase of the frame before the cursor)",
            at(RECORDING_TRANSITIONS_FILE)
        ));
    } else {
        match read_jsonl::<TransitionLine>(&transitions_path) {
            Err(e) => check
                .errors
                .push(format!("{} {e}", at(RECORDING_TRANSITIONS_FILE))),
            Ok(lines) => {
                let end = manifest.end_step.or(table_steps.last().map(|s| *s as u64));
                if let Some(t) = lines
                    .iter()
                    .find(|t| t.step < manifest.start_step || end.is_some_and(|e| t.step > e))
                {
                    check.errors.push(format!(
                        "{} step {} is outside the recording's steps {}..{}",
                        at(RECORDING_TRANSITIONS_FILE),
                        t.step,
                        manifest.start_step,
                        end.map_or_else(|| "?".to_string(), |e| e.to_string())
                    ));
                }
                if lines.windows(2).any(|w| w[1].step < w[0].step) {
                    check.errors.push(format!(
                        "{} is not ordered by step",
                        at(RECORDING_TRANSITIONS_FILE)
                    ));
                }
            }
        }
    }

    // log.jsonl: every line is a JSON object.
    let log_path = dir.join(RECORDING_LOG_FILE);
    if log_path.is_file()
        && let Err(e) = read_jsonl::<serde_json::Map<String, serde_json::Value>>(&log_path)
    {
        check.errors.push(format!("{} {e}", at(RECORDING_LOG_FILE)));
    }

    if check.errors.is_empty() {
        check.summary = Some(format!(
            "{label}: {} frames, {bodies} bodies, steps {}..{}",
            table_steps.len(),
            manifest.start_step,
            table_steps
                .last()
                .map_or(manifest.start_step, |s| *s as u64)
        ));
    }
    check
}

fn check_table(
    manifest: &RecordingManifest,
    steps: &[i64],
    rows: u64,
    bodies: u64,
    interval: u64,
    at: &dyn Fn(&str) -> String,
    check: &mut RecordingCheck,
) {
    let file = at(RECORDING_KEYFRAMES_PARQUET);
    if rows != bodies * steps.len() as u64 {
        check.errors.push(format!(
            "{file} has {rows} rows; {bodies} bodies × {} frames would be {}",
            steps.len(),
            bodies * steps.len() as u64
        ));
    }
    if let Some(pair) = steps.windows(2).find(|w| w[1] <= w[0]) {
        check.errors.push(format!(
            "{file} steps are not strictly increasing ({} then {})",
            pair[0], pair[1]
        ));
    }
    if steps
        .first()
        .is_some_and(|s| *s as u64 != manifest.start_step)
    {
        check.errors.push(format!(
            "{file} starts at step {}, not at start_step {}",
            steps[0], manifest.start_step
        ));
    }
    // A first step off start_step is reported above; the cadence is measured from it.
    let last = steps.len().saturating_sub(1);
    let aligned = steps
        .first()
        .is_none_or(|s| *s as u64 == manifest.start_step);
    if let Some(step) = steps.iter().take(last).filter(|_| aligned).find(|s| {
        (**s as u64)
            .checked_sub(manifest.start_step)
            .is_none_or(|d| d % interval != 0)
    }) {
        check.errors.push(format!(
            "{file} step {step} is not start_step {} plus a multiple of {interval}",
            manifest.start_step
        ));
    }
    if let (Some(end), Some(step)) = (manifest.end_step, steps.last())
        && end != *step as u64
    {
        check
            .errors
            .push(format!("{file} ends at step {step}, not at end_step {end}"));
    }
}

/// The folders under `recordings_dir`, finished ones and `.partial` ones, sorted
/// by name; empty when the folder does not exist.
pub fn list_recording_dirs(recordings_dir: &Path) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = fs::read_dir(recordings_dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    dirs.sort();
    dirs
}
