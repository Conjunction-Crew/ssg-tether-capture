use bevy::prelude::*;

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
