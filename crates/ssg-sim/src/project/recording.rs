//! The recording manifest, `recordings/<name>/manifest.json` (schema v1, design D8):
//! the file model, the reader and validator, and the writer.
//!
//! P6 publishes the layout and this type; P7 writes recordings with it. A recording
//! folder holds the manifest, the input log and the keyframes:
//!
//! ```text
//! recordings/<name>/
//!   manifest.json      this file
//!   inputs.jsonl       one JSON object per line: {"step": u64, "command": …}
//!   keyframes.parquet  one row per body per keyframe (CSV fallback: keyframes.csv)
//! ```

use std::fs;
use std::path::Path;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::capture::plan::{CapturePlan, validate_capture_plan};
use crate::project::epoch::parse_utc;
use crate::project::manifest::BuiltWith;
use crate::project::plan_files::read_plan_value;

/// The manifest's file name in the recording folder.
pub const RECORDING_MANIFEST_FILE: &str = "manifest.json";

/// The input log's file name: one JSON object per sim-affecting input.
pub const RECORDING_INPUTS_FILE: &str = "inputs.jsonl";

/// The keyframe table's file name (Parquet).
pub const RECORDING_KEYFRAMES_PARQUET: &str = "keyframes.parquet";

/// The keyframe table's file name for tiny segments (CSV, the same columns).
pub const RECORDING_KEYFRAMES_CSV: &str = "keyframes.csv";

/// The `schema_version` of the recording manifests this build reads and writes.
pub const RECORDING_SCHEMA_VERSION: u32 = 1;

/// The id the embedded plan is validated under; messages name it by pointer.
const PLAN_KEY: &str = "plan";

/// The recording manifest, schema v1. Lengths are metres, durations seconds, epochs
/// UTC strings with microseconds.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub struct RecordingManifest {
    /// File format version; `1`.
    #[cfg_attr(test, schemars(range(min = 1, max = 1)))]
    pub schema_version: u32,
    /// The build that recorded; a recording replays bit for bit only on the same
    /// build and platform.
    pub built_with: BuiltWith,
    /// Id (file stem) of the capture plan the recording ran. Must not be blank.
    pub plan_id: String,
    /// A copy of that plan (schema v1) as it was when the recording started.
    pub plan: CapturePlan,
    /// The simulation step the recording starts at.
    pub start_step: u64,
    /// The simulation epoch at `start_step`, `YYYY-MM-DDTHH:MM:SS.ffffffZ` (UTC,
    /// microseconds).
    pub start_epoch_utc: String,
    /// Steps between two keyframes (K); at least 1.
    #[cfg_attr(test, schemars(range(min = 1)))]
    pub keyframe_interval_steps: u32,
    /// The recorded bodies. `index` is the position in this list and the `body`
    /// column of the keyframe table.
    pub bodies: Vec<RecordingBody>,
    /// The caps on the recorded segment.
    pub limits: RecordingLimits,
}

/// One recorded body.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub struct RecordingBody {
    /// Position in `bodies` (0, 1, 2, …): the `body` column of the keyframe table.
    pub index: u32,
    /// What the body is.
    pub role: RecordingBodyRole,
    /// Display name. Must not be blank.
    pub name: String,
}

/// What a recorded body is.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub enum RecordingBodyRole {
    /// The capture target.
    Target,
    /// The tether's root, which the chaser carries.
    TetherRoot,
    /// An interior tether node.
    TetherNode,
    /// The tether's free end.
    TetherTail,
}

/// The caps on a recorded segment: a recording is a short interaction, not a whole
/// approach (the recording budget is sized for 200 tether bodies).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub struct RecordingLimits {
    /// The longest segment in simulation seconds; greater than 0.
    pub max_duration_s: f64,
    /// The most tether bodies (every body except the target); at least 1.
    #[cfg_attr(test, schemars(range(min = 1)))]
    pub max_tether_bodies: u32,
}

/// What reading a recording manifest gave: the manifest when it loads (valid) and
/// the errors, each `[manifest.json] <json-pointer>: <message>`.
#[derive(Debug, Clone, Default)]
pub struct RecordingReport {
    pub manifest: Option<RecordingManifest>,
    pub errors: Vec<String>,
}

