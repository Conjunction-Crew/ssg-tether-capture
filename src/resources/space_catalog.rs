use bevy::prelude::*;
use nalgebra::Vector6;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct SpaceCatalogEntry {
    pub gpu_index: usize,
    pub norad_id: u32,
    pub object_name: String,
    pub object_id: String,
    pub search_blob: String,
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

#[derive(Resource, Debug, Clone)]
pub struct SpaceCatalogUiState {
    pub show_catalog: bool,
    pub show_points: bool,
    pub show_satellite_indicator: bool,
    pub search_text: String,
    pub search_focused: bool,
    pub selected_index: Option<usize>,
    pub page: usize,
}

impl Default for SpaceCatalogUiState {
    fn default() -> Self {
        Self {
            show_catalog: false,
            show_points: false,
            show_satellite_indicator: true,
            search_text: String::new(),
            search_focused: false,
            selected_index: None,
            page: 0,
        }
    }
}

#[derive(Resource, Debug, Clone, Default)]
pub struct FilteredSpaceCatalogResults(pub Vec<usize>);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrbitalSelectionRole {
    Rso,
    Chaser,
}

#[derive(Debug, Clone)]
pub enum OrbitalSelectionSource {
    Catalog {
        catalog_index: usize,
        gpu_index: usize,
        label: String,
    },
    Custom {
        label: String,
    },
}

#[derive(Debug, Clone, Copy)]
pub struct EditableOrbitalElements {
    pub semi_major_axis_m: f64,
    pub eccentricity: f64,
    pub inclination_rad: f64,
    pub raan_rad: f64,
    pub arg_perigee_rad: f64,
    pub mean_anomaly_rad: f64,
    pub epoch_offset_seconds: f64,
}

impl EditableOrbitalElements {
    pub fn to_vec6(&self) -> Vector6<f64> {
        Vector6::new(
            self.semi_major_axis_m,
            self.eccentricity,
            self.inclination_rad,
            self.raan_rad,
            self.arg_perigee_rad,
            self.mean_anomaly_rad,
        )
    }
}

#[derive(Debug, Clone)]
pub struct SelectedOrbitalObject {
    pub source: OrbitalSelectionSource,
    pub elements: EditableOrbitalElements,
}

#[derive(Resource, Debug, Clone, Default)]
pub struct OrbitalSelectionState {
    pub rso: Option<SelectedOrbitalObject>,
    pub chaser: Option<SelectedOrbitalObject>,
}

/// Serializable orbital-element block stored on a capture plan so the RSO and chaser
/// orbits can be pre-specified in JSON. Uses SI base units (metres / radians) to match
/// [`EditableOrbitalElements`] and `ISS_ORBIT`.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OrbitSpec {
    /// Optional display label for this orbit.
    #[serde(default)]
    pub label: Option<String>,
    pub semi_major_axis_m: f64,
    pub eccentricity: f64,
    pub inclination_rad: f64,
    pub raan_rad: f64,
    pub arg_perigee_rad: f64,
    pub mean_anomaly_rad: f64,
    #[serde(default)]
    pub epoch_offset_seconds: f64,
}

impl From<&OrbitSpec> for EditableOrbitalElements {
    fn from(spec: &OrbitSpec) -> Self {
        EditableOrbitalElements {
            semi_major_axis_m: spec.semi_major_axis_m,
            eccentricity: spec.eccentricity,
            inclination_rad: spec.inclination_rad,
            raan_rad: spec.raan_rad,
            arg_perigee_rad: spec.arg_perigee_rad,
            mean_anomaly_rad: spec.mean_anomaly_rad,
            epoch_offset_seconds: spec.epoch_offset_seconds,
        }
    }
}

impl OrbitSpec {
    /// Builds a [`SelectedOrbitalObject`] (as a `Custom` source) from this spec, using the
    /// spec's own label if present or `default_label` otherwise.
    pub fn to_selected(&self, default_label: &str) -> SelectedOrbitalObject {
        SelectedOrbitalObject {
            source: OrbitalSelectionSource::Custom {
                label: self
                    .label
                    .clone()
                    .unwrap_or_else(|| default_label.to_string()),
            },
            elements: EditableOrbitalElements::from(self),
        }
    }

    /// Builds an [`OrbitSpec`] from editable elements, for persisting a selection back
    /// into a capture plan's JSON.
    pub fn from_elements(elements: &EditableOrbitalElements, label: &str) -> Self {
        OrbitSpec {
            label: Some(label.to_string()),
            semi_major_axis_m: elements.semi_major_axis_m,
            eccentricity: elements.eccentricity,
            inclination_rad: elements.inclination_rad,
            raan_rad: elements.raan_rad,
            arg_perigee_rad: elements.arg_perigee_rad,
            mean_anomaly_rad: elements.mean_anomaly_rad,
            epoch_offset_seconds: elements.epoch_offset_seconds,
        }
    }
}
