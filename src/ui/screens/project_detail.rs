use bevy::camera::visibility::RenderLayers;
use bevy::ecs::hierarchy::ChildSpawnerCommands;
use bevy::ecs::observer::On;
use bevy::input::mouse::MouseScrollUnit;
use bevy::picking::events::{Pointer, Scroll};
use bevy::prelude::*;
use bevy::ui_widgets::{ControlOrientation, CoreScrollbarThumb, Scrollbar};

use crate::components::user_interface::{
    CaptureGuidanceReadout, CaptureTelemetryReadout, OrbitLabel, TimeWarpReadout,
};
use crate::constants::UI_LAYER;
use crate::resources::orbital_cache::OrbitalCache;
use crate::ui::events::UiEvent;
use crate::ui::state::{ProjectCatalog, SelectedProject};
use crate::ui::theme::UiTheme;
use crate::ui::widgets::ScreenRoot;

#[derive(Component)]
pub struct SimScreen;

#[derive(Component)]
pub struct BackButton;

#[derive(Component)]
pub struct CaptureButton {
    pub entity: Option<Entity>,
    pub plan_id: String,
}

#[derive(Component, PartialEq, Eq, Clone, Copy)]
pub enum CollapsibleSection {
    ProjectInformation,
    TimeWarp,
    SimulationControls,
    SimulationHud,
    Reference,
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
pub struct MapViewButton;

#[derive(Component)]
pub struct TimeWarpDecreaseButton;

#[derive(Component)]
pub struct TimeWarpLabel;

#[derive(Component)]
pub struct TimeWarpIncreaseButton;

#[derive(Component)]
pub struct ToggleOriginButton;

#[derive(Component)]
pub struct CycleCameraButton;

#[derive(Component)]
pub struct SidebarPanel;

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
    orbital_entities: Res<OrbitalCache>,
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
    let tether_root_entity = tether_entity.get(0).copied();
    let capture_target_entity = orbital_entities.debris.get("Satellite1").copied();
    let capture_target_label = String::from("Satellite1");
    let capture_plan_id = String::from("example_plan");

