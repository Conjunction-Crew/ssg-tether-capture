use super::spawn::{
    clamped_slider_value_from_elements, format_orbital_element_value, orbital_element_slider_spec,
    orbital_selection_role_label, orbital_selection_source_label, selected_orbital_object,
    slider_fraction,
};
use super::*;
use ssg_sim::time::SimConfig;

pub fn sync_orbital_selection_ui(
    mut commands: Commands,
    selection: Option<Res<OrbitalSelectionState>>,
    catalog: Res<SpaceObjectCatalog>,
    catalog_ui: Res<SpaceCatalogUiState>,
    mut sliders: Query<(Entity, &OrbitalElementSlider, &SliderValue)>,
    mut text_queries: ParamSet<(
        Query<&mut Text, With<CurrentCatalogSelectionText>>,
        Query<(&mut Text, &OrbitalSelectionSummaryText)>,
        Query<(&mut Text, &OrbitalElementSliderValueText)>,
    )>,
    mut node_queries: ParamSet<(
        Query<(&mut Node, &OrbitalSelectionSliderGroup)>,
        Query<(&mut Node, &OrbitalElementSliderFill)>,
        Query<(&mut Node, &OrbitalElementSliderThumb)>,
    )>,
) {
    let selection = selection.as_deref();
    let catalog_label = catalog_ui
        .selected_index
        .and_then(|index| catalog.entries.get(index))
        .map(|entry| format!("Selected catalog object: {}", entry.display_label()))
        .unwrap_or_else(|| "Selected catalog object: none".to_string());

    for mut text in &mut text_queries.p0() {
        text.0 = catalog_label.clone();
    }

    for (mut text, summary) in &mut text_queries.p1() {
        text.0 = selected_orbital_object(selection, summary.role)
            .map(|selected| {
                format!(
                    "{}: {}",
                    orbital_selection_role_label(summary.role),
                    orbital_selection_source_label(selected)
                )
            })
            .unwrap_or_else(|| {
                format!(
                    "{}: No orbit selected",
                    orbital_selection_role_label(summary.role)
                )
            });
    }

    for (mut node, group) in &mut node_queries.p0() {
        node.display = if selected_orbital_object(selection, group.role).is_some() {
            Display::Flex
        } else {
            Display::None
        };
    }

    for (entity, slider, value) in &mut sliders {
        let next_value = selected_orbital_object(selection, slider.role)
            .map(|selected| clamped_slider_value_from_elements(&selected.elements, slider.field))
            .unwrap_or_else(|| orbital_element_slider_spec(slider.field).default_value);

        if (value.0 - next_value).abs() > f32::EPSILON {
            commands.entity(entity).insert(SliderValue(next_value));
        }
    }

    for (mut text, value_text) in &mut text_queries.p2() {
        text.0 = selected_orbital_object(selection, value_text.role)
            .map(|selected| format_orbital_element_value(&selected.elements, value_text.field))
            .unwrap_or_else(|| "--".to_string());
    }

    for (mut node, fill) in &mut node_queries.p1() {
        let width = selected_orbital_object(selection, fill.role)
            .map(|selected| slider_fraction(&selected.elements, fill.field) * 100.0)
            .unwrap_or(0.0);
        node.width = percent(width);
    }

    for (mut node, thumb) in &mut node_queries.p2() {
        let left = selected_orbital_object(selection, thumb.role)
            .map(|selected| slider_fraction(&selected.elements, thumb.field) * 100.0)
            .unwrap_or(0.0);
        node.left = percent(left);
    }
}

pub fn orbital_selection_interactions(
    mut buttons: Query<
        (
            &Interaction,
            Option<&UseCatalogSelectionButton>,
            Option<&CreateCustomOrbitButton>,
            &mut BackgroundColor,
        ),
        (
            Changed<Interaction>,
            With<Button>,
            Or<(
                With<UseCatalogSelectionButton>,
                With<CreateCustomOrbitButton>,
            )>,
        ),
    >,
    catalog_ui: Res<SpaceCatalogUiState>,
    catalog: Res<SpaceObjectCatalog>,
    sim_config: Res<SimConfig>,
    mut selection: ResMut<OrbitalSelectionState>,
    theme: Res<UiTheme>,
) {
    for (interaction, catalog_selection, custom_orbit, mut background_color) in &mut buttons {
        match *interaction {
            Interaction::Pressed => {
                *background_color = BackgroundColor(theme.button_background_hover);

                if let Some(button) = catalog_selection {
                    let Some(catalog_index) = catalog_ui.selected_index else {
                        continue;
                    };
                    let Some(entry) = catalog.entries.get(catalog_index) else {
                        continue;
                    };
                    let selected = SelectedOrbitalObject {
                        source: OrbitalSelectionSource::Catalog {
                            catalog_index,
                            gpu_index: entry.gpu_index,
                            label: entry.display_label(),
                        },
                        elements: EditableOrbitalElements::from_gp(&entry.gp),
                        gp: Some(entry.gp.clone()),
                        mass_kg: None,
                        inertia_kg_m2: None,
                    };

                    match button.role {
                        OrbitalSelectionRole::Target => selection.target = Some(selected),
                        OrbitalSelectionRole::Chaser => selection.chaser = Some(selected),
                    }
                } else if let Some(button) = custom_orbit {
                    let label = match button.role {
                        OrbitalSelectionRole::Target => "Custom Target",
                        OrbitalSelectionRole::Chaser => "Custom Chaser",
                    };
                    let selected = SelectedOrbitalObject {
                        source: OrbitalSelectionSource::Custom {
                            label: label.to_string(),
                        },
                        elements: EditableOrbitalElements {
                            semi_major_axis_m: ISS_ORBIT[0],
                            eccentricity: ISS_ORBIT[1],
                            inclination_rad: ISS_ORBIT[2],
                            raan_rad: ISS_ORBIT[3],
                            arg_perigee_rad: ISS_ORBIT[4],
                            mean_anomaly_rad: ISS_ORBIT[5],
                            epoch: sim_config.start_epoch,
                        },
                        gp: None,
                        mass_kg: None,
                        inertia_kg_m2: None,
                    };

                    match button.role {
                        OrbitalSelectionRole::Target => selection.target = Some(selected),
                        OrbitalSelectionRole::Chaser => selection.chaser = Some(selected),
                    }
                }
            }
            Interaction::Hovered => {
                *background_color = BackgroundColor(theme.button_background);
            }
            Interaction::None => {
                *background_color = BackgroundColor(theme.panel_background_soft);
            }
        }
    }
}
