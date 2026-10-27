use super::*;

pub fn reset_space_catalog_ui_state(
    mut catalog_ui: ResMut<SpaceCatalogUiState>,
    mut filtered_results: ResMut<FilteredSpaceCatalogResults>,
) {
    *catalog_ui = SpaceCatalogUiState::default();
    filtered_results.0.clear();
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

    let search = catalog_ui.search_text.trim().to_lowercase();
    filtered_results.0.clear();
    filtered_results.0.reserve(catalog.entries.len());

    for (index, entry) in catalog.entries.iter().enumerate() {
        if search.is_empty() || entry.search_blob.contains(&search) {
            filtered_results.0.push(index);
        }
    }

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

pub fn catalog_keyboard_input(
    mut keyboard_events: MessageReader<KeyboardInput>,
    filtered_results: Res<FilteredSpaceCatalogResults>,
    mut catalog_ui: ResMut<SpaceCatalogUiState>,
) {
    if !catalog_ui.search_focused {
        return;
    }

    let mut search_changed = false;

    for event in keyboard_events.read() {
        if event.state != ButtonState::Pressed || event.repeat {
            continue;
        }

        match event.key_code {
            KeyCode::Backspace => {
                search_changed |= catalog_ui.search_text.pop().is_some();
            }
            KeyCode::Escape => {
                catalog_ui.search_focused = false;
            }
            KeyCode::Enter | KeyCode::NumpadEnter => {
                let page_start = catalog_ui.page * CATALOG_PAGE_SIZE;
                if let Some(&entry_index) = filtered_results.0.get(page_start) {
                    catalog_ui.selected_index = Some(entry_index);
                    catalog_ui.search_focused = false;
                }
            }
            _ => {}
        }

        if let Some(text) = event.text.as_deref() {
            for character in text.chars().filter(|character| !character.is_control()) {
                catalog_ui.search_text.push(character);
                search_changed = true;
            }
        }
    }

    if search_changed {
        catalog_ui.page = 0;
    }
}

pub fn catalog_interactions(
    mut interactions: Query<
        (
            &Interaction,
            Option<&CatalogToggleButton>,
            Option<&CatalogSearchField>,
            Option<&CatalogPointsToggleButton>,
            Option<&SatelliteIndicatorToggleButton>,
            Option<&CatalogPrevPageButton>,
            Option<&CatalogNextPageButton>,
            Option<&CatalogResultButton>,
            &mut BackgroundColor,
        ),
        (
            Changed<Interaction>,
            With<Button>,
            Or<(
                With<CatalogToggleButton>,
                With<CatalogSearchField>,
                With<CatalogPointsToggleButton>,
                With<SatelliteIndicatorToggleButton>,
                With<CatalogPrevPageButton>,
                With<CatalogNextPageButton>,
                With<CatalogResultButton>,
            )>,
        ),
    >,
    catalog: Res<SpaceObjectCatalog>,
    filtered_results: Res<FilteredSpaceCatalogResults>,
    mut catalog_ui: ResMut<SpaceCatalogUiState>,
    theme: Res<UiTheme>,
    mut log: MessageWriter<LogEvent>,
) {
    for (
        interaction,
        toggle_button,
        search_field,
        points_toggle,
        satellite_indicator_toggle,
        prev_page,
        next_page,
        result_button,
        mut background_color,
    ) in &mut interactions
    {
        match *interaction {
            Interaction::Pressed => {
                *background_color = BackgroundColor(theme.button_background_hover);

                if search_field.is_some() {
                    catalog_ui.search_focused = true;
                    continue;
                }

                catalog_ui.search_focused = false;

                if toggle_button.is_some() {
                    catalog_ui.show_catalog = !catalog_ui.show_catalog;
                } else if points_toggle.is_some() {
                    catalog_ui.show_points = !catalog_ui.show_points;
                } else if satellite_indicator_toggle.is_some() {
                    catalog_ui.show_satellite_indicator = !catalog_ui.show_satellite_indicator;
                } else if prev_page.is_some() {
                    catalog_ui.page = catalog_ui.page.saturating_sub(1);
                } else if next_page.is_some() {
                    let max_page = filtered_results.0.len().saturating_sub(1) / CATALOG_PAGE_SIZE;
                    catalog_ui.page = (catalog_ui.page + 1).min(max_page);
                } else if let Some(result_button) = result_button
                    && let Some(entry_index) = result_button.entry_index
                {
                    if catalog_ui.selected_index == Some(entry_index) {
                        catalog_ui.selected_index = None;
                    } else {
                        catalog_ui.selected_index = Some(entry_index);
                        if let Some(entry) = catalog.entries.get(entry_index) {
                            log.write(LogEvent {
                                level: LogLevel::Info,
                                source: "propagation",
                                message: format!("Space object selected: {}", entry.object_name),
                            });
                        }
                    }
                }
            }
            Interaction::Hovered => {
                *background_color = BackgroundColor(theme.button_background);
            }
            Interaction::None => {
                let base_color = if search_field.is_some() && catalog_ui.search_focused {
                    theme.button_background
                } else if let Some(result_button) = result_button {
                    if result_button.entry_index == catalog_ui.selected_index {
                        theme.button_background
                    } else {
                        theme.panel_background_soft
                    }
                } else {
                    theme.panel_background_soft
                };

                *background_color = BackgroundColor(base_color);
            }
        }
    }
}

pub fn sync_space_catalog_ui(
    camera: Single<&RenderLayers, With<Camera3d>>,
    catalog: Res<SpaceObjectCatalog>,
    filtered_results: Res<FilteredSpaceCatalogResults>,
    catalog_ui: Res<SpaceCatalogUiState>,
    theme: Res<UiTheme>,
    mut text_queries: ParamSet<(
        Query<&mut Text>,
        Query<&mut Text, With<CatalogResultsSummary>>,
        Query<&mut Text, With<CatalogPageLabel>>,
        Query<(&mut Text, &CatalogResultText)>,
    )>,
    mut text_colors: Query<&mut TextColor>,
    mut node_queries: ParamSet<(
        Query<(&mut Node, &Children), With<CatalogToggleButton>>,
        Query<&mut Node, (With<CatalogPanel>, Without<CatalogToggleButton>)>,
        Query<
            (
                &mut Node,
                &mut CatalogResultButton,
                &Interaction,
                &mut BackgroundColor,
            ),
            With<CatalogResultButton>,
        >,
    )>,
    search_texts: Query<Entity, With<CatalogSearchText>>,
    point_toggle_buttons: Query<&Children, With<CatalogPointsToggleButton>>,
    satellite_indicator_toggle_buttons: Query<&Children, With<SatelliteIndicatorToggleButton>>,
) {
    let map_visible = camera.intersects(&RenderLayers::layer(MAP_LAYER));

    for (mut node, children) in &mut node_queries.p0() {
        node.display = if map_visible {
            Display::Flex
        } else {
            Display::None
        };

        {
            let mut texts = text_queries.p0();
            for child in children.iter() {
                if let Ok(mut text) = texts.get_mut(child) {
                    text.0 = if catalog_ui.show_catalog {
                        "Hide Catalog".to_string()
                    } else {
                        "Show Catalog".to_string()
                    };
                }
            }
        }
    }

    for mut node in &mut node_queries.p1() {
        node.display = if map_visible && catalog_ui.show_catalog {
            Display::Flex
        } else {
            Display::None
        };
    }

    for entity in &search_texts {
        {
            let mut texts = text_queries.p0();
            if let Ok(mut text) = texts.get_mut(entity) {
                let is_empty = catalog_ui.search_text.is_empty();
                text.0 = if is_empty {
                    "Search by name or catalog id".to_string()
                } else {
                    catalog_ui.search_text.clone()
                };
            }
        }

        if let Ok(mut color) = text_colors.get_mut(entity) {
            color.0 = if catalog_ui.search_text.is_empty() {
                theme.text_muted
            } else {
                theme.text_primary
            };
        }
    }

    let page_count = filtered_results
        .0
        .len()
        .saturating_add(CATALOG_PAGE_SIZE - 1)
        / CATALOG_PAGE_SIZE;
    let page_number = if page_count == 0 {
        0
    } else {
        catalog_ui.page + 1
    };
    let page_start = if filtered_results.0.is_empty() {
        0
    } else {
        catalog_ui.page * CATALOG_PAGE_SIZE + 1
    };
    let page_end =
        (catalog_ui.page * CATALOG_PAGE_SIZE + CATALOG_PAGE_SIZE).min(filtered_results.0.len());

    for mut text in &mut text_queries.p1() {
        text.0 = format!(
            "{} objects loaded · showing {}-{} of {}",
            catalog.entries.len(),
            page_start,
            page_end,
            filtered_results.0.len()
        );
    }

    for mut text in &mut text_queries.p2() {
        text.0 = if page_count == 0 {
            "Page 0 / 0".to_string()
        } else {
            format!("Page {} / {}", page_number, page_count)
        };
    }

    for children in &point_toggle_buttons {
        {
            let mut texts = text_queries.p0();
            for child in children.iter() {
                if let Ok(mut text) = texts.get_mut(child) {
                    text.0 = if catalog_ui.show_points {
                        "Hide Data Points".to_string()
                    } else {
                        "Show Data Points".to_string()
                    };
                }
            }
        }
    }

    for children in &satellite_indicator_toggle_buttons {
        let mut texts = text_queries.p0();
        for child in children.iter() {
            if let Ok(mut text) = texts.get_mut(child) {
                text.0 = if catalog_ui.show_satellite_indicator {
                    "Hide Satellite Indicator".to_string()
                } else {
                    "Show Satellite Indicator".to_string()
                };
            }
        }
    }

    for (mut node, mut row, interaction, mut background_color) in &mut node_queries.p2() {
        let slot_index = catalog_ui.page * CATALOG_PAGE_SIZE + row.slot;

        if let Some(&entry_index) = filtered_results.0.get(slot_index) {
            row.entry_index = Some(entry_index);
            node.display = Display::Flex;

            let entry = &catalog.entries[entry_index];
            let base_color = if *interaction == Interaction::Hovered
                || Some(entry_index) == catalog_ui.selected_index
            {
                theme.button_background
            } else {
                theme.panel_background_soft
            };
            *background_color = BackgroundColor(base_color);

            for (mut text, label) in &mut text_queries.p3() {
                if label.slot == row.slot {
                    text.0 = entry.display_label();
                }
            }
        } else {
            row.entry_index = None;
            node.display = Display::None;

            for (mut text, label) in &mut text_queries.p3() {
                if label.slot == row.slot {
                    text.0.clear();
                }
            }
        }
    }
}
