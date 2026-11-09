//! The file sink that writes `recordings/<name>/`.

use super::*;

// ─── The file sink ────────────────────────────────────────────────────────────

/// The name of a recording that has none of its own (an automatic start).
#[derive(Debug, Clone)]
pub enum DefaultName {
    /// Always this name (the CLI, the tests).
    Fixed(String),
    /// `<prefix>-<stamp()>`, the stamp taken when the recording starts (#127):
    /// the app passes its wall-clock stamp; the sim never reads the clock (F-06).
    Stamped {
        prefix: String,
        stamp: fn() -> String,
    },
}

impl DefaultName {
    /// The name for a recording that starts now.
    pub fn resolve(&self) -> String {
        match self {
            DefaultName::Fixed(name) => name.clone(),
            DefaultName::Stamped { prefix, stamp } => format!("{prefix}-{}", stamp()),
        }
    }
}

/// What the sink knows about the run besides the frames.
#[derive(Debug, Clone)]
pub struct RecordingContext {
    /// The folder `recordings/` of the project; created on demand.
    pub recordings_dir: PathBuf,
    /// The name of a recording that has none of its own (an automatic start).
    pub default_name: DefaultName,
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
        let stem = sanitize_file_stem(
            &header
                .name
                .clone()
                .unwrap_or_else(|| context.default_name.resolve()),
        );
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
