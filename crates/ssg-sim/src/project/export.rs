//! Export files (design D7): the CLI's `trajectory.csv`, the app's capture CSV,
//! CCSDS OEM ephemerides and the capture log as JSON lines.

use std::fs;
use std::path::Path;

use brahe::Epoch;
use brahe::ccsds::common::{CCSDSFormat, CCSDSRefFrame, CCSDSTimeSystem};
use brahe::ccsds::oem::{OEM, OEMMetadata, OEMSegment, OEMStateVector};
use nalgebra::Vector6;

use crate::telemetry::LogEntry;

/// The `ORIGINATOR` of every OEM file this program writes.
pub const OEM_ORIGINATOR: &str = "SSG-TETHER-CAPTURE";

/// One row of the CLI's `trajectory.csv`.
#[derive(Debug, Clone, PartialEq)]
pub struct TrajectoryRow<'a> {
    pub step: u64,
    /// `format_utc` of the step's epoch.
    pub epoch_utc: &'a str,
    pub phase: &'a str,
    pub range_m: f64,
    pub rel_speed_mps: f64,
}

/// One OEM segment: a body's ECI (GCRF) states, position in m and velocity in
/// m/s, at increasing epochs (UTC).
#[derive(Debug, Clone)]
pub struct OemObject<'a> {
    pub name: &'a str,
    /// The GP record's `OBJECT_ID`; the name is written when it is `None`.
    pub id: Option<&'a str>,
    pub states: Vec<(Epoch, Vector6<f64>)>,
}

/// `s` with every character other than a letter, a digit, `-` and `_` replaced
/// by `_`, so that it is a valid file name on every platform; `unnamed` when `s`
/// is empty.
pub fn sanitize_file_stem(s: &str) -> String {
    let stem = replace_unsafe(s, '_');
    if stem.is_empty() {
        "unnamed".to_string()
    } else {
        stem
    }
}

fn replace_unsafe(s: &str, with: char) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                with
            }
        })
        .collect()
}

/// Writes the CLI's `trajectory.csv`: header `step,epoch_utc,phase,range_m,rel_speed_mps`,
/// numbers in Rust's shortest round-trip form.
pub fn write_trajectory_csv<'a>(
    path: &Path,
    rows: impl IntoIterator<Item = TrajectoryRow<'a>>,
) -> Result<(), String> {
    let error = |e: csv::Error| format!("cannot write {}: {e}", path.display());
    let mut csv = csv::Writer::from_path(path).map_err(error)?;
    csv.write_record(["step", "epoch_utc", "phase", "range_m", "rel_speed_mps"])
        .map_err(error)?;
    for row in rows {
        csv.write_record([
            row.step.to_string(),
            row.epoch_utc.to_string(),
            row.phase.to_string(),
            row.range_m.to_string(),
            row.rel_speed_mps.to_string(),
        ])
        .map_err(error)?;
    }
    csv.flush()
        .map_err(|e| format!("cannot write {}: {e}", path.display()))
}

/// Writes the app's capture CSV: header `seconds_since_start_s,range_m,rel_speed_mps`,
/// one row per pair of `(seconds since the sim start, value)` samples of the range
/// (m) and of the relative speed (m/s), as many rows as the shorter has.
pub fn write_capture_csv(
    path: &Path,
    range_m: &[(f64, f64)],
    rel_speed_mps: &[(f64, f64)],
) -> Result<(), String> {
    let error = |e: csv::Error| format!("cannot write {}: {e}", path.display());
    let mut csv = csv::Writer::from_path(path).map_err(error)?;
    csv.write_record(["seconds_since_start_s", "range_m", "rel_speed_mps"])
        .map_err(error)?;
    for (range, speed) in range_m.iter().zip(rel_speed_mps) {
        csv.serialize((range.0, range.1, speed.1)).map_err(error)?;
    }
    csv.flush()
        .map_err(|e| format!("cannot write {}: {e}", path.display()))
}

/// The CCSDS OEM text (KVN, version 2.0, design D7.1) of `objects`, one segment
/// each in order: `CENTER_NAME = EARTH`, `REF_FRAME = GCRF` (brahe's ECI),
/// `TIME_SYSTEM = UTC`. `creation` is the `CREATION_DATE` (the run's start epoch,
/// so the same run gives the same bytes). brahe writes km with 6 decimals (1 mm)
/// and km/s with 9 (1 µm/s). An object without states is an error.
pub fn oem_text(creation: Epoch, objects: &[OemObject]) -> Result<String, String> {
    let mut oem = OEM::new(OEM_ORIGINATOR.to_string());
    oem.header.format_version = 2.0;
    oem.header.creation_date = creation;
    for object in objects {
        let (Some(first), Some(last)) = (object.states.first(), object.states.last()) else {
            return Err(format!("no ephemeris samples for {}", object.name));
        };
        let mut segment = OEMSegment::new(OEMMetadata::new(
            object.name.to_string(),
            object.id.unwrap_or(object.name).to_string(),
            "EARTH".to_string(),
            CCSDSRefFrame::GCRF,
            CCSDSTimeSystem::UTC,
            first.0,
            last.0,
        ));
        for (epoch, state) in &object.states {
            segment.push_state(OEMStateVector::new(
                *epoch,
                [state[0], state[1], state[2]],
                [state[3], state[4], state[5]],
            ));
        }
        oem.push_segment(segment);
    }
    oem.to_string(CCSDSFormat::KVN).map_err(|e| e.to_string())
}

/// Writes [`oem_text`] to `path`.
pub fn write_oem(path: &Path, creation: Epoch, objects: &[OemObject]) -> Result<(), String> {
    let text = oem_text(creation, objects)?;
    fs::write(path, text).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

/// Writes `entries` as JSON lines to `<project_dir>/logs/<plan_id>_<stamp>.jsonl`
/// and returns the file name: `plan_id` sanitised by [`sanitize_file_stem`], every
/// other character than a letter, a digit, `-` and `_` in `stamp` replaced by `-`
/// (as before P6). `logs/` is created on demand.
pub fn write_log_jsonl<'a>(
    project_dir: &Path,
    plan_id: &str,
    stamp: &str,
    entries: impl IntoIterator<Item = &'a LogEntry>,
) -> Result<String, String> {
    let logs_dir = project_dir.join("logs");
    fs::create_dir_all(&logs_dir).map_err(|e| format!("could not create logs directory: {e}"))?;
    let filename = format!(
        "{}_{}.jsonl",
        sanitize_file_stem(plan_id),
        replace_unsafe(stamp, '-')
    );
    let lines = entries
        .into_iter()
        .map(serde_json::to_string)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("serialization error: {e}"))?;
    fs::write(logs_dir.join(&filename), lines.join("\n"))
        .map_err(|e| format!("write error: {e}"))?;
    Ok(filename)
}