fn message(pointer: &str, text: impl std::fmt::Display) -> String {
    format!("[{RECORDING_MANIFEST_FILE}] {pointer}: {text}")
}

/// An embedded plan's messages (`[plan] /phases/0/id: …`) moved under `/plan`.
fn under_plan(messages: Vec<String>) -> Vec<String> {
    let prefix = format!("[{PLAN_KEY}] ");
    messages
        .into_iter()
        .map(|m| match m.strip_prefix(&prefix) {
            Some(rest) => message(&format!("/{PLAN_KEY}{}", pointer_of(rest)), text_of(rest)),
            None => message(&format!("/{PLAN_KEY}"), m),
        })
        .collect()
}

/// `/phases/0/id` of `/phases/0/id: must not be blank`.
fn pointer_of(rest: &str) -> &str {
    rest.split_once(": ").map_or("", |(pointer, _)| pointer)
}

/// `must not be blank` of `/phases/0/id: must not be blank`.
fn text_of(rest: &str) -> &str {
    rest.split_once(": ").map_or(rest, |(_, text)| text)
}

impl RecordingManifest {
    /// Every rule of design D8 that serde cannot check, in field order, as
    /// `[manifest.json] <json-pointer>: <message>`.
    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();
        if self.schema_version != RECORDING_SCHEMA_VERSION {
            errors.push(message(
                "/schema_version",
                format!("must be {RECORDING_SCHEMA_VERSION}"),
            ));
        }
        if self.plan_id.trim().is_empty() {
            errors.push(message("/plan_id", "must not be blank"));
        }
        errors.extend(under_plan(validate_capture_plan(PLAN_KEY, &self.plan)));
        if parse_utc(&self.start_epoch_utc).is_none() {
            errors.push(message(
                "/start_epoch_utc",
                format!(
                    "`{}` is not a UTC epoch (YYYY-MM-DDTHH:MM:SS.ffffffZ)",
                    self.start_epoch_utc
                ),
            ));
        }
        if self.keyframe_interval_steps == 0 {
            errors.push(message("/keyframe_interval_steps", "must be at least 1"));
        }
        if self.bodies.is_empty() {
            errors.push(message("/bodies", "must list at least one body"));
        }
        for (i, body) in self.bodies.iter().enumerate() {
            if body.index as usize != i {
                errors.push(message(
                    &format!("/bodies/{i}/index"),
                    format!("must be {i} (the position in the list), not {}", body.index),
                ));
            }
            if body.name.trim().is_empty() {
                errors.push(message(&format!("/bodies/{i}/name"), "must not be blank"));
            }
        }
        if !(self.limits.max_duration_s.is_finite() && self.limits.max_duration_s > 0.0) {
            errors.push(message("/limits/max_duration_s", "must be greater than 0"));
        }
        if self.limits.max_tether_bodies == 0 {
            errors.push(message("/limits/max_tether_bodies", "must be at least 1"));
        }
        let tether_bodies = self
            .bodies
            .iter()
            .filter(|b| b.role != RecordingBodyRole::Target)
            .count();
        if self.limits.max_tether_bodies > 0
            && tether_bodies > self.limits.max_tether_bodies as usize
        {
            errors.push(message(
                "/bodies",
                format!(
                    "lists {tether_bodies} tether bodies; limits.max_tether_bodies allows {}",
                    self.limits.max_tether_bodies
                ),
            ));
        }
        errors
    }
}

/// The JSON pointer of the part of `value` that does not deserialize, `/` for the
/// document itself (serde's message does not say where).
fn error_pointer(value: &Value) -> String {
    fn fails<T: DeserializeOwned>(value: Option<&Value>) -> bool {
        value.is_some_and(|v| !v.is_null() && T::deserialize(v).is_err())
    }
    if fails::<BuiltWith>(value.get("built_with")) {
        return "/built_with".to_string();
    }
    if fails::<CapturePlan>(value.get(PLAN_KEY)) {
        return format!("/{PLAN_KEY}");
    }
    let bodies = value.get("bodies").and_then(Value::as_array);
    for (i, body) in bodies.into_iter().flatten().enumerate() {
        if fails::<RecordingBody>(Some(body)) {
            return format!("/bodies/{i}");
        }
    }
    if fails::<RecordingLimits>(value.get("limits")) {
        return "/limits".to_string();
    }
    "/".to_string()
}

