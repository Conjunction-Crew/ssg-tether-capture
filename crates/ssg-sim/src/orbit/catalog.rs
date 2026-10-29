//! The space-object catalog: dataset records, their element buffer and the loader.

use std::f64::consts::PI;
use std::fs;
use std::path::{Path, PathBuf};

use bevy::math::DVec3;
use bevy::prelude::*;
use brahe::{Epoch, GM_EARTH, TimeSystem};
use nalgebra::Vector6;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::project::{AssetsDir, WorkingDirectory};
use crate::telemetry::{LogEvent, LogLevel};
use crate::time::WorldTime;

// unused today; the P3 component/resource audit decides
#[allow(dead_code)]
/// One Space-Track GP (OMM JSON) record: the keys the sim reads. Other keys of a
/// record are ignored on read and not written back. `project.json` carries it as the
/// snapshot a body starts from.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub struct JsonOrbital {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "OBJECT_NAME")]
    pub object_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "OBJECT_ID")]
    pub object_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "EPOCH")]
    pub epoch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "MEAN_MOTION")]
    pub mean_motion: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "ECCENTRICITY")]
    pub eccentricity: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "INCLINATION")]
    pub inclination: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "RA_OF_ASC_NODE")]
    pub ra_of_asc_node: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "ARG_OF_PERICENTER")]
    pub arg_of_pericenter: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "MEAN_ANOMALY")]
    pub mean_anomaly: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "EPHEMERIS_TYPE")]
    pub ephemeris_type: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "CLASSIFICATION_TYPE")]
    pub classification_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "NORAD_CAT_ID")]
    pub norad_cat_id: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "ELEMENT_SET_NO")]
    pub element_set_no: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "REV_AT_EPOCH")]
    pub rev_at_epoch: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "BSTAR")]
    pub bstar: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "MEAN_MOTION_DOT")]
    pub mean_motion_dot: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "MEAN_MOTION_DDOT")]
    pub mean_motion_ddot: Option<Value>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(from = "JsonOrbitalDataSource")]
pub struct JsonOrbitalData {
    // unused today; the P3 component/resource audit decides
    #[allow(dead_code)]
    pub name: String,
    pub data: Vec<JsonOrbital>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(untagged)]
enum JsonOrbitalDataSource {
    Wrapped {
        name: String,
        data: Vec<JsonOrbital>,
    },
    Bare(Vec<JsonOrbital>),
}

