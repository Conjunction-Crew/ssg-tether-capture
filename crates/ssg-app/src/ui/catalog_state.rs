//! Space catalog panel state.

use bevy::prelude::*;

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