/// Reads the recording manifest document `value`: the version is checked first,
/// then the document is read (unknown keys refused, the key named) and validated.
/// `manifest` is `Some` only when there are no errors.
pub fn read_recording_value(value: &Value) -> RecordingReport {
    let failed = |errors: Vec<String>| RecordingReport {
        errors,
        manifest: None,
    };
    match value.get("schema_version") {
        None => return failed(vec![message("/schema_version", "missing")]),
        Some(v) if v.as_u64() == Some(u64::from(RECORDING_SCHEMA_VERSION)) => {}
        Some(v)
            if v.as_u64()
                .is_some_and(|n| n > u64::from(RECORDING_SCHEMA_VERSION)) =>
        {
            return failed(vec![message(
                "/schema_version",
                format!(
                    "{v} is not supported; this version reads schema_version \
                     {RECORDING_SCHEMA_VERSION} (the file was written by a newer version)"
                ),
            )]);
        }
        Some(v) => {
            return failed(vec![message(
                "/schema_version",
                format!(
                    "{v} is not supported; this version reads schema_version {RECORDING_SCHEMA_VERSION}"
                ),
            )]);
        }
    }
    // An embedded plan is always v1: a copy is written by the app, never migrated.
    if let Some(plan) = value.get(PLAN_KEY)
        && plan.get("schema_version").and_then(Value::as_u64) != Some(1)
    {
        return failed(vec![message(
            "/plan/schema_version",
            "must be 1 (an embedded plan is always schema v1)",
        )]);
    }
    let mut manifest = match RecordingManifest::deserialize(value) {
        Ok(manifest) => manifest,
        Err(e) => {
            // The plan's own reader names the exact key; the rest is located here.
            if let Some(plan) = value.get(PLAN_KEY)
                && CapturePlan::deserialize(plan).is_err()
            {
                let errors = read_plan_value(PLAN_KEY, plan).errors;
                if !errors.is_empty() {
                    return failed(under_plan(errors));
                }
            }
            return failed(vec![message(&error_pointer(value), e)]);
        }
    };
    manifest.plan.id = manifest.plan_id.clone();
    let errors = manifest.validate();
    RecordingReport {
        manifest: errors.is_empty().then_some(manifest),
        errors,
    }
}

/// Reads `<dir>/manifest.json` of the recording folder `dir`. `None` when there is
/// no such file.
pub fn read_recording_file(dir: &Path) -> Option<RecordingReport> {
    let path = dir.join(RECORDING_MANIFEST_FILE);
    if !path.is_file() {
        return None;
    }
    let value = match fs::read_to_string(&path) {
        Err(e) => {
            return Some(RecordingReport {
                errors: vec![format!(
                    "[{RECORDING_MANIFEST_FILE}] Could not read file: {e}"
                )],
                manifest: None,
            });
        }
        Ok(raw) => match serde_json::from_str::<Value>(&raw) {
            Err(e) => {
                return Some(RecordingReport {
                    errors: vec![format!("[{RECORDING_MANIFEST_FILE}] Invalid JSON: {e}")],
                    manifest: None,
                });
            }
            Ok(value) => value,
        },
    };
    Some(read_recording_value(&value))
}

/// The manifest as a file: pretty JSON, `schema_version` first.
pub fn recording_manifest_to_json(manifest: &RecordingManifest) -> String {
    serde_json::to_string_pretty(manifest).expect("a recording manifest always serializes")
}

/// Writes `manifest` to `<dir>/manifest.json`; `dir` must exist.
pub fn save_recording_manifest(dir: &Path, manifest: &RecordingManifest) -> std::io::Result<()> {
    fs::write(
        dir.join(RECORDING_MANIFEST_FILE),
        recording_manifest_to_json(manifest),
    )
}
