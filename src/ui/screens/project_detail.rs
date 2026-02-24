use bevy::prelude::*;

use crate::components::user_interface::{OrbitLabel, TrackObject};
use crate::resources::devices::Devices;
use crate::ui::events::UiEvent;
use crate::ui::state::{ProjectCatalog, SelectedProject};
use crate::ui::theme::UiTheme;
use crate::ui::widgets::ScreenRoot;

#[derive(Component)]
pub struct ProjectDetailScreen;

#[derive(Component)]
pub struct BackButton;

pub fn spawn_project_detail_screen(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    theme: Res<UiTheme>,
    selected_project: Res<SelectedProject>,
    catalog: Res<ProjectCatalog>,
    devices: Res<Devices>,
) {
    let font = asset_server.load("fonts/FiraMono-Medium.ttf");

    let selected = selected_project
        .project_id
        .as_ref()
        .and_then(|project_id| catalog.projects.iter().find(|project| &project.id == project_id));

    let project_title = selected
        .map(|project| project.title.clone())
        .unwrap_or_else(|| "No project selected".to_string());

    let project_description = selected
        .map(|project| project.description.clone())
        .unwrap_or_else(|| "Return to Home and choose a project.".to_string());

    let tether_entity = selected.and_then(|project| devices.tethers.get(&project.tether_id).copied());

    commands
        .spawn((
            ProjectDetailScreen,
            ScreenRoot,
            Node {
                width: percent(100),
                height: percent(100),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Start,
                padding: UiRect::axes(px(24.0), px(24.0)),
                ..default()
            },
            BackgroundColor(theme.background),
        ))
        .with_children(|parent| {
            parent
                .spawn((
                    Node {
                        width: theme.content_max_width,
                        max_width: percent(100),
                        flex_direction: FlexDirection::Column,
                        padding: UiRect::all(px(20.0)),
                        row_gap: px(16.0),
                        ..default()
                    },
                    BackgroundColor(theme.panel_background),
                ))
                .with_children(|content| {
                    content
                        .spawn((
                            Node {
                                width: percent(100),
                                justify_content: JustifyContent::SpaceBetween,
                                align_items: AlignItems::Center,
                                ..default()
                            },
                        ))
                        .with_children(|top_row| {
                            top_row.spawn((
                                Text::new(project_title),
                                TextFont {
                                    font: font.clone(),
                                    font_size: 34.0,
                                    ..default()
                                },
                                TextColor(theme.text_primary),
                            ));

                            top_row
                                .spawn((
                                    Button,
                                    BackButton,
                                    Node {
                                        min_width: px(120.0),
                                        min_height: px(44.0),
                                        justify_content: JustifyContent::Center,
                                        align_items: AlignItems::Center,
                                        ..default()
                                    },
                                    BackgroundColor(theme.button_background),
                                ))
                                .with_children(|button| {
                                    button.spawn((
                                        Text::new("Back to Home"),
                                        TextFont {
                                            font: font.clone(),
                                            font_size: 14.0,
                                            ..default()
                                        },
                                        TextColor(theme.button_text),
                                    ));
                                });
                        });

                    content.spawn((
                        Text::new(project_description),
                        TextFont {
                            font: font.clone(),
                            font_size: 18.0,
                            ..default()
                        },
                        TextColor(theme.text_muted),
                    ));

                    content.spawn((
                        Text::new("Simulation HUD"),
                        TextFont {
                            font: font.clone(),
                            font_size: 20.0,
                            ..default()
                        },
                        TextColor(theme.text_primary),
                    ));

                    content.spawn((
                        TrackObject {
                            entity: tether_entity,
                        },
                        Text::new("Waiting for tether telemetry..."),
                        TextFont {
                            font: font.clone(),
                            font_size: 14.0,
                            ..default()
                        },
                        TextColor(theme.text_primary),
                        Node {
                            margin: UiRect::top(px(6.0)),
                            ..default()
                        },
                    ));

                    content.spawn((
                        OrbitLabel {
                            entity: tether_entity,
                        },
                        Text::new("─ Orbit Label"),
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
    mut interactions: Query<&Interaction, (Changed<Interaction>, With<Button>, With<BackButton>)>,
    mut events: MessageWriter<UiEvent>,
    screen: Res<State<crate::ui::state::UiScreen>>,
) {
    if *screen.get() != crate::ui::state::UiScreen::ProjectDetail {
        return;
    }

    for interaction in &mut interactions {
        if *interaction == Interaction::Pressed {
            events.write(UiEvent::BackToHome);
        }
    }
}
