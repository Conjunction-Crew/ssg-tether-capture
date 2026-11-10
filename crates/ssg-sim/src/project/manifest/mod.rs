//! The project manifest, `project.json` (schema v1, design D2): the file model, the
//! reader and validator, the writer and the conversion to and from the sim inputs.
//!
//! One function turns a manifest into sim inputs ([`ProjectManifest::sim_inputs`]);
//! the app and the CLI both use it, so the same files give the same inputs.

use std::fs;
use std::path::Path;

use brahe::Epoch;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::orbit::catalog::{GpElements, JsonOrbital, gp_record_to_elements};
use crate::orbit::selection::{
    EditableOrbitalElements, OrbitalSelectionSource, OrbitalSelectionState, SelectedOrbitalObject,
    relative_chaser_elements,
};
use crate::project::epoch::{format_utc, parse_utc};

mod validate;

/// The manifest's file name in the project folder; the plan loader skips it.
pub const MANIFEST_FILE: &str = "project.json";

/// The `schema_version` of the manifests this build reads and writes.
pub const MANIFEST_SCHEMA_VERSION: u32 = 1;

/// The avian3d features the workspace builds with (`Cargo.toml`); results are
/// bit-identical only for the same version, whatever the platform (the guard
/// `scripts/check_platform_math.sh` checks this list against the tree).
const AVIAN_FEATURES: &[&str] = &["enhanced-determinism", "parallel"];

/// The six GP fields a body needs besides `NORAD_CAT_ID`, in load order.
const GP_NUMBER_KEYS: [&str; 6] = [
    "MEAN_MOTION",
    "ECCENTRICITY",
    "INCLINATION",
    "RA_OF_ASC_NODE",
    "ARG_OF_PERICENTER",
    "MEAN_ANOMALY",
];

/// The project manifest, schema v1. Lengths are metres, angles radians, masses kg,
/// epochs UTC strings with microseconds.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub struct ProjectManifest {
    /// File format version; `1`.
    #[cfg_attr(test, schemars(range(min = 1, max = 1)))]
    pub schema_version: u32,
    /// Display name of the project. Must not be blank.
    pub name: String,
    /// The simulation start epoch, `YYYY-MM-DDTHH:MM:SS.ffffffZ` (UTC, microseconds).
    /// The simulation always starts from exactly this string.
    pub start_epoch_utc: String,
    /// Id (file stem) of the capture plan to run. Absent: the project's only plan,
    /// else `example_plan`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_plan: Option<String>,
    /// The capture target.
    pub target: ManifestBody,
    /// The chaser (the tether root's carrier).
    pub chaser: ManifestBody,
    /// The build that wrote the file; read only by `ssg-sim validate`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub built_with: Option<BuiltWith>,
    /// When and how this project records. Absent: manual recording only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recording: Option<RecordingBlock>,
}

/// The `recording` block of `project.json`: every key is optional and falls back
/// to the default named here. Durations are simulation seconds.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(deny_unknown_fields)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub struct RecordingBlock {
    /// Start a recording when the capture enters this phase (a phase id of the
    /// active plan). Absent: no automatic start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_start_phase: Option<String>,
    /// Stop the recording `stop_after_end_s` after the capture enters an end phase.
    /// Default `true`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_stop_on_end: Option<bool>,
    /// Seconds recorded after the end event; greater than 0. Default 60.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_after_end_s: Option<f64>,
    /// Steps between two frames (K); at least 1. Default 32.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, schemars(range(min = 1)))]
    pub keyframe_interval_steps: Option<u32>,
    /// The longest recording; greater than 0. Default 1800.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_duration_s: Option<f64>,
    /// The largest keyframe table in megabytes; greater than 0. Default 100.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_size_mb: Option<f64>,
    /// The most tether bodies a recording may hold; at least 1. Default 200.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, schemars(range(min = 1)))]
    pub max_tether_bodies: Option<u32>,
}

impl RecordingBlock {
    /// The warning for an `auto_start_phase` that names no phase of `plan`; `None`
    /// when there is none or it does (the plan is not known to the manifest, so
    /// this is a warning, not an error).
    pub fn auto_start_warning(&self, plan: &crate::capture::plan::CapturePlan) -> Option<String> {
        let phase = self.auto_start_phase.as_ref()?;
        if plan.phases.iter().any(|p| &p.id == phase) {
            return None;
        }
        let ids: Vec<&str> = plan.phases.iter().map(|p| p.id.as_str()).collect();
        Some(message(
            "/recording/auto_start_phase",
            format!(
                "no phase `{phase}` in plan `{}` (its phases: {}); the recording would never start",
                plan.id,
                ids.join(", ")
            ),
        ))
    }

