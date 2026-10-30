//! `ssg-sim validate <project-dir> [--assets DIR]` (design D9): reads `project.json`
//! and every plan file of the folder without running anything, and reports what it
//! finds as `error:`, `warning:` and `info:` lines.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use ssg_sim::capture::plan::CapturePlan;
use ssg_sim::project::manifest::{MANIFEST_FILE, read_manifest_file};
use ssg_sim::project::plan_files::plan_file_reports;
use ssg_sim::project::recording::RECORDING_PARTIAL_SUFFIX;
use ssg_sim::recording::files::{check_recording, list_recording_dirs};

use crate::args::{DEFAULT_PLAN, ValidateArgs};

/// The lines to print and how many of them are errors and warnings.
pub struct Report {
    pub lines: Vec<String>,
    pub errors: usize,
    pub warnings: usize,
}

impl Report {
    fn new() -> Self {
        Self {
            lines: Vec::new(),
            errors: 0,
            warnings: 0,
        }
    }

    fn error(&mut self, message: impl std::fmt::Display) {
        self.errors += 1;
        self.lines.push(format!("error: {message}"));
    }

    fn warning(&mut self, message: impl std::fmt::Display) {
        self.warnings += 1;
        self.lines.push(format!("warning: {message}"));
    }

    fn info(&mut self, message: impl std::fmt::Display) {
        self.lines.push(format!("info: {message}"));
    }
}

/// A plan file's message with its file named: the typed messages already start
/// with `[<plan-id>]`, the read errors do not.
fn plan_message(stem: &str, message: &str) -> String {
    if message.starts_with('[') {
        message.to_string()
    } else {
        format!("{stem}.json: {message}")
    }
}

/// Validate the project folder. `Err` when `args.project_dir` is not a directory
/// (a usage error, exit 2).
pub fn validate(args: &ValidateArgs) -> Result<Report, String> {
    let dir: &Path = &args.project_dir;
    if !dir.is_dir() {
        return Err(format!("{} is not a directory", dir.display()));
    }
    let mut report = Report::new();

    let manifest = read_manifest_file(dir);
    let mut active_plan = None;
    let mut recording_block = None;
    match &manifest {
        None => report.info(format!(
            "no {MANIFEST_FILE}: the folder is read as a v0 project (plans only)"
        )),
        Some(read) => {
            for message in &read.errors {
                report.error(message);
            }
            for message in &read.warnings {
                report.warning(message);
            }
            for message in &read.notes {
                report.info(message);
            }
            if let Some(manifest) = &read.manifest {
                active_plan = manifest.active_plan.clone();
                recording_block = manifest.recording.clone();
            }
        }
    }

    let mut plan_ids = BTreeSet::new();
    let mut plans: BTreeMap<String, CapturePlan> = BTreeMap::new();
    let mut project_plans = 0;
    for (stem, plan_report) in plan_file_reports(dir) {
        project_plans += 1;
        for message in &plan_report.errors {
            report.error(plan_message(&stem, message));
        }
        for message in &plan_report.warnings {
            report.warning(plan_message(&stem, message));
        }
        if let Some(plan) = plan_report.plan {
            plan_ids.insert(stem.clone());
            plans.insert(stem, plan);
        }
    }
    let only_plan = (project_plans == 1 && plans.len() == 1)
        .then(|| plans.keys().next().cloned())
        .flatten();
    if project_plans == 0 {
        report.info("no plan files in the project folder (the example plans are used)");
    }
    // The example plans only add ids; their problems are not the project's.
    for (stem, example) in plan_file_reports(&args.assets.join("example_capture_plans")) {
        if let Some(plan) = example.plan {
            plan_ids.insert(stem.clone());
            plans.entry(stem).or_insert(plan);
        }
    }

    if let Some(id) = &active_plan
        && !plan_ids.contains(id)
    {
        report.error(format!(
            "[{MANIFEST_FILE}] /active_plan: no plan `{id}` in the project or the example plans"
        ));
    }

    // `run` picks the plan like this: the active plan, else the project's only plan,
    // else the example plan.
    let running = active_plan
        .or(only_plan)
        .unwrap_or_else(|| DEFAULT_PLAN.to_string());
    if let (Some(block), Some(plan)) = (&recording_block, plans.get(&running))
        && let Some(warning) = block.auto_start_warning(plan)
    {
        report.warning(warning);
    }

    let recordings = dir.join("recordings");
    for path in list_recording_dirs(&recordings) {
        let name = path
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        let label = format!("recordings/{name}");
        if name.ends_with(RECORDING_PARTIAL_SUFFIX) {
            report.warning(format!(
                "{label}: unfinished (an interrupted run or a crash); its Parquet file has no \
                 footer and cannot be recovered: make the recording again"
            ));
            continue;
        }
        let check = check_recording(&path, &label);
        for message in &check.errors {
            report.error(message);
        }
        for message in &check.warnings {
            report.warning(message);
        }
        if let Some(summary) = &check.summary {
            report.info(summary);
        }
    }

    report.lines.push(format!(
        "{} errors, {} warnings",
        report.errors, report.warnings
    ));
    Ok(report)
}
