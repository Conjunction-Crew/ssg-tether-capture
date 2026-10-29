//! `ssg-sim validate <project-dir> [--assets DIR]` (design D9): reads `project.json`
//! and every plan file of the folder without running anything, and reports what it
//! finds as `error:`, `warning:` and `info:` lines.

use std::collections::BTreeSet;
use std::path::Path;

use ssg_sim::project::manifest::{MANIFEST_FILE, read_manifest_file};
use ssg_sim::project::plan_files::plan_file_reports;

use crate::args::ValidateArgs;

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
            }
        }
    }

    let mut plan_ids = BTreeSet::new();
    let mut project_plans = 0;
    for (stem, plan_report) in plan_file_reports(dir) {
        project_plans += 1;
        for message in &plan_report.errors {
            report.error(plan_message(&stem, message));
        }
        for message in &plan_report.warnings {
            report.warning(plan_message(&stem, message));
        }
        if plan_report.plan.is_some() {
            plan_ids.insert(stem);
        }
    }
    if project_plans == 0 {
        report.info("no plan files in the project folder (the example plans are used)");
    }
    // The example plans only add ids; their problems are not the project's.
    for (stem, example) in plan_file_reports(&args.assets.join("example_capture_plans")) {
        if example.plan.is_some() {
            plan_ids.insert(stem);
        }
    }

    if let Some(id) = active_plan
        && !plan_ids.contains(&id)
    {
        report.error(format!(
            "[{MANIFEST_FILE}] /active_plan: no plan `{id}` in the project or the example plans"
        ));
    }

    report.lines.push(format!(
        "{} errors, {} warnings",
        report.errors, report.warnings
    ));
    Ok(report)
}