    fn validate(&self, at: &str, errors: &mut Vec<String>) {
        if let Some(phase) = &self.auto_start_phase
            && phase.trim().is_empty()
        {
            errors.push(message(
                &format!("{at}/auto_start_phase"),
                "must not be blank",
            ));
        }
        let positive = |key: &str, value: Option<f64>, errors: &mut Vec<String>| {
            if let Some(v) = value
                && !(v.is_finite() && v > 0.0)
            {
                errors.push(message(&format!("{at}/{key}"), "must be greater than 0"));
            }
        };
        positive("stop_after_end_s", self.stop_after_end_s, errors);
        positive("max_duration_s", self.max_duration_s, errors);
        positive("max_size_mb", self.max_size_mb, errors);
        for (key, value) in [
            ("keyframe_interval_steps", self.keyframe_interval_steps),
            ("max_tether_bodies", self.max_tether_bodies),
        ] {
            if value == Some(0) {
                errors.push(message(&format!("{at}/{key}"), "must be at least 1"));
            }
        }
    }
}

/// One body of the scenario.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub struct ManifestBody {
    /// Display name. Must not be blank.
    pub label: String,
    /// Where the body starts.
    pub orbit: ManifestOrbit,
    /// Where the orbit came from. Provenance only: the orbit is the source of truth.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<ManifestSource>,
    /// Mass in kg; greater than 0. The chaser's is the tether root's (the root is
    /// the chaser); without it the root takes the plan's `end_mass_kg`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mass_kg: Option<f64>,
    /// Inertia tensor in kg m², row-major; finite, symmetric and positive definite.
    /// It is taken in the sim's ECI-aligned axes at spawn: every body starts with
    /// the identity attitude and the sim has no attitude model. Absent: the
    /// collider's inertia scaled to the mass.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inertia_kg_m2: Option<[[f64; 3]; 3]>,
}

/// How a body's initial state is given: exactly one key.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub enum ManifestOrbit {
    /// The Space-Track GP (OMM JSON) record verbatim. The body starts from SGP4, as
    /// an unedited catalog pick does.
    Gp(Box<JsonOrbital>),
    /// Osculating Keplerian elements in metres and radians.
    Elements(ManifestElements),
    /// Chaser only: an offset from the target along V-bar or R-bar, at rest in the
    /// target's LVLH frame at the start epoch. The chaser gets the osculating elements
    /// of that state.
    RelativeToTarget(RelativeToTarget),
}

/// Osculating Keplerian elements, metres and radians (degrees would not round-trip
/// bit for bit), valid at `epoch_utc`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub struct ManifestElements {
    /// Semi-major axis in metres; greater than 0.
    pub semi_major_axis_m: f64,
    /// Eccentricity; at least 0 and below 1.
    pub eccentricity: f64,
    /// Inclination in radians.
    pub inclination_rad: f64,
    /// Right ascension of the ascending node in radians.
    pub raan_rad: f64,
    /// Argument of perigee in radians.
    pub arg_perigee_rad: f64,
    /// Mean anomaly in radians.
    pub mean_anomaly_rad: f64,
    /// The epoch the six elements are valid at, `YYYY-MM-DDTHH:MM:SS.ffffffZ` (UTC).
    pub epoch_utc: String,
}

/// A chaser placed relative to the target.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub struct RelativeToTarget {
    /// The approach line.
    pub preset: RelativePreset,
    /// Signed distance from the target in metres, not 0: positive ahead of the target
    /// (V-bar) or above it, away from the Earth (R-bar); negative behind or below.
    pub offset_m: f64,
}

/// The approach line of a chaser placed relative to the target.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub enum RelativePreset {
    /// Along the velocity vector.
    VBar,
    /// Along the radial vector.
    RBar,
}

/// Where a body's orbit was taken from.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub struct ManifestSource {
    /// Path of the dataset file, relative to the project folder.
    pub dataset: String,
    /// NORAD catalog id of the record.
    pub norad_id: u64,
}

