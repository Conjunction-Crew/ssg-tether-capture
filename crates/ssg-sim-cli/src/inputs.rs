//! Reading the inputs: the project manifest, the capture plans and the GP records
//! of the two bodies.

use std::fs;
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use brahe::Epoch;
use ssg_sim::capture::plan::CapturePlanLibrary;
use ssg_sim::orbit::catalog::{JsonOrbital, JsonOrbitalData, gp_record_to_elements};
use ssg_sim::orbit::selection::{
    EditableOrbitalElements, OrbitalSelectionSource, SelectedOrbitalObject,
};
use ssg_sim::project::manifest::read_manifest_file;
use ssg_sim::project::plan_files::plan_file_reports;

use crate::args::{Args, DEFAULT_PLAN, EpochArg};

/// What a run starts from: the flags laid over `<project-dir>/project.json`.
pub struct Inputs {
    pub start_epoch: Epoch,
    /// `now` (the wall clock, resolved once) or `fixed`.
    pub epoch_mode: &'static str,
    pub target: SelectedOrbitalObject,
    pub chaser: SelectedOrbitalObject,
    /// The NORAD ids, when the bodies have them (an elements body has none).
    pub target_id: Option<u64>,
    pub chaser_id: Option<u64>,
    /// `--plan`, else the manifest's active plan; `None` picks by the default rules.
    pub plan: Option<String>,
}

/// The inputs of `args`: `project.json` when the project has one (every flag given
/// overrides its entry and says so on stderr; so does each of its warnings, as
/// `validate` words them), the flags and the datasets otherwise.
pub fn resolve(args: &Args) -> Result<Inputs, String> {
    let manifest = match read_manifest_file(&args.project_dir) {
        None => None,
        Some(report) => {
            for line in &report.warnings {
                eprintln!("ssg-sim: warning: {line}");
            }
            match report.manifest {
                Some(manifest) => Some(manifest),
                None => return Err(report.errors.join("; ")),
            }
        }
    };
    let from_manifest = manifest
        .as_ref()
        .map(|manifest| manifest.sim_inputs())
        .transpose()
        .map_err(|errors| errors.join("; "))?;
    if let Some(manifest) = &manifest {
        let overrides = [
            (args.target.is_some(), "--target"),
            (args.chaser.is_some(), "--chaser"),
            (args.epoch.is_some(), "--epoch"),
            (
                args.plan.is_some() && manifest.active_plan.is_some(),
                "--plan",
            ),
        ];
        for (given, flag) in overrides {
            if given {
                eprintln!("ssg-sim: {flag} overrides project.json");
            }
        }
    }

    let (start_epoch, epoch_mode) = match (args.epoch, &from_manifest) {
        (Some(EpochArg::At(epoch)), _) => (epoch, "fixed"),
        (None, Some(inputs)) => (inputs.start_epoch, "fixed"),
        (Some(EpochArg::Now), _) | (None, None) => (Epoch::now(), "now"),
    };

    let (target_record, chaser_record) = find_records(args, args.target, args.chaser)?;
    let manifest_body =
        |pick: fn(&ssg_sim::project::manifest::ManifestSimInputs) -> &SelectedOrbitalObject| {
            from_manifest
                .as_ref()
                .map(|inputs| pick(inputs).clone())
                .ok_or_else(|| "no project.json and no NORAD id".to_string())
        };
    let target = match (&target_record, args.target) {
        (Some(record), Some(id)) => pick(record, id, start_epoch)?,
        _ => manifest_body(|inputs| &inputs.target)?,
    };
    let chaser = match (&chaser_record, args.chaser) {
        (Some(record), Some(id)) => pick(record, id, start_epoch)?,
        _ => manifest_body(|inputs| &inputs.chaser)?,
    };
    let id_of = |flag: Option<u64>, body: &SelectedOrbitalObject| {
        flag.or_else(|| body.gp.as_ref().map(|gp| u64::from(gp.norad_id)))
    };
    Ok(Inputs {
        start_epoch,
        epoch_mode,
        target_id: id_of(args.target, &target),
        chaser_id: id_of(args.chaser, &chaser),
        target,
        chaser,
        plan: args
            .plan
            .clone()
            .or_else(|| from_manifest.and_then(|inputs| inputs.active_plan)),
    })
}

