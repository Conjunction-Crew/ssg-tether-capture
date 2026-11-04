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

mod read;
mod sink;

pub use read::*;
pub use sink::*;

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
