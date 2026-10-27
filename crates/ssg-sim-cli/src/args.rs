//! Hand-rolled argument parsing for `ssg-sim run` (no argument-parser dependency).

use std::path::PathBuf;

use brahe::{Epoch, TimeSystem};

pub const USAGE: &str = "usage: ssg-sim run <project-dir> [options]

options:
  --plan ID            capture plan id (default: the only plan in <project-dir>, else example_plan)
  --target NORAD       NORAD id of the capture target (required)
  --chaser NORAD       NORAD id of the chaser (required)
  --dataset FILE       GP JSON dataset to search for the ids (repeatable; searched after
                       <project-dir>/datasets/*.json and <assets>/datasets/*.json)
  --assets DIR         assets directory (default ./assets)
  --epoch now|ISO      start epoch: `now` (default, the app path) or YYYY-MM-DDTHH:MM:SS[.fff]
  --steps N            number of updates to run (default 200000)
  --warp N             time-warp multiplier (default 1; at most 64 fixed steps run per update)
  --sample-every N     trajectory sample interval in updates (default 64)
  --out DIR            output directory (default <project-dir>/runs/<utc-timestamp>)
  --stop-at-capture    stop as soon as the state is `capture`";

pub const DEFAULT_PLAN: &str = "example_plan";

#[derive(Debug, Clone, Copy)]
pub enum EpochArg {
    /// The wall clock, as in the app (F-06).
    Now,
    At(Epoch),
}

#[derive(Debug)]
pub struct Args {
    pub project_dir: PathBuf,
    pub plan: Option<String>,
    pub target: u64,
    pub chaser: u64,
    pub datasets: Vec<PathBuf>,
    pub assets: PathBuf,
    pub epoch: EpochArg,
    pub steps: u64,
    pub warp: u32,
    pub sample_every: u64,
    pub out: Option<PathBuf>,
    pub stop_at_capture: bool,
}

/// Parse the arguments after the program name. `Err` carries the message to
/// print above the usage text (empty for a bare invocation); the caller exits 2.
pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Args, String> {
    let mut args = args.into_iter();
    match args.next().as_deref() {
        Some("run") => {}
        Some(other) => return Err(format!("unknown command `{other}`")),
        None => return Err(String::new()),
    }
    let mut parsed = Args {
        project_dir: PathBuf::new(),
        plan: None,
        target: 0,
        chaser: 0,
        datasets: Vec::new(),
        assets: PathBuf::from("assets"),
        epoch: EpochArg::Now,
        steps: 200_000,
        warp: 1,
        sample_every: 64,
        out: None,
        stop_at_capture: false,
    };
    let mut project_dir = None;
    let (mut target, mut chaser) = (None, None);
    while let Some(arg) = args.next() {
        let mut value = |name: &str| args.next().ok_or_else(|| format!("{name} needs a value"));
        match arg.as_str() {
            "--plan" => parsed.plan = Some(value("--plan")?),
            "--target" => target = Some(number("--target", &value("--target")?)?),
            "--chaser" => chaser = Some(number("--chaser", &value("--chaser")?)?),
            "--dataset" => parsed.datasets.push(PathBuf::from(value("--dataset")?)),
            "--assets" => parsed.assets = PathBuf::from(value("--assets")?),
            "--epoch" => parsed.epoch = parse_epoch(&value("--epoch")?)?,
            "--steps" => parsed.steps = number("--steps", &value("--steps")?)?,
            "--warp" => parsed.warp = number("--warp", &value("--warp")?)?,
            "--sample-every" => {
                parsed.sample_every = number("--sample-every", &value("--sample-every")?)?
            }
            "--out" => parsed.out = Some(PathBuf::from(value("--out")?)),
            "--stop-at-capture" => parsed.stop_at_capture = true,
            flag if flag.starts_with("--") => return Err(format!("unknown option `{flag}`")),
            dir if project_dir.is_none() => project_dir = Some(PathBuf::from(dir)),
            extra => return Err(format!("unexpected argument `{extra}`")),
        }
    }
    parsed.project_dir = project_dir.ok_or("missing <project-dir>")?;
    parsed.target = target.ok_or("missing --target")?;
    parsed.chaser = chaser.ok_or("missing --chaser")?;
    if parsed.sample_every == 0 {
        return Err("--sample-every must be at least 1".to_string());
    }
    if parsed.warp == 0 {
        return Err("--warp must be at least 1".to_string());
    }
    Ok(parsed)
}

fn number<T: std::str::FromStr>(name: &str, raw: &str) -> Result<T, String> {
    raw.parse()
        .map_err(|_| format!("{name}: `{raw}` is not a valid number"))
}

/// `now`, or `YYYY-MM-DDTHH:MM:SS[.fff]` read as UTC. The seconds field is parsed
/// as one number, so `47.002464` is the same `f64` as the literal in the harness.
fn parse_epoch(raw: &str) -> Result<EpochArg, String> {
    if raw == "now" {
        return Ok(EpochArg::Now);
    }
    let bad = || format!("--epoch: `{raw}` is not `now` or YYYY-MM-DDTHH:MM:SS[.fff]");
    let (date, time) = raw.split_once('T').ok_or_else(bad)?;
    let mut date = date.split('-');
    let mut time = time.splitn(3, ':');
    let field = |part: Option<&str>| part.and_then(|p| p.parse::<u32>().ok()).ok_or_else(bad);
    let (year, month, day) = (
        field(date.next())?,
        field(date.next())?,
        field(date.next())?,
    );
    let (hour, minute) = (field(time.next())?, field(time.next())?);
    let seconds: f64 = time.next().and_then(|s| s.parse().ok()).ok_or_else(bad)?;
    if date.next().is_some() || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return Err(bad());
    }
    if hour > 23 || minute > 59 || !(0.0..61.0).contains(&seconds) {
        return Err(bad());
    }
    Ok(EpochArg::At(Epoch::from_datetime(
        year,
        month as u8,
        day as u8,
        hour as u8,
        minute as u8,
        seconds,
        0.0,
        TimeSystem::UTC,
    )))
}
