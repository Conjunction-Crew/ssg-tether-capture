//! The Space Catalog panel on egui (P8 S6): the search box (F-36), the results
//! (48 per page), the pager and the data-point toggle. It opens only in map view.

use bevy_egui::{EguiContexts, egui};

use super::*;
use ssg_sim::orbit::catalog::SpaceCatalogEntry;

const CATALOG_PAGE_SIZE: usize = 48;
/// The panel's width (rc.3), plus the margin it keeps from the window's edge.
const CATALOG_WIDTH: f32 = 340.0;
const CATALOG_MARGIN: f32 = 12.0;

pub fn reset_space_catalog_ui_state(
    mut catalog_ui: ResMut<SpaceCatalogUiState>,
    mut filtered_results: ResMut<FilteredSpaceCatalogResults>,
) {
    *catalog_ui = SpaceCatalogUiState::default();
    filtered_results.0.clear();
}

/// The indices of the entries `search` finds (all of them when it is blank).
pub fn filter_catalog(entries: &[SpaceCatalogEntry], search: &str) -> Vec<usize> {
    let search = search.trim().to_lowercase();
    entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| search.is_empty() || entry.search_blob.contains(&search))
        .map(|(index, _)| index)
        .collect()
}

pub fn refresh_space_catalog_results(
    catalog: Res<SpaceObjectCatalog>,
    mut catalog_ui: ResMut<SpaceCatalogUiState>,
    mut filtered_results: ResMut<FilteredSpaceCatalogResults>,
    mut log: MessageWriter<LogEvent>,
    mut prev_count: Local<usize>,
) {
    let should_refresh = catalog.is_changed() || catalog_ui.is_changed();
    if !should_refresh {
        return;
    }

    filtered_results.0 = filter_catalog(&catalog.entries, &catalog_ui.search_text);

    let max_page = filtered_results.0.len().saturating_sub(1) / CATALOG_PAGE_SIZE;
    catalog_ui.page = catalog_ui.page.min(max_page);

    if catalog_ui
        .selected_index
        .is_some_and(|selected_index| selected_index >= catalog.entries.len())
    {
        catalog_ui.selected_index = None;
    }

    let count = filtered_results.0.len();
    if count != *prev_count {
        log.write(LogEvent {
            level: LogLevel::Debug,
            source: "propagation",
            message: format!("Catalog filter: {count} results"),
        });
        *prev_count = count;
    }
}

fn soft_button<'a>(ui: &egui::Ui, text: &str) -> egui::Button<'a> {
    egui::Button::new(egui::RichText::new(text).size(12.0)).fill(ui.visuals().faint_bg_color)
}

