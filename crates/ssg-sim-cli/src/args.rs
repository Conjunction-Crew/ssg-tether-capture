//! Hand-rolled argument parsing for `ssg-sim run`, `ssg-sim validate` and
//! `ssg-sim replay` (no argument-parser dependency).

use std::path::PathBuf;

use brahe::Epoch;
use ssg_sim::project::export::sanitize_file_stem;
use ssg_sim::project::manifest::MANIFEST_FILE;
use ssg_sim::project::parse_utc;
use ssg_sim::time::MAX_STEPS_PER_UPDATE;

pub const USAGE: &str = "usage: ssg-sim run <project-dir> [options]
       ssg-sim validate <project-dir> [--assets DIR]
       ssg-sim replay <recording-dir> [--assets DIR] [--frames-only]

run options:
  --plan ID            capture plan id (default: project.json's active_plan, else the only
                       plan in <project-dir>, else example_plan)
  --target NORAD       NORAD id of the capture target (required without <project-dir>/project.json;
                       overrides it otherwise)
  --chaser NORAD       NORAD id of the chaser (required without project.json; overrides it)
  --dataset FILE       GP JSON dataset to search for the ids (repeatable; searched after
                       <project-dir>/datasets/*.json and <assets>/datasets/*.json)
  --assets DIR         assets directory (default ./assets)
  --epoch now|ISO      start epoch: `now` (default without project.json, the app path) or
                       YYYY-MM-DDTHH:MM:SS[.fff][Z]; overrides project.json's start_epoch_utc
  --steps N            fixed steps to run after the capture starts (default 200000;
                       a multiple of --warp)
  --warp N             time warp: fixed steps per update (default 1, at most 64)
  --sample-every N     trajectory sample interval in fixed steps (default 64; a multiple
                       of --warp)
  --out DIR            output directory (default <project-dir>/runs/<utc-timestamp>)
  --stop-at-capture    stop at the end of the update in which the state becomes `capture`
  --record NAME        record the run to <project-dir>/recordings/NAME/ (letters, digits, `-`, `_`);
                       starts with the capture command, or when the capture enters
                       project.json's recording.auto_start_phase; ends with the run
  --record-from-phase ID
                       start the recording when the capture enters phase ID (overrides
                       recording.auto_start_phase; needs --record)

validate checks project.json, every plan file, the active plan, the orbits and every
recording under <project-dir>/recordings; it prints
`error:`, `warning:` and `info:` lines and exits 0 without errors, 1 with errors.

replay checks a recording's files and re-simulates it from step 0 with its inputs,
comparing every frame bit for bit (--frames-only: check the files only). Exit 0: every
frame identical; 1: a difference with the same target and version; 3: a difference with
another target or version (expected, F-48); 2: usage error or invalid recording.";

pub const DEFAULT_PLAN: &str = "example_plan";

#[derive(Debug, Clone, Copy)]
pub enum EpochArg {
    /// The wall clock, as in the app (F-06).
    Now,
    At(Epoch),
}

/// What the command line asks for.
#[derive(Debug)]
pub enum Command {
    Run(Box<Args>),
    Validate(ValidateArgs),
    Replay(ReplayArgs),
}

/// `ssg-sim replay <recording-dir> [--assets DIR] [--frames-only]`.
#[derive(Debug)]
pub struct ReplayArgs {
    pub recording_dir: PathBuf,
    pub assets: PathBuf,
    /// Check the files without re-simulating.
    pub frames_only: bool,
}

/// `ssg-sim validate <project-dir> [--assets DIR]`.
#[derive(Debug)]
pub struct ValidateArgs {
    pub project_dir: PathBuf,
    pub assets: PathBuf,
}

#[derive(Debug)]
pub struct Args {
    pub project_dir: PathBuf,
    /// `--plan`; `None`: `project.json`'s active plan, else the default rules.
    pub plan: Option<String>,
    /// `--target`; `None` reads the target from `project.json`.
    pub target: Option<u64>,
    /// `--chaser`; `None` reads the chaser from `project.json`.
    pub chaser: Option<u64>,
    pub datasets: Vec<PathBuf>,
    pub assets: PathBuf,
    /// `--epoch`; `None`: `project.json`'s start epoch, else `now`.
    pub epoch: Option<EpochArg>,
    /// Fixed steps to run after the capture starts; a multiple of `warp`.
    pub steps: u64,
    /// Fixed steps per update, at most [`MAX_STEPS_PER_UPDATE`].
    pub warp: u32,
    /// Trajectory sample interval in fixed steps; a multiple of `warp`.
    pub sample_every: u64,
    pub out: Option<PathBuf>,
    pub stop_at_capture: bool,
    /// `--record`: the name of the recording folder under `<project-dir>/recordings`.
    pub record: Option<String>,
    /// `--record-from-phase`: overrides the project's `recording.auto_start_phase`.
    pub record_from_phase: Option<String>,
}

/// Parse the arguments after the program name. `Err` carries the message to
/// print above the usage text (empty for a bare invocation); the caller exits 2.
pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Command, String> {
    let mut args = args.into_iter();
    match args.next().as_deref() {
        Some("run") => parse_run(args).map(|run| Command::Run(Box::new(run))),
        Some("validate") => parse_validate(args).map(Command::Validate),
        Some("replay") => parse_replay(args).map(Command::Replay),
        Some(other) => Err(format!("unknown command `{other}`")),
        None => Err(String::new()),
    }
}

fn parse_validate(args: impl Iterator<Item = String>) -> Result<ValidateArgs, String> {
    let mut args = args;
    let mut assets = PathBuf::from("assets");
    let mut project_dir = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--assets" => {
                assets = PathBuf::from(args.next().ok_or("--assets needs a value")?);
            }
            flag if flag.starts_with("--") => return Err(format!("unknown option `{flag}`")),
            dir if project_dir.is_none() => project_dir = Some(PathBuf::from(dir)),
            extra => return Err(format!("unexpected argument `{extra}`")),
        }
    }
    Ok(ValidateArgs {
        project_dir: project_dir.ok_or("missing <project-dir>")?,
        assets,
    })
}

fn parse_replay(args: impl Iterator<Item = String>) -> Result<ReplayArgs, String> {
    let mut args = args;
    let mut assets = PathBuf::from("assets");
    let mut frames_only = false;
    let mut recording_dir = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--assets" => {
                assets = PathBuf::from(args.next().ok_or("--assets needs a value")?);
            }
            "--frames-only" => frames_only = true,
            flag if flag.starts_with("--") => return Err(format!("unknown option `{flag}`")),
            dir if recording_dir.is_none() => recording_dir = Some(PathBuf::from(dir)),
            extra => return Err(format!("unexpected argument `{extra}`")),
        }
    }
    Ok(ReplayArgs {
        recording_dir: recording_dir.ok_or("missing <recording-dir>")?,
        assets,
        frames_only,
    })
}

fn parse_run(args: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut args = args;
    let mut parsed = Args {
        project_dir: PathBuf::new(),
        plan: None,
        target: None,
        chaser: None,
        datasets: Vec::new(),
        assets: PathBuf::from("assets"),
        epoch: None,
        steps: 200_000,
        warp: 1,
        sample_every: 64,
        out: None,
        stop_at_capture: false,
        record: None,
        record_from_phase: None,
    };
    let mut project_dir = None;
    while let Some(arg) = args.next() {
        let mut value = |name: &str| args.next().ok_or_else(|| format!("{name} needs a value"));
        match arg.as_str() {
            "--plan" => parsed.plan = Some(value("--plan")?),
            "--target" => parsed.target = Some(number("--target", &value("--target")?)?),
            "--chaser" => parsed.chaser = Some(number("--chaser", &value("--chaser")?)?),
            "--dataset" => parsed.datasets.push(PathBuf::from(value("--dataset")?)),
            "--assets" => parsed.assets = PathBuf::from(value("--assets")?),
            "--epoch" => parsed.epoch = Some(parse_epoch(&value("--epoch")?)?),
            "--steps" => parsed.steps = number("--steps", &value("--steps")?)?,
            "--warp" => parsed.warp = number("--warp", &value("--warp")?)?,
            "--sample-every" => {
                parsed.sample_every = number("--sample-every", &value("--sample-every")?)?
            }
            "--out" => parsed.out = Some(PathBuf::from(value("--out")?)),
            "--stop-at-capture" => parsed.stop_at_capture = true,
            "--record" => parsed.record = Some(value("--record")?),
            "--record-from-phase" => parsed.record_from_phase = Some(value("--record-from-phase")?),
            flag if flag.starts_with("--") => return Err(format!("unknown option `{flag}`")),
            dir if project_dir.is_none() => project_dir = Some(PathBuf::from(dir)),
            extra => return Err(format!("unexpected argument `{extra}`")),
        }
    }
    parsed.project_dir = project_dir.ok_or("missing <project-dir>")?;
    // With a `project.json` the bodies come from it; the flags override it.
    if !parsed.project_dir.join(MANIFEST_FILE).is_file() {
        if parsed.target.is_none() {
            return Err("missing --target".to_string());
        }
        if parsed.chaser.is_none() {
            return Err("missing --chaser".to_string());
        }
    }
    if let Some(name) = &parsed.record
        && (name.is_empty() || sanitize_file_stem(name) != *name)
    {
        return Err(format!(
            "--record: `{name}` is not a name (use letters, digits, `-` and `_`)"
        ));
    }
    if parsed.record_from_phase.is_some() && parsed.record.is_none() {
        return Err("--record-from-phase needs --record".to_string());
    }
    if parsed.sample_every == 0 {
        return Err("--sample-every must be at least 1".to_string());
    }
    if parsed.warp == 0 {
        return Err("--warp must be at least 1".to_string());
    }
    // The sim runs at most `MAX_STEPS_PER_UPDATE` fixed steps per update, so a
    // larger warp would run fewer steps than `--steps` asks for (operator
    // decision, P4 Run 5).
    if parsed.warp > MAX_STEPS_PER_UPDATE {
        return Err(format!(
            "--warp must be at most {MAX_STEPS_PER_UPDATE} (fixed steps per update)"
        ));
    }
    // Runs at different warps then sample the same fixed steps (A-12).
    if !parsed.steps.is_multiple_of(u64::from(parsed.warp)) {
        return Err("--steps must be a multiple of --warp".to_string());
    }
    if !parsed.sample_every.is_multiple_of(u64::from(parsed.warp)) {
        return Err("--sample-every must be a multiple of --warp".to_string());
    }
    Ok(parsed)
}

fn number<T: std::str::FromStr>(name: &str, raw: &str) -> Result<T, String> {
    raw.parse()
        .map_err(|_| format!("{name}: `{raw}` is not a valid number"))
}

/// `now`, or `YYYY-MM-DDTHH:MM:SS[.fff]` read as UTC (see `ssg_sim::project::parse_utc`).
fn parse_epoch(raw: &str) -> Result<EpochArg, String> {
    if raw == "now" {
        return Ok(EpochArg::Now);
    }
    parse_utc(raw)
        .map(EpochArg::At)
        .ok_or_else(|| format!("--epoch: `{raw}` is not `now` or YYYY-MM-DDTHH:MM:SS[.fff]"))
}