/// The build that wrote the manifest (the determinism settings: nothing here is
/// configurable at run time).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub struct BuiltWith {
    /// The application version.
    pub app_version: String,
    /// The platform, `<arch>-<os>`.
    pub target: String,
    /// The avian3d features the physics was built with.
    pub avian_features: Vec<String>,
    /// The commit the build was made from (`git rev-parse --short=12 HEAD`, with
    /// `-dirty` when tracked files had uncommitted changes); absent in a build
    /// made outside a git checkout and in files written before version 0.5.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
}

/// The commit of the running build, set by `build.rs` when it was built in a git
/// checkout.
const BUILD_COMMIT: Option<&str> = option_env!("SSG_BUILD_COMMIT");

impl BuiltWith {
    /// The running build.
    pub fn current() -> Self {
        Self {
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            target: format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS),
            avian_features: AVIAN_FEATURES.iter().map(|f| f.to_string()).collect(),
            commit: BUILD_COMMIT.map(str::to_string),
        }
    }

    /// What differs between `self` (the build that wrote a file) and `running`,
    /// one phrase each; empty when they match.
    pub fn differences(&self, running: &BuiltWith) -> Vec<String> {
        let mut differences = Vec::new();
        if self.app_version != running.app_version {
            differences.push(format!(
                "app version {} (this build is {})",
                self.app_version, running.app_version
            ));
        }
        if self.target != running.target {
            differences.push(format!(
                "platform {} (this build is {})",
                self.target, running.target
            ));
        }
        if self.avian_features != running.avian_features {
            differences.push(format!(
                "avian features [{}] (this build has [{}])",
                self.avian_features.join(", "),
                running.avian_features.join(", ")
            ));
        }
        differences
    }
}

/// What a manifest turns into: the inputs the app and the CLI start a run from.
#[derive(Debug, Clone)]
pub struct ManifestSimInputs {
    /// The sim start epoch: `parse(start_epoch_utc)`.
    pub start_epoch: Epoch,
    pub target: SelectedOrbitalObject,
    pub chaser: SelectedOrbitalObject,
    /// The manifest's active plan id, if it names one.
    pub active_plan: Option<String>,
    /// The chaser's `relative_to_target`, when it is placed that way (P9 S5).
    pub chaser_relative: Option<RelativeToTarget>,
}

/// What reading `project.json` gave: the manifest when it loads (valid), the errors
/// and the warnings and notes, each `[project.json] <json-pointer>: <message>`.
#[derive(Debug, Clone, Default)]
pub struct ManifestReport {
    pub manifest: Option<ProjectManifest>,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    /// Facts that need no action, e.g. that the file was written by another build.
    pub notes: Vec<String>,
}

fn message(pointer: &str, text: impl std::fmt::Display) -> String {
    format!("[{MANIFEST_FILE}] {pointer}: {text}")
}

/// The JSON Pointer of the innermost object of a manifest document that does not
/// deserialize (serde's message does not say where), `/` for the document itself.
fn error_pointer(value: &Value) -> String {
    fn fails<T: DeserializeOwned>(value: Option<&Value>) -> bool {
        value.is_some_and(|v| !v.is_null() && T::deserialize(v).is_err())
    }
    for key in ["target", "chaser"] {
        let Some(body) = value.get(key) else { continue };
        if !fails::<ManifestBody>(Some(body)) {
            continue;
        }
        let at = format!("/{key}");
        if fails::<ManifestSource>(body.get("source")) {
            return format!("{at}/source");
        }
        if let Some(orbit) = body.get("orbit")
            && fails::<ManifestOrbit>(Some(orbit))
        {
            return format!("{at}/orbit");
        }
        return at;
    }
    if fails::<BuiltWith>(value.get("built_with")) {
        return "/built_with".to_string();
    }
    if fails::<RecordingBlock>(value.get("recording")) {
        return "/recording".to_string();
    }
    "/".to_string()
}

