//! Reading a recording back, the checks `ssg-sim validate` runs and the
//! recording folder listing.

use super::*;

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