impl From<JsonOrbitalDataSource> for JsonOrbitalData {
    fn from(source: JsonOrbitalDataSource) -> Self {
        match source {
            JsonOrbitalDataSource::Wrapped { name, data } => Self { name, data },
            JsonOrbitalDataSource::Bare(data) => Self {
                name: String::new(),
                data,
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct SpaceCatalogEntry {
    pub gpu_index: usize,
    pub norad_id: u32,
    pub object_name: String,
    pub object_id: String,
    pub search_blob: String,
    /// The full-precision GP record the entry was loaded from (F-27).
    pub gp: GpElements,
}

impl SpaceCatalogEntry {
    pub fn display_name(&self) -> &str {
        if self.object_name.is_empty() {
            if self.object_id.is_empty() {
                "Unnamed Object"
            } else {
                &self.object_id
            }
        } else {
            &self.object_name
        }
    }

    pub fn display_label(&self) -> String {
        if self.object_id.is_empty() {
            format!("{} · NORAD {}", self.display_name(), self.norad_id)
        } else {
            format!(
                "{} · NORAD {} · {}",
                self.display_name(),
                self.norad_id,
                self.object_id
            )
        }
    }
}

#[derive(Resource, Debug, Clone, Default)]
pub struct SpaceObjectCatalog {
    pub entries: Vec<SpaceCatalogEntry>,
}

/// One GP (OMM) record's mean elements at full precision, with the record's own
/// epoch (F-27). Catalog picks carry it so the sim starts the body from SGP4
/// (F-10); the f32 [`GpuOrbitalElements`] are derived from it for the map.
#[derive(Debug, Clone, PartialEq)]
pub struct GpElements {
    pub norad_id: u32,
    /// The record's `EPOCH` verbatim; brahe's SGP4 parses it. Empty when the
    /// record has none (SGP4 then fails at init and logs it).
    pub epoch_str: String,
    /// The record's epoch, or the loader's reference epoch when it has none.
    pub epoch: Epoch,
    pub mean_motion_rev_per_day: f64,
    pub eccentricity: f64,
    pub inclination_deg: f64,
    pub raan_deg: f64,
    pub arg_perigee_deg: f64,
    pub mean_anomaly_deg: f64,
    pub bstar: Option<f64>,
    pub mean_motion_dot: Option<f64>,
    pub mean_motion_ddot: Option<f64>,
    pub ephemeris_type: Option<u8>,
    pub classification: Option<char>,
}

impl GpElements {
    /// The semi-major axis from the mean motion (two-body, `GM_EARTH`).
    pub fn semi_major_axis_m(&self) -> f64 {
        let mean_motion_rad_s = self.mean_motion_rev_per_day * 2.0 * PI / 86400.0;
        (GM_EARTH / (mean_motion_rad_s * mean_motion_rad_s)).powf(1.0 / 3.0)
    }

    /// `[a, e, i, raan, argp, mean anomaly]`, metres and radians.
    pub fn kepler_elements_rad(&self) -> Vector6<f64> {
        Vector6::new(
            self.semi_major_axis_m(),
            self.eccentricity,
            self.inclination_deg.to_radians(),
            self.raan_deg.to_radians(),
            self.arg_perigee_deg.to_radians(),
            self.mean_anomaly_deg.to_radians(),
        )
    }
}

/// One catalog object's elements as stored in the catalog buffer (f32).
///
/// The `Gpu*` names are kept because the P2 tests import them; the sim never
/// touches the GPU (the renderer mirrors this as `GpuOrbitalElementsShader`).
/// P4 may rename them with F-27.
#[derive(Clone, Copy)]
pub struct GpuOrbitalElements {
    pub id: u32,
    pub a: f32,
    pub e: f32,
    pub i: f32,
    pub raan: f32,
    pub argp: f32,
    pub mean_anomaly: f32,
    pub epoch_offset_seconds: f32,
}

pub fn propagate_catalog_eci_state(
    element: &GpuOrbitalElements,
    current_epoch_offset_seconds: f32,
) -> Option<(Vec3, Vec3)> {
    let semi_major_axis = element.a as f64;
    let eccentricity = element.e as f64;

    if semi_major_axis <= 0.0 || !(0.0..1.0).contains(&eccentricity) {
        return None;
    }

    let dt_seconds = current_epoch_offset_seconds as f64 - element.epoch_offset_seconds as f64;
    let mean_motion =
        (3.986_004_4e14_f64 / (semi_major_axis * semi_major_axis * semi_major_axis)).sqrt();
    let mean_anomaly = wrap_angle(element.mean_anomaly as f64 + mean_motion * dt_seconds);
    let eccentric_anomaly = solve_eccentric_anomaly(mean_anomaly, eccentricity);

    let sin_e = eccentric_anomaly.sin();
    let cos_e = eccentric_anomaly.cos();
    let one_minus_e2 = (1.0 - eccentricity * eccentricity).max(0.0);
    let sqrt_one_minus_e2 = one_minus_e2.sqrt();
    let radius = semi_major_axis * (1.0 - eccentricity * cos_e);

    if radius <= 0.0 {
        return None;
    }

    let position_pf = DVec3::new(
        semi_major_axis * (cos_e - eccentricity),
        semi_major_axis * sqrt_one_minus_e2 * sin_e,
        0.0,
    );

    let velocity_scale = (3.986_004_4e14_f64 * semi_major_axis).sqrt() / radius;
    let velocity_pf = DVec3::new(
        -velocity_scale * sin_e,
        velocity_scale * sqrt_one_minus_e2 * cos_e,
        0.0,
    );

    let position_eci = perifocal_to_eci(
        position_pf,
        element.i as f64,
        element.raan as f64,
        element.argp as f64,
    );
    let velocity_eci = perifocal_to_eci(
        velocity_pf,
        element.i as f64,
        element.raan as f64,
        element.argp as f64,
    );

    Some((position_eci.as_vec3(), velocity_eci.as_vec3()))
}

fn wrap_angle(angle: f64) -> f64 {
    angle - std::f64::consts::TAU * ((angle + std::f64::consts::PI) / std::f64::consts::TAU).floor()
}

fn solve_eccentric_anomaly(mean_anomaly: f64, eccentricity: f64) -> f64 {
    let mut eccentric_anomaly = if eccentricity > 0.8 {
        if mean_anomaly < 0.0 {
            -std::f64::consts::PI
        } else {
            std::f64::consts::PI
        }
    } else {
        mean_anomaly
    };

    for _ in 0..6 {
        let sin_e = eccentric_anomaly.sin();
        let cos_e = eccentric_anomaly.cos();
        let residual = eccentric_anomaly - eccentricity * sin_e - mean_anomaly;
        let derivative = 1.0 - eccentricity * cos_e;
        eccentric_anomaly -= residual / derivative;
    }

    eccentric_anomaly
}

fn perifocal_to_eci(value: DVec3, inclination: f64, raan: f64, argp: f64) -> DVec3 {
    let cos_raan = raan.cos();
    let sin_raan = raan.sin();
    let cos_argp = argp.cos();
    let sin_argp = argp.sin();
    let cos_i = inclination.cos();
    let sin_i = inclination.sin();

    let r11 = cos_raan * cos_argp - sin_raan * sin_argp * cos_i;
    let r12 = -cos_raan * sin_argp - sin_raan * cos_argp * cos_i;
    let r21 = sin_raan * cos_argp + cos_raan * sin_argp * cos_i;
    let r22 = -sin_raan * sin_argp + cos_raan * cos_argp * cos_i;
    let r31 = sin_argp * sin_i;
    let r32 = cos_argp * sin_i;

    DVec3::new(
        r11 * value.x + r12 * value.y,
        r21 * value.x + r22 * value.y,
        r31 * value.x + r32 * value.y,
    )
}

/// The catalog buffer, filled by [`load_dataset_entities`]; the renderer extracts
/// it to the GPU.
#[derive(Resource, Clone, Default)]
pub struct GpuElements(pub Vec<GpuOrbitalElements>);

#[derive(Resource, Default)]
pub struct GpuComputeEpochOrigin(pub Option<Epoch>);

pub fn load_dataset_entities(
    mut space_catalog: ResMut<SpaceObjectCatalog>,
    gpu_elements: Option<ResMut<GpuElements>>,
    world_time: Res<WorldTime>,
    gpu_epoch_origin: Option<ResMut<GpuComputeEpochOrigin>>,
    assets_dir: Res<AssetsDir>,
    working_directory: Option<Res<WorkingDirectory>>,
    mut log_events: MessageWriter<LogEvent>,
) {
    let plans_dir = assets_dir.0.join("datasets");
    let mut gpu_elements = gpu_elements;
    let mut gpu_epoch_origin = gpu_epoch_origin;
    let reference_epoch = if let Some(origin) = gpu_epoch_origin.as_deref_mut() {
        let epoch = origin.0.get_or_insert(world_time.epoch);
        *epoch
    } else {
        world_time.epoch
    };

    space_catalog.entries.clear();

    if let Some(gpu_elements) = gpu_elements.as_deref_mut() {
        gpu_elements.0.clear();
    }

    // The project's own datasets first (sorted by name), then the assets' (P6).
    let mut dataset_files: Vec<PathBuf> = Vec::new();
    if let Some(working_directory) = working_directory.as_deref() {
        let mut project_files: Vec<PathBuf> =
            fs::read_dir(Path::new(&working_directory.path).join("datasets"))
                .into_iter()
                .flatten()
                .flatten()
                .map(|entry| entry.path())
                .collect();
        project_files.sort();
        dataset_files.extend(project_files);
    }
    match fs::read_dir(&plans_dir) {
        Ok(iter) => dataset_files.extend(iter.flatten().map(|entry| entry.path())),
        Err(e) => warn!("Could not read dataset directory {plans_dir:?}: {e}"),
    }
    for path in dataset_files {
        if path
            .extension()
            .is_some_and(|extension| extension == "json")
            && let Ok(raw_json) = fs::read_to_string(&path)
        {
            if let Ok(dataset) = serde_json::from_str(&raw_json) {
                let data: JsonOrbitalData = dataset;
                for object in data.data {
                    let Some((id, gp, elements)) = gp_record_to_elements(&object, reference_epoch)
                    else {
                        if let Some(event) = missing_field_event(&object) {
                            log_events.write(event);
                        }
                        continue;
                    };
                    let object_name = object
                        .object_name
                        .clone()
                        .filter(|name| !name.is_empty())
                        .or_else(|| object.object_id.clone().filter(|id| !id.is_empty()))
                        .unwrap_or_else(|| format!("NORAD {}", id));
                    let object_id = object
                        .object_id
                        .clone()
                        .filter(|id| !id.is_empty())
                        .unwrap_or_default();
                    let gpu_index = space_catalog.entries.len();

                    space_catalog.entries.push(SpaceCatalogEntry {
                        gpu_index,
                        norad_id: id as u32,
                        search_blob: format!(
                            "{} {} {}",
                            object_name.to_lowercase(),
                            object_id.to_lowercase(),
                            id
                        ),
                        object_name,
                        object_id,
                        gp,
                    });

                    if let Some(gpu_elements) = gpu_elements.as_deref_mut() {
                        gpu_elements.0.push(elements);
                    }
                }
            } else {
                log_events.write(LogEvent {
                    level: LogLevel::Error,
                    source: "dataset",
                    message: format!("Failed to parse dataset JSON: {}", path.display()),
                });
            }
        }
    }

    let entry_count = space_catalog.entries.len();
    space_catalog
        .entries
        .sort_by(|left, right| left.display_name().cmp(right.display_name()));
    if entry_count > 0 {
        log_events.write(LogEvent {
            level: LogLevel::Info,
            source: "propagation",
            message: format!("Dataset loaded: {entry_count} objects"),
        });
    }
}

/// The full-precision elements of one GP `record` and the catalog buffer entry
/// derived from them: mean motion to semi-major axis, degrees to radians, values
/// stored as f32, the epoch offset taken against `reference_epoch` (which a
/// missing or unparsable `EPOCH` defaults to). `None` if a field is missing or
/// not a number.
pub fn gp_record_to_elements(
    record: &JsonOrbital,
    reference_epoch: Epoch,
) -> Option<(u64, GpElements, GpuOrbitalElements)> {
    let id = record.norad_cat_id.as_ref()?.as_u64()?;
    let mean_motion = record.mean_motion.as_ref()?.as_f64()?;
    let eccentricity = record.eccentricity.as_ref()?.as_f64()?;
    let inclination = record.inclination.as_ref()?.as_f64()?;
    let raan = record.ra_of_asc_node.as_ref()?.as_f64()?;
    let argp = record.arg_of_pericenter.as_ref()?.as_f64()?;
    let mean_anomaly = record.mean_anomaly.as_ref()?.as_f64()?;
    let object_epoch = record
        .epoch
        .as_deref()
        .and_then(parse_dataset_epoch_fast)
        .unwrap_or(reference_epoch);
    let optional_f64 = |field: &Option<Value>| field.as_ref().and_then(Value::as_f64);

    let gp = GpElements {
        norad_id: id as u32,
        epoch_str: record.epoch.clone().unwrap_or_default(),
        epoch: object_epoch,
        mean_motion_rev_per_day: mean_motion,
        eccentricity,
        inclination_deg: inclination,
        raan_deg: raan,
        arg_perigee_deg: argp,
        mean_anomaly_deg: mean_anomaly,
        bstar: optional_f64(&record.bstar),
        mean_motion_dot: optional_f64(&record.mean_motion_dot),
        mean_motion_ddot: optional_f64(&record.mean_motion_ddot),
        ephemeris_type: record
            .ephemeris_type
            .as_ref()
            .and_then(Value::as_u64)
            .and_then(|value| u8::try_from(value).ok()),
        classification: record
            .classification_type
            .as_deref()
            .and_then(|value| value.chars().next()),
    };
    let elements = gp.kepler_elements_rad();
    let gpu = GpuOrbitalElements {
        id: id as u32,
        a: elements[0] as f32,
        e: elements[1] as f32,
        i: elements[2] as f32,
        raan: elements[3] as f32,
        argp: elements[4] as f32,
        mean_anomaly: elements[5] as f32,
        epoch_offset_seconds: (object_epoch - reference_epoch) as f32,
    };

    Some((id, gp, gpu))
}

/// The log line the loader writes for a record that [`gp_record_to_elements`]
/// rejects: the first missing field, in load order. A field that is present but
/// not a number skips the record without a log line, as before P3.
fn missing_field_event(record: &JsonOrbital) -> Option<LogEvent> {
    let Some(id_val) = &record.norad_cat_id else {
        return Some(LogEvent {
            level: LogLevel::Error,
            source: "dataset",
            message: "Failed to parse norad_cat_id for an object".to_string(),
        });
    };
    let id = id_val.as_u64()?;
    let Some(mean_motion_val) = &record.mean_motion else {
        return Some(LogEvent {
            level: LogLevel::Error,
            source: "dataset",
            message: format!("Failed to parse mean_motion for object {:?}", id_val),
        });
    };
    mean_motion_val.as_f64()?;
    for (field, name) in [
        (&record.eccentricity, "eccentricity"),
        (&record.inclination, "inclination"),
        (&record.ra_of_asc_node, "ra_of_asc_node"),
        (&record.arg_of_pericenter, "arg_of_pericenter"),
        (&record.mean_anomaly, "mean_anomaly"),
    ] {
        let Some(value) = field else {
            return Some(LogEvent {
                level: LogLevel::Error,
                source: "propagation",
                message: format!("Failed to parse {name} for object {id}"),
            });
        };
        value.as_f64()?;
    }
    None
}

fn parse_dataset_epoch_fast(raw: &str) -> Option<Epoch> {
    let bytes = raw.as_bytes();
    let is_fixed_format = bytes.len() >= 19
        && bytes.get(4) == Some(&b'-')
        && bytes.get(7) == Some(&b'-')
        && bytes.get(10) == Some(&b'T')
        && bytes.get(13) == Some(&b':')
        && bytes.get(16) == Some(&b':');

    if !is_fixed_format {
        return Epoch::from_string(raw).or_else(|| Epoch::from_string(&format!("{raw}Z")));
    }

    let year = raw.get(0..4)?.parse().ok()?;
    let month = raw.get(5..7)?.parse().ok()?;
    let day = raw.get(8..10)?.parse().ok()?;
    let hour = raw.get(11..13)?.parse().ok()?;
    let minute = raw.get(14..16)?.parse().ok()?;
    let whole_seconds: f64 = raw.get(17..19)?.parse().ok()?;

    let fractional_seconds = raw
        .get(19..)
        .and_then(|suffix| suffix.strip_prefix('.'))
        .and_then(|digits| {
            if digits.is_empty() {
                return None;
            }

            let value = digits.parse::<u32>().ok()? as f64;
            let scale = 10_f64.powi(digits.len() as i32);
            Some(value / scale)
        })
        .unwrap_or(0.0);

    Some(Epoch::from_datetime(
        year,
        month,
        day,
        hour,
        minute,
        whole_seconds + fractional_seconds,
        0.0,
        TimeSystem::UTC,
    ))
}
