use bevy::camera::visibility::RenderLayers;
use bevy::input_focus::tab_navigation::TabIndex;
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui_widgets::{
    Slider, SliderRange, SliderThumb, SliderValue, TrackClick, ValueChange, observe,
    slider_self_update,
};

use crate::components::user_interface::OrbitLabel;
use crate::constants::UI_LAYER;
use crate::resources::capture_plans::RadiusSliderResource;
use crate::resources::orbital_entities::OrbitalEntities;
use crate::ui::events::UiEvent;
use crate::ui::state::{ProjectCatalog, SelectedProject};
use crate::ui::theme::UiTheme;
use crate::ui::widgets::ScreenRoot;

#[derive(Component)]
pub struct ProjectDetailScreen;

#[derive(Component)]
pub struct BackButton;

#[derive(Component)]
pub struct CaptureButton {
    pub entity: Option<Entity>,
}

#[derive(Component, Default)]
pub struct RadiusSlider;

#[derive(Component, Default)]
pub struct RadiusSliderThumb;

#[derive(Component)]
pub struct MapViewButton;

#[derive(Component)]
pub struct TimeWarpIncreaseButton;

#[derive(Component)]
pub struct TimeWarpDecreaseButton;

#[derive(Component)]
pub struct ToggleOriginButton;

#[derive(Component)]
pub struct TimeWarpLabel;

#[derive(Component, Clone, PartialEq, Eq)]
pub enum CollapsibleSection {
    SimulationHud,
    NearbyDebris,
}

#[derive(Component)]
pub struct CollapsibleToggle {
    pub section: CollapsibleSection,
}

#[derive(Component)]
pub struct CollapsibleContent {
    pub section: CollapsibleSection,
}

#[derive(Component)]
pub struct TelemetryValue {
    pub entity: Option<Entity>,
    pub field_index: usize,
}