    commands
        .spawn((
            SimScreen,
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
                flex_grow: 1.0,
                flex_shrink: 1.0,
                min_height: px(0.0),
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
                            flex_grow: 1.0,
                            flex_shrink: 1.0,
                            min_height: px(0.0),
                            flex_direction: FlexDirection::Row,
                            ..default()
                        },
                    ))
                    .with_children(|sidebar_wrapper| {
                        let sidebar_id = sidebar_wrapper
                            .spawn((
                                SidebarPanel,
                                Interaction::default(),
                                ScrollPosition::default(),
                                Node {
                                    width: percent(100.0),
                                    flex_grow: 1.0,
                                    flex_shrink: 1.0,
                                    min_height: px(0.0),
                                    flex_direction: FlexDirection::Column,
                                    row_gap: px(10.0),
                                    padding: UiRect::all(px(12.0)),
                                    overflow: Overflow::scroll_y(),
                                    scrollbar_width: 8.0,
                                    ..default()
                                },
                                BackgroundColor(theme.panel_background_soft),
                            ))
                            .observe(
                                |mut ev: On<Pointer<Scroll>>,
                                 mut query: Query<&mut ScrollPosition>| {
                                    ev.propagate(false);
                                    let scroll_amount = match ev.event.unit {
                                        MouseScrollUnit::Line => ev.event.y * 24.0,
                                        MouseScrollUnit::Pixel => ev.event.y,
                                    };
                                    if let Ok(mut scroll_pos) = query.get_mut(ev.entity) {
                                        scroll_pos.0.y -= scroll_amount;
                                        scroll_pos.0.y = scroll_pos.0.y.max(0.0);
                                    }
                                },
                            )
                            .with_children(|sidebar| {
                        // === Project Information (collapsible) ===
                        spawn_collapsible_section(
                            sidebar,
                            &font,
                            &theme,
                            "Project Information",
                            CollapsibleSection::ProjectInformation,
                            |content| {
                                content.spawn((
                                    Text::new(project_description),
                                    TextFont {
                                        font: font.clone(),
                                        font_size: 12.0,
                                        ..default()
                                    },
                                    TextColor(theme.text_muted),
                                ));

                                content.spawn((
                                    Text::new(format!("Working Directory: {}", project_directory)),
                                    TextFont {
                                        font: font.clone(),
                                        font_size: 11.0,
                                        ..default()
                                    },
                                    TextColor(theme.text_primary),
                                ));

                                content.spawn((
                                    Text::new(format!("Main File: {}", project_file)),
                                    TextFont {
                                        font: font.clone(),
                                        font_size: 11.0,
                                        ..default()
                                    },
                                    TextColor(theme.text_primary),
                                ));
                            },
                        );

                        // === Time Warp (collapsible) ===
                        spawn_collapsible_section(
                            sidebar,
                            &font,
                            &theme,
                            "Time Warp",
                            CollapsibleSection::TimeWarp,
                            |content| {
                                content
                                    .spawn((
                                        Node {
                                            width: percent(100),
                                            flex_direction: FlexDirection::Row,
                                            align_items: AlignItems::Center,
                                            justify_content: JustifyContent::Center,
                                            column_gap: px(6.0),
                                            padding: UiRect::all(px(12.0)),
                                            ..default()
                                        },
                                        BackgroundColor(theme.panel_background_soft),
                                    ))
                                    .with_children(|row| {
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
                                            BackgroundColor(theme.panel_background),
                                        ))
                                        .with_children(|btn| {
                                            btn.spawn((
                                                Text::new("<"),
                                                TextFont {
                                                    font: font.clone(),
                                                    font_size: 14.0,
                                                    ..default()
                                                },
                                                TextColor(theme.text_primary),
                                            ));
                                        });

                                        row.spawn((
                                            TimeWarpReadout,
                                            Text::new("1x"),
                                            TextFont {
                                                font: font.clone(),
                                                font_size: 30.0,
                                                ..default()
                                            },
                                            TextColor(theme.text_accent),
                                        ));

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
                                            BackgroundColor(theme.panel_background),
                                        ))
                                        .with_children(|btn| {
                                            btn.spawn((
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
                            },
                        );

                        // === Simulation Controls (collapsible) ===
                        spawn_collapsible_section(
                            sidebar,
                            &font,
                            &theme,
                            "Simulation Controls",
                            CollapsibleSection::SimulationControls,
                            |content| {
                                // Map View button
                                content
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
                                    .with_children(|btn| {
                                        btn.spawn((
                                            Text::new("Map View (M)"),
                                            TextFont {
                                                font: font.clone(),
                                                font_size: 14.0,
                                                ..default()
                                            },
                                            TextColor(theme.text_primary),
                                        ));
                                    });

                                // Toggle Origin button
                                content
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
                                    .with_children(|btn| {
                                        btn.spawn((
                                            Text::new("Toggle Origin (O)"),
                                            TextFont {
                                                font: font.clone(),
                                                font_size: 14.0,
                                                ..default()
                                            },
                                            TextColor(theme.text_primary),
                                        ));
                                    });

                                // Cycle Camera Target button
                                content
                                    .spawn((
                                        Button,
                                        CycleCameraButton,
                                        Node {
                                            width: percent(100),
                                            min_height: px(40.0),
                                            align_items: AlignItems::Center,
                                            justify_content: JustifyContent::Center,
                                            ..default()
                                        },
                                        BackgroundColor(theme.panel_background_soft),
                                    ))
                                    .with_children(|btn| {
                                        btn.spawn((
                                            Text::new("Cycle Camera Target (Tab)"),
                                            TextFont {
                                                font: font.clone(),
                                                font_size: 14.0,
                                                ..default()
                                            },
                                            TextColor(theme.text_primary),
                                        ));
                                    });

                                // Capture button
                                content
                                    .spawn((
                                        Button,
                                        CaptureButton {
                                            entity: capture_target_entity,
                                            plan_id: capture_plan_id.clone(),
                                        },
                                        Node {
                                            width: percent(100),
                                            min_height: px(42.0),
                                            align_items: AlignItems::Center,
                                            justify_content: JustifyContent::Center,
                                            ..default()
                                        },
                                        BackgroundColor(theme.panel_background_soft),
                                    ))
                                    .with_children(|btn| {
                                        btn.spawn((
                                            Text::new("Capture"),
                                            TextFont {
                                                font: font.clone(),
                                                font_size: 14.0,
                                                ..default()
                                            },
                                            TextColor(theme.text_primary),
                                        ));
                                    });
                            },
                        );

                        // === Simulation HUD (collapsible) ===
                        spawn_collapsible_section(
                            sidebar,
                            &font,
                            &theme,
                            "Simulation HUD",
                            CollapsibleSection::SimulationHud,
                            |content| {
                                content.spawn((
                                    CaptureTelemetryReadout {
                                        target_entity: capture_target_entity,
                                        reference_entity: tether_root_entity,
                                        target_label: capture_target_label.clone(),
                                    },
                                    Text::new("Waiting for live capture telemetry..."),
                                    TextFont {
                                        font: font.clone(),
                                        font_size: 12.0,
                                        ..default()
                                    },
                                    TextColor(theme.text_primary),
                                ));

                                content
                                    .spawn((
                                        Node {
                                            width: percent(100),
                                            flex_direction: FlexDirection::Column,
                                            row_gap: px(8.0),
                                            ..default()
                                        },
                                    ))
                                    .with_children(|content| {
                                        content.spawn((
                                            CaptureTelemetryReadout {
                                                target_entity: capture_target_entity,
                                                reference_entity: tether_root_entity,
                                                target_label: capture_target_label.clone(),
                                            },
                                            Text::new("Waiting for live capture telemetry..."),
                                            TextFont {
                                                font: font.clone(),
                                                font_size: 12.0,
                                                ..default()
                                            },
                                            TextColor(theme.text_primary),
                                        ));

                                        content
                                            .spawn((
                                                Button,
                                                CaptureButton {
                                                    entity: capture_target_entity,
                                                    plan_id: capture_plan_id.clone(),
                                                },
                                                Node {
                                                    min_width: px(140.0),
                                                    min_height: px(42.0),
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
                                                Node {
                                                    width: percent(100),
                                                    flex_direction: FlexDirection::Column,
                                                    row_gap: px(8.0),
                                                    padding: UiRect::all(px(12.0)),
                                                    margin: UiRect::top(px(4.0)),
                                                    ..default()
                                                },
                                                BackgroundColor(theme.panel_background_soft),
                                            ))
                                            .with_children(|guidance| {
                                                guidance.spawn((
                                                    Text::new("Capture Guidance"),
                                                    TextFont {
                                                        font: font.clone(),
                                                        font_size: 13.0,
                                                        ..default()
                                                    },
                                                    TextColor(theme.text_accent),
                                                ));

                                                guidance.spawn((
                                                    CaptureGuidanceReadout {
                                                        target_entity: capture_target_entity,
                                                        reference_entity: tether_root_entity,
                                                        target_label: capture_target_label.clone(),
                                                        plan_id: capture_plan_id.clone(),
                                                    },
                                                    Text::new("Waiting for capture plan telemetry..."),
                                                    TextFont {
                                                        font: font.clone(),
                                                        font_size: 12.0,
                                                        ..default()
                                                    },
                                                    TextColor(theme.text_primary),
                                                ));
                                            });
                                    });
                            },
                        );

                        // === Reference (collapsible) ===
                        spawn_collapsible_section(
                            sidebar,
                            &font,
                            &theme,
                            "Reference",
                            CollapsibleSection::Reference,
                            |content| {
                                content.spawn((
                                    Text::new(
                                        "Target telemetry is measured against the tether root. Guidance shows the currently active state and the transitions available from it.",
                                    ),
                                    TextFont {
                                        font: font.clone(),
                                        font_size: 12.0,
                                        ..default()
                                    },
                                    TextColor(theme.text_muted),
                                ));
                            },
                        );
                    })
                    .id();

                        // Scrollbar
                        sidebar_wrapper
                            .spawn((
                                Scrollbar::new(
                                    sidebar_id,
                                    ControlOrientation::Vertical,
                                    20.0,
                                ),
                                Node {
                                    width: px(8.0),
                                    ..default()
                                },
                                BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.05)),
                            ))
                            .with_child((
                                CoreScrollbarThumb,
                                Node {
                                    width: percent(100.0),
                                    ..default()
                                },
                                BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.3)),
                            ));
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

fn spawn_collapsible_section(
    parent: &mut ChildSpawnerCommands,
    font: &Handle<Font>,
    theme: &UiTheme,
    title: &str,
    section: CollapsibleSection,
    content_builder: impl FnOnce(&mut ChildSpawnerCommands),
) {
    parent
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
        .with_children(|container| {
            // Header row with title + toggle button
            container
                .spawn(Node {
                    width: percent(100),
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::Center,
                    ..default()
                })
                .with_children(|header_row| {
                    header_row.spawn((
                        Text::new(title),
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
                            CollapsibleToggle { section },
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
                                Text::new("v"),
                                TextFont {
                                    font: font.clone(),
                                    font_size: 14.0,
                                    ..default()
                                },
                                TextColor(theme.text_muted),
                            ));
                        });
                });

            // Collapsible content
            container
                .spawn((
                    CollapsibleContent { section },
                    Node {
                        width: percent(100),
                        flex_direction: FlexDirection::Column,
                        row_gap: px(8.0),
                        ..default()
                    },
                ))
                .with_children(content_builder);
        });
}