/// Reads the manifest document `value`: the version is checked first, then the
/// document is read (unknown keys refused, the key named) and validated.
/// `manifest` is `Some` only when there are no errors.
pub fn read_manifest_value(value: &Value) -> ManifestReport {
    let failed = |text: String| ManifestReport {
        errors: vec![text],
        ..ManifestReport::default()
    };
    match value.get("schema_version") {
        None => return failed(message("/schema_version", "missing")),
        Some(v) if v.as_u64() == Some(u64::from(MANIFEST_SCHEMA_VERSION)) => {}
        Some(v)
            if v.as_u64()
                .is_some_and(|n| n > u64::from(MANIFEST_SCHEMA_VERSION)) =>
        {
            return failed(message(
                "/schema_version",
                format!(
                    "{v} is not supported; this version reads schema_version \
                     {MANIFEST_SCHEMA_VERSION} (the file was written by a newer version)"
                ),
            ));
        }
        Some(v) => {
            return failed(message(
                "/schema_version",
                format!(
                    "{v} is not supported; this version reads schema_version {MANIFEST_SCHEMA_VERSION}"
                ),
            ));
        }
    }
    let manifest = match ProjectManifest::deserialize(value) {
        Ok(manifest) => manifest,
        Err(e) => return failed(message(&error_pointer(value), e)),
    };
    let errors = manifest.validate();
    ManifestReport {
        manifest: errors.is_empty().then_some(manifest),
        errors,
        ..ManifestReport::default()
    }
}

/// Reads `<dir>/project.json`. `None` when there is no such file (a v0 folder).
/// Adds the warning for a `source.dataset` that does not exist and the note for a
/// file written by another build.
pub fn read_manifest_file(dir: &Path) -> Option<ManifestReport> {
    let path = dir.join(MANIFEST_FILE);
    if !path.is_file() {
        return None;
    }
    let value = match fs::read_to_string(&path) {
        Err(e) => {
            return Some(ManifestReport {
                errors: vec![format!("[{MANIFEST_FILE}] Could not read file: {e}")],
                ..ManifestReport::default()
            });
        }
        Ok(raw) => match serde_json::from_str::<Value>(&raw) {
            Err(e) => {
                return Some(ManifestReport {
                    errors: vec![format!("[{MANIFEST_FILE}] Invalid JSON: {e}")],
                    ..ManifestReport::default()
                });
            }
            Ok(value) => value,
        },
    };
    let mut report = read_manifest_value(&value);
    if let Some(manifest) = &report.manifest {
        for (role, body) in [("target", &manifest.target), ("chaser", &manifest.chaser)] {
            if let Some(source) = &body.source
                && !dir.join(&source.dataset).is_file()
            {
                report.warnings.push(message(
                    &format!("/{role}/source/dataset"),
                    format!(
                        "`{}` does not exist (the stored orbit is used)",
                        source.dataset
                    ),
                ));
            }
        }
        if let Some(built_with) = &manifest.built_with {
            let differences = built_with.differences(&BuiltWith::current());
            if !differences.is_empty() {
                report.notes.push(message(
                    "/built_with",
                    format!(
                        "written by {}; re-simulation is identical only for the same version",
                        differences.join(", ")
                    ),
                ));
            }
        }
    }
    Some(report)
}

/// The manifest as a file: pretty JSON, `schema_version` first, only the optional
/// keys it has.
pub fn manifest_to_json(manifest: &ProjectManifest) -> String {
    serde_json::to_string_pretty(manifest).expect("a manifest always serializes")
}

/// Writes `manifest` to `<dir>/project.json`.
pub fn save_manifest(dir: &Path, manifest: &ProjectManifest) -> std::io::Result<()> {
    fs::write(dir.join(MANIFEST_FILE), manifest_to_json(manifest))
}

/// The sim inputs of `<dir>/project.json`: `None` when the folder has no manifest
/// (a v0 folder), `Err` with the messages when it has one that cannot be used.
pub fn read_sim_inputs(dir: &Path) -> Option<Result<ManifestSimInputs, Vec<String>>> {
    let report = read_manifest_file(dir)?;
    Some(match report.manifest {
        Some(manifest) => manifest.sim_inputs(),
        None => Err(report.errors),
    })
}

/// Writes `manifest` to `<dir>/project.json`, reads the file back and returns the
/// sim inputs of what was read: the one path from files to sim inputs. When the
/// file cannot be written the inputs come from `manifest` itself and the write
/// error is returned beside them. `Err` when the manifest, as written or as read
/// back, is not valid.
pub fn save_and_reload(
    dir: &Path,
    manifest: &ProjectManifest,
) -> Result<(ManifestSimInputs, Option<String>), Vec<String>> {
    if let Err(e) = save_manifest(dir, manifest) {
        let inputs = manifest.sim_inputs()?;
        return Ok((
            inputs,
            Some(format!("could not write {MANIFEST_FILE}: {e}")),
        ));
    }
    match read_sim_inputs(dir) {
        Some(result) => result.map(|inputs| (inputs, None)),
        None => Err(vec![format!(
            "[{MANIFEST_FILE}] disappeared after it was written"
        )]),
    }
}