pub fn spawn_project_detail_screen(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    theme: Res<UiTheme>,
    selected_project: Res<SelectedProject>,
    catalog: Res<ProjectCatalog>,
    orbital_entities: Res<OrbitalEntities>,
) {
    let font = asset_server.load("fonts/FiraMono-Medium.ttf");

    let selected = selected_project.project_id.as_ref().and_then(|project_id| {
        catalog
            .projects
            .iter()
            .find(|project| &project.id == project_id)
    });

    let project_title = selected
        .map(|project| project.title.clone())
        .unwrap_or_else(|| "No project selected".to_string());

    let project_description = selected
        .map(|project| project.description.clone())
        .unwrap_or_else(|| "Return to Home and choose a project.".to_string());

    let project_directory = selected
        .map(|project| project.working_directory.clone())
        .unwrap_or_else(|| "Unknown directory".to_string());

    let project_file = selected
        .map(|project| project.file_name.clone())
        .unwrap_or_else(|| "Unknown file".to_string());

    let tether_entity = selected
        .and_then(|project| orbital_entities.tethers.get(&project.tether_id))
        .expect("Failed to get tether entity");

    commands
        .spawn((
            ProjectDetailScreen,
            ScreenRoot,
            RenderLayers::layer(UI_LAYER),
            Node {
                width: percent(100),
                height: percent(100),
                flex_direction: FlexDirection::Column,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.0)),
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    width: percent(100),
                    min_height: px(72.0),
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::SpaceBetween,
                    padding: UiRect::axes(px(18.0), px(14.0)),
                    ..default()
                },
                BackgroundColor(theme.header_background),
            ))
            .with_children(|header| {
                header
                    .spawn((
                        Button,
                        BackButton,
                        Node {
                            min_width: px(120.0),
                            min_height: px(40.0),
                            align_items: AlignItems::Center,
                            justify_content: JustifyContent::Center,
                            ..default()
                        },
                        BackgroundColor(theme.panel_background_soft),
                    ))
                    .with_children(|button| {
                        button.spawn((
                            Text::new("Back"),
                            TextFont {
                                font: font.clone(),
                                font_size: 14.0,
                                ..default()
                            },
                            TextColor(theme.text_primary),
                        ));
                    });

                header.spawn((
                    Text::new(project_title),
                    TextFont {
                        font: font.clone(),
                        font_size: 24.0,
                        ..default()
                    },
                    TextColor(theme.text_primary),
                ));

                header.spawn((
                    Text::new("Project View"),
                    TextFont {
                        font: font.clone(),
                        font_size: 12.0,
                        ..default()
                    },
                    TextColor(theme.text_muted),
                ));
            });

            root.spawn(Node {
                width: percent(100),
                height: percent(100),
                flex_direction: FlexDirection::Row,
                ..default()
            })
            .with_children(|content| {
                content
                    .spawn(Node {
                        width: percent(100),
                        flex_grow: 1.0,
                        ..default()
                    })
                    .with_children(|left| {
                        left.spawn((
                            Text::new("3D View"),
                            TextFont {
                                font: font.clone(),
                                font_size: 14.0,
                                ..default()
                            },
                            TextColor(theme.text_muted),
                            Node {
                                position_type: PositionType::Absolute,
                                left: px(12.0),
                                top: px(12.0),
                                ..default()
                            },
                        ));
                    });

                content
                    .spawn((
                        Node {
                            width: px(420.0),
                            max_width: percent(42.0),
                            height: percent(100),
                            flex_direction: FlexDirection::Column,
                            row_gap: px(10.0),
                            padding: UiRect::all(px(12.0)),
                            overflow: Overflow::scroll_y(),
                            ..default()
                        },
                        BackgroundColor(theme.panel_background_soft),
                    ))
                    .with_children(|sidebar| {
                        // Project Information
                        sidebar
                            .spawn((
                                Node {
                                    width: percent(100),
                                    flex_direction: FlexDirection::Column,
                                    row_gap: px(8.0),
                                    padding: UiRect::all(px(12.0)),
                                    ..default()
                                },
                                BackgroundColor(theme.panel_background),
                            ))
                            .with_children(|info| {
                                info.spawn((
                                    Text::new("Project Information"),
                                    TextFont {
                                        font: font.clone(),
                                        font_size: 17.0,
                                        ..default()
                                    },
                                    TextColor(theme.text_primary),
                                ));

                                info.spawn((
                                    Text::new(project_description),
                                    TextFont {
                                        font: font.clone(),
                                        font_size: 12.0,
                                        ..default()
                                    },
                                    TextColor(theme.text_muted),
                                ));

                                info.spawn((
                                    Text::new(format!("Working Directory: {}", project_directory)),
                                    TextFont {
                                        font: font.clone(),
                                        font_size: 11.0,
                                        ..default()
                                    },
                                    TextColor(theme.text_primary),
                                ));

                                info.spawn((
                                    Text::new(format!("Main File: {}", project_file)),
                                    TextFont {
                                        font: font.clone(),
                                        font_size: 11.0,
                                        ..default()
                                    },
                                    TextColor(theme.text_primary),
                                ));
                            });

                        // Simulation Controls
                        sidebar
                            .spawn((
                                Node {
                                    width: percent(100),
                                    flex_direction: FlexDirection::Column,
                                    row_gap: px(8.0),
                                    padding: UiRect::all(px(12.0)),
                                    ..default()
                                },
                                BackgroundColor(theme.panel_background),
                            ))
                            .with_children(|controls| {
                                controls.spawn((
                                    Text::new("Simulation Controls"),
                                    TextFont {
                                        font: font.clone(),
                                        font_size: 17.0,
                                        ..default()
                                    },
                                    TextColor(theme.text_primary),
                                ));

                                // Map View button
                                controls
                                    .spawn((
                                        Button,
                                        MapViewButton,
                                        Node {
                                            width: percent(100),
                                            min_height: px(40.0),
                                            align_items: AlignItems::Center,
                                            justify_content: JustifyContent::Center,
                                            ..default()
                                        },
                                        BackgroundColor(theme.panel_background_soft),
                                    ))
                                    .with_children(|button| {
                                        button.spawn((
                                            Text::new("Map View (M)"),
                                            TextFont {
                                                font: font.clone(),
                                                font_size: 14.0,
                                                ..default()
                                            },
                                            TextColor(theme.text_primary),
                                        ));
                                    });

                                // Time Warp controls row
                                controls
                                    .spawn(Node {
                                        width: percent(100),
                                        flex_direction: FlexDirection::Row,
                                        align_items: AlignItems::Center,
                                        justify_content: JustifyContent::SpaceBetween,
                                        column_gap: px(6.0),
                                        ..default()
                                    })
                                    .with_children(|row| {
                                        // Decrease button
                                        row.spawn((
                                            Button,
                                            TimeWarpDecreaseButton,
                                            Node {
                                                min_width: px(40.0),
                                                min_height: px(40.0),
                                                align_items: AlignItems::Center,
                                                justify_content: JustifyContent::Center,
                                                ..default()
                                            },
                                            BackgroundColor(theme.panel_background_soft),
                                        ))
                                        .with_children(|button| {
                                            button.spawn((
                                                Text::new("<"),
                                                TextFont {
                                                    font: font.clone(),
                                                    font_size: 14.0,
                                                    ..default()
                                                },
                                                TextColor(theme.text_primary),
                                            ));
                                        });

                                        // Time warp label
                                        row.spawn((
                                            TimeWarpLabel,
                                            Text::new("1x"),
                                            TextFont {
                                                font: font.clone(),
                                                font_size: 14.0,
                                                ..default()
                                            },
                                            TextColor(theme.text_accent),
                                        ));

                                        // Increase button
                                        row.spawn((
                                            Button,
                                            TimeWarpIncreaseButton,
                                            Node {
                                                min_width: px(40.0),
                                                min_height: px(40.0),
                                                align_items: AlignItems::Center,
                                                justify_content: JustifyContent::Center,
                                                ..default()
                                            },
                                            BackgroundColor(theme.panel_background_soft),
                                        ))
                                        .with_children(|button| {
                                            button.spawn((
                                                Text::new(">"),
                                                TextFont {
                                                    font: font.clone(),
                                                    font_size: 14.0,
                                                    ..default()
                                                },
                                                TextColor(theme.text_primary),
                                            ));
                                        });
                                    });

                                // Toggle Origin button
                                controls
                                    .spawn((
                                        Button,
                                        ToggleOriginButton,
                                        Node {
                                            width: percent(100),
                                            min_height: px(40.0),
                                            align_items: AlignItems::Center,
                                            justify_content: JustifyContent::Center,
                                            ..default()
                                        },
                                        BackgroundColor(theme.panel_background_soft),
                                    ))
                                    .with_children(|button| {
                                        button.spawn((
                                            Text::new("Toggle Origin (O)"),
                                            TextFont {
                                                font: font.clone(),
                                                font_size: 14.0,
                                                ..default()
                                            },
                                            TextColor(theme.text_primary),
                                        ));
                                    });
                            });

                        // Simulation HUD
                        sidebar
                            .spawn((
                                Node {
                                    width: percent(100),
                                    flex_direction: FlexDirection::Column,
                                    row_gap: px(8.0),
                                    padding: UiRect::all(px(12.0)),
                                    ..default()
                                },
                                BackgroundColor(theme.panel_background),
                            ))
                            .with_children(|hud| {
                                hud.spawn(Node {
                                    width: percent(100),
                                    flex_direction: FlexDirection::Row,
                                    justify_content: JustifyContent::SpaceBetween,
                                    align_items: AlignItems::Center,
                                    ..default()
                                })
                                .with_children(|header_row| {
                                    header_row.spawn((
                                        Text::new("Simulation HUD"),
                                        TextFont {
                                            font: font.clone(),
                                            font_size: 17.0,
                                            ..default()
                                        },
                                        TextColor(theme.text_primary),
                                    ));
                                    header_row
                                        .spawn((
                                            Button,
                                            CollapsibleToggle {
                                                section: CollapsibleSection::SimulationHud,
                                            },
                                            Node {
                                                min_width: px(30.0),
                                                min_height: px(30.0),
                                                align_items: AlignItems::Center,
                                                justify_content: JustifyContent::Center,
                                                ..default()
                                            },
                                            BackgroundColor(theme.panel_background_soft),
                                        ))
                                        .with_children(|btn| {
                                            btn.spawn((
                                                Text::new("▼"),
                                                TextFont {
                                                    font: font.clone(),
                                                    font_size: 14.0,
                                                    ..default()
                                                },
                                                TextColor(theme.text_muted),
                                            ));
                                        });
                                });

                                hud.spawn((
                                    CollapsibleContent {
                                        section: CollapsibleSection::SimulationHud,
                                    },
                                    Node {
                                        width: percent(100),
                                        flex_direction: FlexDirection::Column,
                                        row_gap: px(4.0),
                                        ..default()
                                    },
                                ))
                                .with_children(|content| {
                                    let entity = tether_entity.get(0).cloned();
                                    let labels = [
                                        "Velocity",
                                        "Semi-major axis",
                                        "Eccentricity",
                                        "Inclination",
                                        "RAAN",
                                        "Arg of periapsis",
                                        "True anomaly",
                                        "Height",
                                    ];
                                    for (i, label) in labels.iter().enumerate() {
                                        content
                                            .spawn(Node {
                                                width: percent(100),
                                                flex_direction: FlexDirection::Row,
                                                justify_content: JustifyContent::SpaceBetween,
                                                column_gap: px(8.0),
                                                ..default()
                                            })
                                            .with_children(|row| {
                                                row.spawn((
                                                    Text::new(format!("{}:", label)),
                                                    TextFont {
                                                        font: font.clone(),
                                                        font_size: 12.0,
                                                        ..default()
                                                    },
                                                    TextColor(theme.text_accent),
                                                ));
                                                row.spawn((
                                                    TelemetryValue {
                                                        entity,
                                                        field_index: i,
                                                    },
                                                    Text::new("--"),
                                                    TextFont {
                                                        font: font.clone(),
                                                        font_size: 12.0,
                                                        ..default()
                                                    },
                                                    TextColor(theme.text_primary),
                                                ));
                                            });
                                    }
                                });
                            });

                        // Nearby Debris
                        sidebar
                            .spawn((
                                Node {
                                    width: percent(100),
                                    flex_direction: FlexDirection::Column,
                                    row_gap: px(8.0),
                                    padding: UiRect::all(px(12.0)),
                                    ..default()
                                },
                                BackgroundColor(theme.panel_background),
                            ))
                            .with_children(|debris_section| {
                                debris_section
                                    .spawn(Node {
                                        width: percent(100),
                                        flex_direction: FlexDirection::Row,
                                        justify_content: JustifyContent::SpaceBetween,
                                        align_items: AlignItems::Center,
                                        ..default()
                                    })
                                    .with_children(|header_row| {
                                        header_row.spawn((
                                            Text::new("Nearby Debris"),
                                            TextFont {
                                                font: font.clone(),
                                                font_size: 17.0,
                                                ..default()
                                            },
                                            TextColor(theme.text_primary),
                                        ));
                                        header_row
                                            .spawn((
                                                Button,
                                                CollapsibleToggle {
                                                    section: CollapsibleSection::NearbyDebris,
                                                },
                                                Node {
                                                    min_width: px(30.0),
                                                    min_height: px(30.0),
                                                    align_items: AlignItems::Center,
                                                    justify_content: JustifyContent::Center,
                                                    ..default()
                                                },
                                                BackgroundColor(theme.panel_background_soft),
                                            ))
                                            .with_children(|btn| {
                                                btn.spawn((
                                                    Text::new("▼"),
                                                    TextFont {
                                                        font: font.clone(),
                                                        font_size: 14.0,
                                                        ..default()
                                                    },
                                                    TextColor(theme.text_muted),
                                                ));
                                            });
                                    });

                                debris_section
                                    .spawn((
                                        CollapsibleContent {
                                            section: CollapsibleSection::NearbyDebris,
                                        },
                                        Node {
                                            width: percent(100),
                                            flex_direction: FlexDirection::Column,
                                            row_gap: px(4.0),
                                            ..default()
                                        },
                                    ))
                                    .with_children(|content| {
                                        let entity = orbital_entities
                                            .debris
                                            .get("Satellite1")
                                            .copied();
                                        let labels = [
                                            "Velocity",
                                            "Semi-major axis",
                                            "Eccentricity",
                                            "Inclination",
                                            "RAAN",
                                            "Arg of periapsis",
                                            "True anomaly",
                                            "Height",
                                        ];
                                        for (i, label) in labels.iter().enumerate() {
                                            content
                                                .spawn(Node {
                                                    width: percent(100),
                                                    flex_direction: FlexDirection::Row,
                                                    justify_content: JustifyContent::SpaceBetween,
                                                    column_gap: px(8.0),
                                                    ..default()
                                                })
                                                .with_children(|row| {
                                                    row.spawn((
                                                        Text::new(format!("{}:", label)),
                                                        TextFont {
                                                            font: font.clone(),
                                                            font_size: 12.0,
                                                            ..default()
                                                        },
                                                        TextColor(theme.text_accent),
                                                    ));
                                                    row.spawn((
                                                        TelemetryValue {
                                                            entity,
                                                            field_index: i,
                                                        },
                                                        Text::new("--"),
                                                        TextFont {
                                                            font: font.clone(),
                                                            font_size: 12.0,
                                                            ..default()
                                                        },
                                                        TextColor(theme.text_primary),
                                                    ));
                                                });
                                        }

                                        content
                                            .spawn((
                                                Button,
                                                CaptureButton {
                                                    entity: orbital_entities
                                                        .debris
                                                        .get("Satellite1")
                                                        .copied(),
                                                },
                                                Node {
                                                    min_width: px(120.0),
                                                    min_height: px(40.0),
                                                    align_items: AlignItems::Center,
                                                    justify_content: JustifyContent::Center,
                                                    margin: UiRect::top(px(4.0)),
                                                    ..default()
                                                },
                                                BackgroundColor(theme.panel_background_soft),
                                            ))
                                            .with_children(|button| {
                                                button.spawn((
                                                    Text::new("Capture"),
                                                    TextFont {
                                                        font: font.clone(),
                                                        font_size: 14.0,
                                                        ..default()
                                                    },
                                                    TextColor(theme.text_primary),
                                                ));
                                            });

                                        content
                                            .spawn((
                                                observe(slider_self_update),
                                                observe(
                                                    |value_change: On<ValueChange<f32>>,
                                                     mut widget_states: ResMut<
                                                        RadiusSliderResource,
                                                    >| {
                                                        widget_states.radius =
                                                            value_change.value;
                                                    },
                                                ),
                                                Node {
                                                    display: Display::Flex,
                                                    flex_direction: FlexDirection::Column,
                                                    justify_content: JustifyContent::Center,
                                                    align_items: AlignItems::Stretch,
                                                    justify_items: JustifyItems::Center,
                                                    column_gap: px(4),
                                                    height: px(12),
                                                    width: percent(100),
                                                    ..default()
                                                },
                                                Name::new("Slider"),
                                                Hovered::default(),
                                                RadiusSlider,
                                                Slider {
                                                    track_click: TrackClick::Snap,
                                                },
                                                SliderValue(25.0),
                                                SliderRange::new(0.0, 25.0),
                                                TabIndex(0),
                                            ))
                                            .with_children(|slider| {
                                                // Background rail
                                                slider.spawn((
                                                    Node {
                                                        height: px(6),
                                                        border_radius: BorderRadius::all(px(3)),
                                                        ..default()
                                                    },
                                                    BackgroundColor(theme.panel_background_soft),
                                                ));

                                                slider
                                                    .spawn((Node {
                                                        display: Display::Flex,
                                                        position_type: PositionType::Absolute,
                                                        left: px(0),
                                                        // Track is short by 12px to accommodate the thumb.
                                                        right: px(12),
                                                        top: px(0),
                                                        bottom: px(0),
                                                        ..default()
                                                    },))
                                                    .with_children(|thumb| {
                                                        thumb.spawn((
                                                            // Thumb
                                                            RadiusSliderThumb,
                                                            SliderThumb,
                                                            Node {
                                                                display: Display::Flex,
                                                                width: px(12),
                                                                height: px(12),
                                                                position_type:
                                                                    PositionType::Absolute,
                                                                left: percent(0),
                                                                border_radius: BorderRadius::MAX,
                                                                ..default()
                                                            },
                                                            BackgroundColor(
                                                                theme.panel_background,
                                                            ),
                                                        ));
                                                    });
                                            });
                                    });
                            });
                    });
            });

            root.spawn((
                OrbitLabel {
                    entity: tether_entity.get(0).cloned(),
                },
                Text::new("─ Tether1"),
                TextFont {
                    font,
                    font_size: 14.0,
                    ..default()
                },
                TextColor(theme.text_primary),
                Node {
                    position_type: PositionType::Absolute,
                    ..default()
                },
            ));
        });
}

