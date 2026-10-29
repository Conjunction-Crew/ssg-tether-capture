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
};
use crate::project::epoch::{format_utc, parse_utc};

/// The manifest's file name in the project folder; the plan loader skips it.
pub const MANIFEST_FILE: &str = "project.json";

/// The `schema_version` of the manifests this build reads and writes.
pub const MANIFEST_SCHEMA_VERSION: u32 = 1;

/// The avian3d features the workspace builds with (`Cargo.toml`); results are
/// bit-identical only on the same build and platform (F-48).
const AVIAN_FEATURES: &[&str] = &["simd", "parallel"];

/// Ends every "not supported yet" message (design D2, E1).
const NOT_YET: &str = "is not supported yet; it is applied from v0.4.0-rc.4";

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
    /// Mass in kg; greater than 0. The target's is applied from v0.4.0-rc.1; the
    /// chaser's is refused until v0.4.0-rc.4.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mass_kg: Option<f64>,
    /// Inertia tensor in kg m², row-major. Refused until v0.4.0-rc.4.
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
    /// Chaser only: an offset from the target along V-bar or R-bar. Refused until
    /// v0.4.0-rc.4.
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
    /// Distance from the target in metres.
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
}

impl BuiltWith {
    /// The running build.
    pub fn current() -> Self {
        Self {
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            target: format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS),
            avian_features: AVIAN_FEATURES.iter().map(|f| f.to_string()).collect(),
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

impl ProjectManifest {
    /// Every rule of design D2 that serde cannot check, in field order, as
    /// `[project.json] <json-pointer>: <message>`.
    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();
        if self.schema_version != MANIFEST_SCHEMA_VERSION {
            errors.push(message(
                "/schema_version",
                format!("must be {MANIFEST_SCHEMA_VERSION}"),
            ));
        }
        if self.name.trim().is_empty() {
            errors.push(message("/name", "must not be blank"));
        }
        let start = parse_utc(&self.start_epoch_utc);
        if start.is_none() {
            errors.push(message(
                "/start_epoch_utc",
                format!(
                    "`{}` is not a UTC epoch (YYYY-MM-DDTHH:MM:SS.ffffffZ)",
                    self.start_epoch_utc
                ),
            ));
        }
        if self
            .active_plan
            .as_deref()
            .is_some_and(|id| id.trim().is_empty())
        {
            errors.push(message("/active_plan", "must not be blank"));
        }
        self.target.validate("/target", Role::Target, &mut errors);
        self.chaser.validate("/chaser", Role::Chaser, &mut errors);
        errors
    }

    /// The sim inputs of this manifest. `Err` carries every message of
    /// [`Self::validate`] (and nothing else can fail after it passes).
    pub fn sim_inputs(&self) -> Result<ManifestSimInputs, Vec<String>> {
        let errors = self.validate();
        if !errors.is_empty() {
            return Err(errors);
        }
        let start_epoch = parse_utc(&self.start_epoch_utc)
            .ok_or_else(|| vec![message("/start_epoch_utc", "is not a UTC epoch")])?;
        Ok(ManifestSimInputs {
            start_epoch,
            target: self.target.selected_object("/target", start_epoch)?,
            chaser: self.chaser.selected_object("/chaser", start_epoch)?,
            active_plan: self.active_plan.clone(),
        })
    }

    /// The manifest of a scenario about to run: the inverse of
    /// [`Self::sim_inputs`]. A body with a catalog record is written as that record
    /// (`OBJECT_NAME` is its label), any other as its elements. `start_epoch` is
    /// written in the file format. `Err` when the target or the chaser is not
    /// selected.
    pub fn from_selection(
        name: &str,
        start_epoch: Epoch,
        active_plan: Option<&str>,
        selection: &OrbitalSelectionState,
    ) -> Result<Self, String> {
        let body = |role: &str, selected: &Option<SelectedOrbitalObject>| {
            selected
                .as_ref()
                .map(ManifestBody::from_selected)
                .ok_or_else(|| format!("no {role} is selected"))
        };
        Ok(Self {
            schema_version: MANIFEST_SCHEMA_VERSION,
            name: name.to_string(),
            start_epoch_utc: format_utc(start_epoch),
            active_plan: active_plan.map(str::to_string),
            target: body("target", &selection.target)?,
            chaser: body("chaser", &selection.chaser)?,
            built_with: Some(BuiltWith::current()),
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
    Target,
    Chaser,
}

impl ManifestBody {
    fn validate(&self, at: &str, role: Role, errors: &mut Vec<String>) {
        if self.label.trim().is_empty() {
            errors.push(message(&format!("{at}/label"), "must not be blank"));
        }
        self.validate_orbit(at, role, errors);
        if let Some(source) = &self.source
            && source.dataset.trim().is_empty()
        {
            errors.push(message(
                &format!("{at}/source/dataset"),
                "must not be blank",
            ));
        }
        match (self.mass_kg, role) {
            (Some(_), Role::Chaser) => errors.push(message(
                &format!("{at}/mass_kg"),
                format!("the chaser's mass {NOT_YET}"),
            )),
            (Some(mass), Role::Target) if !(mass.is_finite() && mass > 0.0) => {
                errors.push(message(&format!("{at}/mass_kg"), "must be greater than 0"))
            }
            _ => {}
        }
        if self.inertia_kg_m2.is_some() {
            errors.push(message(
                &format!("{at}/inertia_kg_m2"),
                format!("the inertia {NOT_YET}"),
            ));
        }
    }

    fn validate_orbit(&self, at: &str, role: Role, errors: &mut Vec<String>) {
        let at = format!("{at}/orbit");
        match &self.orbit {
            ManifestOrbit::Gp(record) => {
                let at = format!("{at}/gp");
                let is_number =
                    |value: &Option<Value>| value.as_ref().is_some_and(|v| v.as_f64().is_some());
                if !record
                    .norad_cat_id
                    .as_ref()
                    .is_some_and(|v| v.as_u64().is_some())
                {
                    errors.push(message(
                        &format!("{at}/NORAD_CAT_ID"),
                        "missing, or not a whole number",
                    ));
                }
                let numbers = [
                    &record.mean_motion,
                    &record.eccentricity,
                    &record.inclination,
                    &record.ra_of_asc_node,
                    &record.arg_of_pericenter,
                    &record.mean_anomaly,
                ];
                for (key, value) in GP_NUMBER_KEYS.iter().zip(numbers) {
                    if !is_number(value) {
                        errors.push(message(&format!("{at}/{key}"), "missing, or not a number"));
                    }
                }
            }
            ManifestOrbit::Elements(elements) => {
                let at = format!("{at}/elements");
                for (key, value) in [
                    ("semi_major_axis_m", elements.semi_major_axis_m),
                    ("eccentricity", elements.eccentricity),
                    ("inclination_rad", elements.inclination_rad),
                    ("raan_rad", elements.raan_rad),
                    ("arg_perigee_rad", elements.arg_perigee_rad),
                    ("mean_anomaly_rad", elements.mean_anomaly_rad),
                ] {
                    if !value.is_finite() {
                        errors.push(message(&format!("{at}/{key}"), "must be a finite number"));
                    }
                }
                if elements.semi_major_axis_m.is_finite() && elements.semi_major_axis_m <= 0.0 {
                    errors.push(message(
                        &format!("{at}/semi_major_axis_m"),
                        "must be greater than 0",
                    ));
                }
                if elements.eccentricity.is_finite() && !(0.0..1.0).contains(&elements.eccentricity)
                {
                    errors.push(message(
                        &format!("{at}/eccentricity"),
                        "must be at least 0 and below 1",
                    ));
                }
                if parse_utc(&elements.epoch_utc).is_none() {
                    errors.push(message(
                        &format!("{at}/epoch_utc"),
                        format!(
                            "`{}` is not a UTC epoch (YYYY-MM-DDTHH:MM:SS.ffffffZ)",
                            elements.epoch_utc
                        ),
                    ));
                }
            }
            ManifestOrbit::RelativeToTarget(relative) => {
                let at = format!("{at}/relative_to_target");
                match role {
                    Role::Target => errors.push(message(
                        &at,
                        "only the chaser can be placed relative to the target",
                    )),
                    Role::Chaser => errors.push(message(
                        &at,
                        format!("an orbit relative to the target {NOT_YET}"),
                    )),
                }
                if !relative.offset_m.is_finite() {
                    errors.push(message(
                        &format!("{at}/offset_m"),
                        "must be a finite number",
                    ));
                }
            }
        }
    }

    /// The selected object of a valid body. `at` is only used in messages.
    fn selected_object(
        &self,
        at: &str,
        start_epoch: Epoch,
    ) -> Result<SelectedOrbitalObject, Vec<String>> {
        let source = OrbitalSelectionSource::Custom {
            label: self.label.clone(),
        };
        match &self.orbit {
            ManifestOrbit::Gp(record) => {
                let (_, gp, _) = gp_record_to_elements(record, start_epoch).ok_or_else(|| {
                    vec![message(
                        &format!("{at}/orbit/gp"),
                        "the record does not give an orbit",
                    )]
                })?;
                // A catalog pick, as the app makes it (F-27): SGP4 at the start epoch.
                Ok(SelectedOrbitalObject {
                    source,
                    elements: EditableOrbitalElements::from_gp(&gp),
                    gp: Some(gp),
                })
            }
            ManifestOrbit::Elements(e) => {
                let epoch = parse_utc(&e.epoch_utc).ok_or_else(|| {
                    vec![message(
                        &format!("{at}/orbit/elements/epoch_utc"),
                        "is not a UTC epoch",
                    )]
                })?;
                Ok(SelectedOrbitalObject {
                    source,
                    elements: EditableOrbitalElements {
                        semi_major_axis_m: e.semi_major_axis_m,
                        eccentricity: e.eccentricity,
                        inclination_rad: e.inclination_rad,
                        raan_rad: e.raan_rad,
                        arg_perigee_rad: e.arg_perigee_rad,
                        mean_anomaly_rad: e.mean_anomaly_rad,
                        epoch,
                    },
                    gp: None,
                })
            }
            ManifestOrbit::RelativeToTarget(_) => Err(vec![message(
                &format!("{at}/orbit/relative_to_target"),
                format!("an orbit relative to the target {NOT_YET}"),
            )]),
        }
    }

    fn from_selected(selected: &SelectedOrbitalObject) -> Self {
        let label = match &selected.source {
            OrbitalSelectionSource::Catalog { label, .. }
            | OrbitalSelectionSource::Custom { label } => label.clone(),
        };
        let orbit = match &selected.gp {
            Some(gp) => ManifestOrbit::Gp(Box::new(gp_to_record(&label, gp))),
            None => {
                let e = &selected.elements;
                ManifestOrbit::Elements(ManifestElements {
                    semi_major_axis_m: e.semi_major_axis_m,
                    eccentricity: e.eccentricity,
                    inclination_rad: e.inclination_rad,
                    raan_rad: e.raan_rad,
                    arg_perigee_rad: e.arg_perigee_rad,
                    mean_anomaly_rad: e.mean_anomaly_rad,
                    epoch_utc: format_utc(e.epoch),
                })
            }
        };
        Self {
            label,
            orbit,
            source: None,
            mass_kg: None,
            inertia_kg_m2: None,
        }
    }
}

/// The GP record a catalog pick came from, as far as the sim reads it.
fn gp_to_record(label: &str, gp: &GpElements) -> JsonOrbital {
    JsonOrbital {
        object_name: Some(label.to_string()),
        object_id: None,
        epoch: (!gp.epoch_str.is_empty()).then(|| gp.epoch_str.clone()),
        mean_motion: Some(Value::from(gp.mean_motion_rev_per_day)),
        eccentricity: Some(Value::from(gp.eccentricity)),
        inclination: Some(Value::from(gp.inclination_deg)),
        ra_of_asc_node: Some(Value::from(gp.raan_deg)),
        arg_of_pericenter: Some(Value::from(gp.arg_perigee_deg)),
        mean_anomaly: Some(Value::from(gp.mean_anomaly_deg)),
        ephemeris_type: gp.ephemeris_type.map(Value::from),
        classification_type: gp.classification.map(|c| c.to_string()),
        norad_cat_id: Some(Value::from(gp.norad_id)),
        element_set_no: None,
        rev_at_epoch: None,
        bstar: gp.bstar.map(Value::from),
        mean_motion_dot: gp.mean_motion_dot.map(Value::from),
        mean_motion_ddot: gp.mean_motion_ddot.map(Value::from),
    }
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
                        "written by {}; results are bit-identical only on the same build and platform",
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