/// A catalog pick from a GP record, as the app makes it (F-27): SGP4 at the start
/// epoch.
fn pick(
    record: &JsonOrbital,
    id: u64,
    reference_epoch: Epoch,
) -> Result<SelectedOrbitalObject, String> {
    let (_, gp, _) = gp_record_to_elements(record, reference_epoch)
        .ok_or_else(|| format!("NORAD {id}: the record is missing an orbital element"))?;
    Ok(SelectedOrbitalObject {
        source: OrbitalSelectionSource::Custom {
            label: record
                .object_name
                .clone()
                .unwrap_or_else(|| format!("NORAD {id}")),
        },
        elements: EditableOrbitalElements::from_gp(&gp),
        gp: Some(gp),
        mass_kg: None,
    })
}

/// Load the example plans and the project's plans, pick the plan id: `requested`
/// when given, else the project's only plan, else the example. The warnings of the
/// project's plan file that runs go to stderr, as `validate` words them.
pub fn load_plans(app: &mut App, args: &Args, requested: Option<&str>) -> Result<String, String> {
    let mut library = app.world_mut().resource_mut::<CapturePlanLibrary>();
    let errors = library.load_all(
        &args.assets.join("example_capture_plans"),
        &args.project_dir,
    );
    for (plan, messages) in &errors {
        eprintln!("ssg-sim: plan `{plan}` not loaded: {}", messages.join("; "));
    }
    let plan = match requested {
        Some(plan) => plan.to_string(),
        None if library.user_plans.len() == 1 => library.user_plans.keys().next().cloned().unwrap(),
        None => DEFAULT_PLAN.to_string(),
    };
    if library.plans.contains_key(&plan) {
        for (stem, report) in plan_file_reports(&args.project_dir) {
            if stem == plan && report.plan.is_some() {
                for line in &report.warnings {
                    eprintln!("ssg-sim: warning: {line}");
                }
            }
        }
        Ok(plan)
    } else {
        Err(format!("capture plan `{plan}` not found"))
    }
}

/// The first record for each wanted NORAD id, searching `<project>/datasets/*.json`,
/// `<assets>/datasets/*.json`, then the `--dataset` files. An id that is `None` is
/// not searched for.
pub fn find_records(
    args: &Args,
    target_id: Option<u64>,
    chaser_id: Option<u64>,
) -> Result<(Option<JsonOrbital>, Option<JsonOrbital>), String> {
    let mut files = json_files(&args.project_dir.join("datasets"));
    files.extend(json_files(&args.assets.join("datasets")));
    let explicit = args.datasets.len();
    files.extend(args.datasets.iter().cloned());
    let (mut target, mut chaser) = (None, None);
    let found =
        |wanted: Option<u64>, record: &Option<JsonOrbital>| wanted.is_none() || record.is_some();
    for (index, path) in files.iter().enumerate() {
        if found(target_id, &target) && found(chaser_id, &chaser) {
            break;
        }
        let from_flag = index >= files.len() - explicit;
        let data = match fs::read_to_string(path)
            .map_err(|e| e.to_string())
            .and_then(|raw| {
                serde_json::from_str::<JsonOrbitalData>(&raw).map_err(|e| e.to_string())
            }) {
            Ok(data) => data,
            Err(e) if from_flag => return Err(format!("dataset {}: {e}", path.display())),
            Err(e) => {
                eprintln!("ssg-sim: skipping dataset {}: {e}", path.display());
                continue;
            }
        };
        for record in data.data {
            let id = record.norad_cat_id.as_ref().and_then(|id| id.as_u64());
            if target_id.is_some() && target.is_none() && id == target_id {
                target = Some(record);
            } else if chaser_id.is_some() && chaser.is_none() && id == chaser_id {
                chaser = Some(record);
            }
        }
    }
    if let (Some(id), None) = (target_id, &target) {
        return Err(format!("NORAD {id} not found in any dataset"));
    }
    if let (Some(id), None) = (chaser_id, &chaser) {
        return Err(format!("NORAD {id} not found in any dataset"));
    }
    Ok((target, chaser))
}

fn json_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    files.sort();
    files
}