/// The panel's contents. Returns the object a click on a row selected (rc.3 logs it).
pub fn catalog_panel_ui(
    ui: &mut egui::Ui,
    state: &mut SpaceCatalogUiState,
    entries: &[SpaceCatalogEntry],
    filtered: &[usize],
) -> Option<usize> {
    let mut selected = None;
    ui.spacing_mut().item_spacing.y = 8.0;
    ui.label(egui::RichText::new("Space Catalog").size(17.0));

    let search = ui.add(
        egui::TextEdit::singleline(&mut state.search_text)
            .hint_text("Search by name or catalog id")
            .desired_width(f32::INFINITY)
            .margin(egui::vec2(10.0, 8.0)),
    );
    if search.changed() {
        state.page = 0;
    }
    // Enter selects the first result on the page and unfocuses, as rc.3 did;
    // Esc only unfocuses (egui's own).
    if search.lost_focus()
        && ui.input(|i| i.key_pressed(egui::Key::Enter))
        && let Some(&entry_index) = filtered.get(state.page * CATALOG_PAGE_SIZE)
    {
        state.selected_index = Some(entry_index);
    }

    let points = if state.show_points {
        "Hide Data Points"
    } else {
        "Show Data Points"
    };
    let size = egui::vec2(ui.available_width(), 34.0);
    if ui.add_sized(size, soft_button(ui, points)).clicked() {
        state.show_points = !state.show_points;
    }

    let page_count = filtered.len().div_ceil(CATALOG_PAGE_SIZE);
    let start = (state.page * CATALOG_PAGE_SIZE).min(filtered.len());
    let end = (start + CATALOG_PAGE_SIZE).min(filtered.len());
    let summary = if entries.is_empty() {
        "Loading catalog...".to_string()
    } else {
        format!(
            "{} objects loaded · showing {}-{} of {}",
            entries.len(),
            if filtered.is_empty() { 0 } else { start + 1 },
            end,
            filtered.len()
        )
    };
    ui.weak(egui::RichText::new(summary).size(11.0));

    let pager_height = 40.0;
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .max_height((ui.available_height() - pager_height).max(pager_height))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            for &entry_index in &filtered[start..end] {
                let row_selected = state.selected_index == Some(entry_index);
                let fill = if row_selected {
                    ui.visuals().widgets.inactive.bg_fill
                } else {
                    ui.visuals().faint_bg_color
                };
                let entry = &entries[entry_index];
                let row = egui::Button::new(egui::RichText::new(entry.display_label()).size(11.0))
                    .fill(fill)
                    .truncate();
                let size = egui::vec2(ui.available_width(), 28.0);
                if ui.add_sized(size, row).clicked() {
                    if row_selected {
                        state.selected_index = None;
                    } else {
                        state.selected_index = Some(entry_index);
                        selected = Some(entry_index);
                    }
                }
            }
        });

    ui.horizontal(|ui| {
        if ui.add(soft_button(ui, "Prev")).clicked() {
            state.page = state.page.saturating_sub(1);
        }
        let page = if page_count == 0 {
            "Page 0 / 0".to_string()
        } else {
            format!("Page {} / {}", state.page + 1, page_count)
        };
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.add(soft_button(ui, "Next")).clicked() {
                state.page = (state.page + 1).min(page_count.saturating_sub(1));
            }
            ui.centered_and_justified(|ui| {
                ui.label(egui::RichText::new(page).size(12.0));
            });
        });
    });
    selected
}

fn catalog_toggle(ui: &mut egui::Ui, state: &mut SpaceCatalogUiState) {
    let text = if state.show_catalog {
        "Hide Catalog"
    } else {
        "Show Catalog"
    };
    let toggle =
        egui::Button::new(egui::RichText::new(text).size(13.0)).fill(ui.visuals().panel_fill);
    if ui.add_sized([156.0, 38.0], toggle).clicked() {
        state.show_catalog = !state.show_catalog;
    }
}

/// "3D View", the catalog toggle (map view only) and, when open, the panel.
pub fn space_catalog_panel(
    mut contexts: EguiContexts,
    camera: Query<&RenderLayers, With<Camera3d>>,
    catalog: Res<SpaceObjectCatalog>,
    filtered: Res<FilteredSpaceCatalogResults>,
    mut catalog_ui: ResMut<SpaceCatalogUiState>,
    mut log: MessageWriter<LogEvent>,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    let map_visible = camera
        .iter()
        .any(|layers| layers.intersects(&RenderLayers::layer(MAP_LAYER)));
    // A copy, so a frame with no change does not refilter the catalog.
    let mut state = catalog_ui.clone();
    let mut selected = None;
    if map_visible && state.show_catalog {
        egui::SidePanel::left("space_catalog")
            .resizable(false)
            .exact_width(CATALOG_WIDTH + 2.0 * CATALOG_MARGIN)
            .frame(egui::Frame::new().inner_margin(CATALOG_MARGIN))
            .show(ctx, |ui| {
                ui.weak(egui::RichText::new("3D View").size(14.0));
                catalog_toggle(ui, &mut state);
                egui::Frame::new()
                    .fill(ui.visuals().panel_fill)
                    .inner_margin(CATALOG_MARGIN)
                    .show(ui, |ui| {
                        selected = catalog_panel_ui(ui, &mut state, &catalog.entries, &filtered.0);
                    });
            });
    } else {
        let corner = ctx.available_rect().left_top() + egui::Vec2::splat(CATALOG_MARGIN);
        egui::Area::new(egui::Id::new("sim_view_corner"))
            .fixed_pos(corner)
            .show(ctx, |ui| {
                ui.weak(egui::RichText::new("3D View").size(14.0));
                if map_visible {
                    catalog_toggle(ui, &mut state);
                }
            });
    }
    if let Some(entry) = selected.and_then(|index| catalog.entries.get(index)) {
        log.write(LogEvent {
            level: LogLevel::Info,
            source: "propagation",
            message: format!("Space object selected: {}", entry.object_name),
        });
    }
    catalog_ui.set_if_neq(state);
}
