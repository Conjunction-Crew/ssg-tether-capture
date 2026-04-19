use bevy::prelude::*;
use nalgebra::Vector6;

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
    Target,
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
    pub target: Option<SelectedOrbitalObject>,
    pub chaser: Option<SelectedOrbitalObject>,
}