pub fn cleanup_project_detail_screen(
    mut commands: Commands,
    roots: Query<Entity, With<SimScreen>>,
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
            Option<&ToggleOriginButton>,
            Option<&TimeWarpDecreaseButton>,
            Option<&TimeWarpIncreaseButton>,
            Option<&CycleCameraButton>,
            &mut BackgroundColor,
        ),
        (Changed<Interaction>, With<Button>),
    >,
    mut events: MessageWriter<UiEvent>,
    screen: Res<State<crate::ui::state::UiScreen>>,
    theme: Res<UiTheme>,
) {
    if *screen.get() != crate::ui::state::UiScreen::Sim {
        return;
    }

    for (
        interaction,
        back_button,
        capture_button,
        map_view_button,
        toggle_origin_button,
        time_warp_decrease,
        time_warp_increase,
        cycle_camera_button,
        mut background_color,
    ) in &mut interactions
    {
        match *interaction {
            Interaction::Pressed => {
                *background_color = BackgroundColor(theme.button_background_hover);
                if back_button.is_some() {
                    events.write(UiEvent::BackToHome);
                } else if let Some(capture_entity) = capture_button {
                    events.write(UiEvent::CaptureDebris {
                        entity: capture_entity.entity,
                        plan_id: capture_entity.plan_id.clone(),
                    });
                } else if map_view_button.is_some() {
                    events.write(UiEvent::ToggleMapView);
                } else if toggle_origin_button.is_some() {
                    events.write(UiEvent::ToggleOrigin);
                } else if time_warp_decrease.is_some() {
                    events.write(UiEvent::ChangeTimeWarp { increase: false });
                } else if time_warp_increase.is_some() {
                    events.write(UiEvent::ChangeTimeWarp { increase: true });
                } else if cycle_camera_button.is_some() {
                    events.write(UiEvent::CycleCameraTarget);
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
                            "<".to_string()
                        } else {
                            "v".to_string()
                        };
                    }
                }
            }
        }
    }
}
