use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;

use crate::constants::UI_LAYER;
use crate::resources::working_directory::WorkingDirectory;
use crate::ui::events::UiEvent;
use crate::ui::state::UiScreen;
use crate::ui::theme::UiTheme;
use crate::ui::widgets::ScreenRoot;

#[derive(Component)]
pub struct WorkingDirectorySetupScreen;

#[derive(Component)]
pub struct SelectDirectoryButton;

#[derive(Component)]
pub struct BrowseButton;

#[derive(Component)]
pub struct DirectoryPathText;

pub fn spawn_working_directory_setup_screen(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    theme: Res<UiTheme>,
    working_directory: Res<WorkingDirectory>,
) {
    let font = asset_server.load("fonts/FiraMono-Medium.ttf");

    commands
        .spawn((
            WorkingDirectorySetupScreen,
            ScreenRoot,
            RenderLayers::layer(UI_LAYER),
            Node {
                width: percent(100),
                height: percent(100),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(theme.background),
        ))
        .with_children(|parent| {
            parent
                .spawn((
                    Node {
                        width: px(560.0),
                        max_width: percent(90),
                        flex_direction: FlexDirection::Column,
                        row_gap: px(16.0),
                        padding: UiRect::all(px(28.0)),
                        ..default()
                    },
                    BackgroundColor(theme.panel_background),
                ))
                .with_children(|modal| {
                    modal.spawn((
                        Text::new("Select Working Directory"),
                        TextFont {
                            font: font.clone(),
                            font_size: 22.0,
                            ..default()
                        },
                        TextColor(theme.text_primary),
                    ));

                    modal.spawn((
                        Text::new("Choose where your projects will be stored."),
                        TextFont {
                            font: font.clone(),
                            font_size: 13.0,
                            ..default()
                        },
                        TextColor(theme.text_muted),
                    ));

                    modal
                        .spawn((
                            Node {
                                width: percent(100),
                                padding: UiRect::all(px(10.0)),
                                ..default()
                            },
                            BackgroundColor(theme.panel_background_soft),
                        ))
                        .with_children(|path_box| {
                            path_box.spawn((
                                DirectoryPathText,
                                Text::new(working_directory.pending_path.clone()),
                                TextFont {
                                    font: font.clone(),
                                    font_size: 13.0,
                                    ..default()
                                },
                                TextColor(theme.text_accent),
                            ));
                        });

                    modal
                        .spawn(Node {
                            width: percent(100),
                            justify_content: JustifyContent::End,
                            column_gap: px(10.0),
                            ..default()
                        })
                        .with_children(|buttons| {
                            buttons
                                .spawn((
                                    Button,
                                    BrowseButton,
                                    Node {
                                        padding: UiRect::axes(px(18.0), px(9.0)),
                                        ..default()
                                    },
                                    BackgroundColor(theme.panel_background_soft),
                                ))
                                .with_children(|btn| {
                                    btn.spawn((
                                        Text::new("Browse"),
                                        TextFont {
                                            font: font.clone(),
                                            font_size: 14.0,
                                            ..default()
                                        },
                                        TextColor(theme.text_primary),
                                    ));
                                });

                            buttons
                                .spawn((
                                    Button,
                                    SelectDirectoryButton,
                                    Node {
                                        padding: UiRect::axes(px(18.0), px(9.0)),
                                        ..default()
                                    },
                                    BackgroundColor(theme.button_background),
                                ))
                                .with_children(|btn| {
                                    btn.spawn((
                                        Text::new("Select"),
                                        TextFont {
                                            font: font.clone(),
                                            font_size: 14.0,
                                            ..default()
                                        },
                                        TextColor(theme.button_text),
                                    ));
                                });
                        });
                });
        });
}

pub fn cleanup_working_directory_setup_screen(
    mut commands: Commands,
    roots: Query<Entity, With<WorkingDirectorySetupScreen>>,
) {
    for entity in &roots {
        commands.entity(entity).despawn();
    }
}

pub fn working_directory_setup_interactions(
    mut select_q: Query<
        (&Interaction, &mut BackgroundColor),
        (
            Changed<Interaction>,
            With<Button>,
            With<SelectDirectoryButton>,
            Without<BrowseButton>,
        ),
    >,
    mut browse_q: Query<
        (&Interaction, &mut BackgroundColor),
        (
            Changed<Interaction>,
            With<Button>,
            With<BrowseButton>,
            Without<SelectDirectoryButton>,
        ),
    >,
    mut events: MessageWriter<UiEvent>,
    screen: Res<State<UiScreen>>,
    theme: Res<UiTheme>,
    working_directory: Res<WorkingDirectory>,
) {
    if *screen.get() != UiScreen::WorkingDirectorySetup {
        return;
    }

    for (interaction, mut bg) in &mut select_q {
        match *interaction {
            Interaction::Pressed => {
                events.write(UiEvent::WorkingDirectorySelected(
                    working_directory.pending_path.clone(),
                ));
            }
            Interaction::Hovered => {
                *bg = BackgroundColor(theme.button_background_hover);
            }
            Interaction::None => {
                *bg = BackgroundColor(theme.button_background);
            }
        }
    }

    for (interaction, mut bg) in &mut browse_q {
        match *interaction {
            Interaction::Pressed => {
                events.write(UiEvent::BrowseForWorkingDirectory);
            }
            Interaction::Hovered => {
                *bg = BackgroundColor(theme.panel_background);
            }
            Interaction::None => {
                *bg = BackgroundColor(theme.panel_background_soft);
            }
        }
    }
}
