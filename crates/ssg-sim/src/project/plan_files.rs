//! Capture-plan files: reading v0 and v1 documents, the per-file report (errors and
//! warnings) and the v1 writer (design D3–D6).

use std::fs;
use std::path::Path;

use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::capture::plan::{
    CapturePlan, CapturePlanDevice, CaptureSphere, ConditionBounds, MigratedFromV0,
    PLAN_SCHEMA_VERSION, Phase, Transition, TransitionConditions, capture_plan_warnings,
    validate_capture_plan,
};
use crate::project::manifest::MANIFEST_FILE;
use crate::project::migrate_v0;

/// What reading one plan file gave: the plan when it loads (validated, id set),
/// and the errors and warnings, each `[<plan-id>] <json-pointer>: <message>`.
#[derive(Debug, Clone, Default)]
pub struct PlanFileReport {
    pub plan: Option<CapturePlan>,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

/// Turns a plan document into the v1 model without validating it: a v0 document
/// (no `schema_version`) is migrated, `1` is read as v1, any other version is
/// refused. Returns the plan, the migration warnings and the errors v0
/// validation would have reported; `Err` when no plan can be built.
pub(crate) fn plan_from_value(
    plan_id: &str,
    value: &Value,
) -> Result<(CapturePlan, Vec<String>, Vec<String>), Vec<String>> {
    match value.get("schema_version") {
        None => migrate_v0::migrate(plan_id, value).map(|m| (m.plan, m.warnings, m.errors)),
        Some(v) if v.as_u64() == Some(u64::from(PLAN_SCHEMA_VERSION)) => {
            CapturePlan::deserialize(value)
                .map(|plan| (plan, Vec::new(), Vec::new()))
                .map_err(|e| vec![format!("[{plan_id}] {}: {e}", v1_error_pointer(value))])
        }
        Some(v)
            if v.as_u64()
                .is_some_and(|n| n > u64::from(PLAN_SCHEMA_VERSION)) =>
        {
            Err(vec![format!(
                "[{plan_id}] /schema_version: {v} is not supported; this version reads \
                 schema_version {PLAN_SCHEMA_VERSION} (the file was written by a newer version)"
            )])
        }
        Some(v) => Err(vec![format!(
            "[{plan_id}] /schema_version: {v} is not supported; this version reads \
             schema_version {PLAN_SCHEMA_VERSION}"
        )]),
    }
}

/// The JSON Pointer of the innermost object of a v1 document that does not
/// deserialize (serde's message does not say where), `/` for the document itself.
fn v1_error_pointer(value: &Value) -> String {
    fn fails<T: DeserializeOwned>(value: Option<&Value>) -> bool {
        value.is_some_and(|v| !v.is_null() && T::deserialize(v).is_err())
    }
    for key in ["device", "capture_sphere", "migrated_from_v0"] {
        let bad = match key {
            "device" => fails::<CapturePlanDevice>(value.get(key)),
            "capture_sphere" => fails::<CaptureSphere>(value.get(key)),
            _ => fails::<MigratedFromV0>(value.get(key)),
        };
        if bad {
            return format!("/{key}");
        }
    }
    let phases = value.get("phases").and_then(Value::as_array);
    for (i, phase) in phases.into_iter().flatten().enumerate() {
        if !fails::<Phase>(Some(phase)) {
            continue;
        }
        let transitions = phase.get("transitions").and_then(Value::as_array);
        for (j, transition) in transitions.into_iter().flatten().enumerate() {
            if !fails::<Transition>(Some(transition)) {
                continue;
            }
            let at = format!("/phases/{i}/transitions/{j}");
            let all_of = transition.get("all_of");
            if !fails::<TransitionConditions>(all_of) {
                return at;
            }
            for key in ["distance_m", "relative_speed_mps"] {
                if fails::<ConditionBounds>(all_of.and_then(|c| c.get(key))) {
                    return format!("{at}/all_of/{key}");
                }
            }
            return format!("{at}/all_of");
        }
        return format!("/phases/{i}");
    }
    "/".to_string()
}

/// Reads the plan document `value` of plan `plan_id` (v0 or v1), validates it and
/// reports. `plan` is `Some` (with `id` set) only when there are no errors.
pub fn read_plan_value(plan_id: &str, value: &Value) -> PlanFileReport {
    match plan_from_value(plan_id, value) {
        Err(errors) => PlanFileReport {
            plan: None,
            errors,
            warnings: Vec::new(),
        },
        Ok((mut plan, mut warnings, mut errors)) => {
            if errors.is_empty() {
                errors = validate_capture_plan(plan_id, &plan);
            }
            warnings.extend(capture_plan_warnings(plan_id, &plan));
            plan.id = plan_id.to_string();
            PlanFileReport {
                plan: errors.is_empty().then_some(plan),
                errors,
                warnings,
            }
        }
    }
}

/// One report per `.json` file in `dir` (except `project.json`), keyed by file
/// stem, in file-name order. Empty if the directory does not exist or cannot be
/// read.
pub fn plan_file_reports(dir: &Path) -> Vec<(String, PlanFileReport)> {
    let Ok(read_dir) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths: Vec<_> = read_dir
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .filter(|path| path.file_name().is_some_and(|name| name != MANIFEST_FILE))
        .collect();
    paths.sort();
    let mut reports = Vec::new();
    for path in paths {
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let failed = |message: String| PlanFileReport {
            errors: vec![message],
            ..PlanFileReport::default()
        };
        let report = match fs::read_to_string(&path) {
            Err(e) => failed(format!("Could not read file: {e}")),
            Ok(raw) => match serde_json::from_str::<Value>(&raw) {
                Err(e) => failed(format!("Invalid JSON: {e}")),
                Ok(value) => read_plan_value(stem, &value),
            },
        };
        reports.push((stem.to_string(), report));
    }
    reports
}

/// The plan as a v1 file: pretty JSON, `schema_version` first, only the optional
/// keys the plan has.
pub fn plan_to_json(plan: &CapturePlan) -> String {
    serde_json::to_string_pretty(plan).expect("a capture plan always serializes")
}

/// Writes `plan` as a v1 file to `dir/filename`.
pub fn save_plan(dir: &Path, filename: &str, plan: &CapturePlan) -> std::io::Result<()> {
    fs::write(dir.join(filename), plan_to_json(plan))
}