pub fn cleanup_project_detail_screen(
    mut commands: Commands,
    roots: Query<Entity, With<ProjectDetailScreen>>,
) {
    for entity in &roots {
        commands.entity(entity).despawn();
    }
}

pub fn project_detail_interactions(
    mut interactions: Query<
        (
            &Interaction,
            Option<&BackButton>,
            Option<&CaptureButton>,
            Option<&MapViewButton>,
            Option<&TimeWarpIncreaseButton>,
            Option<&TimeWarpDecreaseButton>,
            Option<&ToggleOriginButton>,
            &mut BackgroundColor,
        ),
        (Changed<Interaction>, With<Button>),
    >,
    mut events: MessageWriter<UiEvent>,
    screen: Res<State<crate::ui::state::UiScreen>>,
    theme: Res<UiTheme>,
) {
    if *screen.get() != crate::ui::state::UiScreen::ProjectDetail {
        return;
    }

    for (interaction, back_button, capture_button, map_view, warp_inc, warp_dec, toggle_origin, mut background_color) in &mut interactions {
        match *interaction {
            Interaction::Pressed => {
                *background_color = BackgroundColor(theme.button_background_hover);
                if back_button.is_some() {
                    events.write(UiEvent::BackToHome);
                } else if let Some(capture_entity) = capture_button {
                    events.write(UiEvent::CaptureDebris(capture_entity.entity));
                } else if map_view.is_some() {
                    events.write(UiEvent::ToggleMapView);
                } else if warp_inc.is_some() {
                    events.write(UiEvent::TimeWarpIncrease);
                } else if warp_dec.is_some() {
                    events.write(UiEvent::TimeWarpDecrease);
                } else if toggle_origin.is_some() {
                    events.write(UiEvent::ToggleOrigin);
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

pub fn collapsible_toggle_interaction(
    toggles: Query<
        (Entity, &Interaction, &CollapsibleToggle),
        (Changed<Interaction>, With<Button>),
    >,
    mut contents: Query<(&mut Node, &CollapsibleContent)>,
    children_query: Query<&Children>,
    mut texts: Query<&mut Text>,
) {
    for (entity, interaction, toggle) in &toggles {
        if *interaction == Interaction::Pressed {
            let mut collapsed = false;
            for (mut node, content) in &mut contents {
                if content.section == toggle.section {
                    if node.display == Display::None {
                        node.display = Display::Flex;
                    } else {
                        node.display = Display::None;
                        collapsed = true;
                    }
                }
            }
            if let Ok(children) = children_query.get(entity) {
                for child in children {
                    if let Ok(mut text) = texts.get_mut(*child) {
                        text.0 = if collapsed {
                            "▶".to_string()
                        } else {
                            "▼".to_string()
                        };
                    }
                }
            }
        }
    }
}
