use bevy::prelude::*;

use crate::ui::events::UiEvent;
use crate::ui::state::ProjectCatalog;
use crate::ui::theme::UiTheme;
use crate::ui::widgets::ScreenRoot;

#[derive(Component)]
pub struct HomeScreen;

#[derive(Component)]
pub struct HomeProjectButton {
    pub project_id: String,
}

pub fn spawn_home_screen(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    theme: Res<UiTheme>,
    projects: Res<ProjectCatalog>,
) {
    let font = asset_server.load("fonts/FiraMono-Medium.ttf");

    commands
        .spawn((
            HomeScreen,
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
                        row_gap: px(18.0),
                        ..default()
                    },
                    BackgroundColor(theme.panel_background),
                ))
                .with_children(|content| {
                    content.spawn((
                        Text::new("Tether Capture"),
                        TextFont {
                            font: font.clone(),
                            font_size: 40.0,
                            ..default()
                        },
                        TextColor(theme.text_primary),
                    ));

                    content.spawn((
                        Text::new("Select a project to open the detail workspace."),
                        TextFont {
                            font: font.clone(),
                            font_size: 18.0,
                            ..default()
                        },
                        TextColor(theme.text_muted),
                    ));

                    content
                        .spawn(Node {
                            width: percent(100),
                            flex_direction: FlexDirection::Column,
                            row_gap: px(12.0),
                            margin: UiRect::top(px(10.0)),
                            ..default()
                        })
                        .with_children(|list| {
                            for project in &projects.projects {
                                list.spawn((
                                    Button,
                                    HomeProjectButton {
                                        project_id: project.id.clone(),
                                    },
                                    Node {
                                        width: percent(100),
                                        min_height: px(64.0),
                                        justify_content: JustifyContent::Center,
                                        align_items: AlignItems::Center,
                                        padding: UiRect::axes(px(14.0), px(10.0)),
                                        ..default()
                                    },
                                    BackgroundColor(theme.button_background),
                                ))
                                .with_children(|button| {
                                    button.spawn((
                                        Text::new(format!("{} — {}", project.title, project.description)),
                                        TextFont {
                                            font: font.clone(),
                                            font_size: 16.0,
                                            ..default()
                                        },
                                        TextColor(theme.button_text),
                                    ));
                                });
                            }
                        });
                });
        });
}

pub fn cleanup_home_screen(mut commands: Commands, roots: Query<Entity, With<HomeScreen>>) {
    for entity in &roots {
        commands.entity(entity).despawn();
    }
}

pub fn home_interactions(
    mut interactions: Query<(&Interaction, &HomeProjectButton), (Changed<Interaction>, With<Button>)>,
    mut events: MessageWriter<UiEvent>,
    screen: Res<State<crate::ui::state::UiScreen>>,
) {
    if *screen.get() != crate::ui::state::UiScreen::Home {
        return;
    }

    for (interaction, project_button) in &mut interactions {
        if *interaction == Interaction::Pressed {
            events.write(UiEvent::OpenProject(project_button.project_id.clone()));
        }
    }
}
