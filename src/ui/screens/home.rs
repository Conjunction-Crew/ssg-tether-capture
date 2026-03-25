use bevy::prelude::*;
use bevy::camera::visibility::RenderLayers;

use crate::constants::UI_LAYER;
use crate::resources::capture_plans::CapturePlanLibrary;
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
    capture_plan_lib: Res<CapturePlanLibrary>,
) {
    let font = asset_server.load("fonts/FiraMono-Medium.ttf");
    let project_count_label = format!(
        "{} project{} in workspace",
        projects.projects.len(),
        if projects.projects.len() == 1 { "" } else { "s" }
    );
    let capture_plan_count_label = format!(
        "{} capture plan{} in workspace",
        capture_plan_lib.plans.len(),
        if capture_plan_lib.plans.len() == 1 { "" } else { "s" }
    );
    let working_directory = projects
        .projects
        .first()
        .map(|project| project.working_directory.clone())
        .unwrap_or_else(|| "/home/user/satellite-projects".to_string());

    commands
        .spawn((
            HomeScreen,
            ScreenRoot,
            RenderLayers::layer(UI_LAYER),
            Node {
                width: percent(100),
                height: percent(100),
                flex_direction: FlexDirection::Column,
                ..default()
            },
            BackgroundColor(theme.background),
        ))
        .with_children(|parent| {
            parent
                .spawn((
                    Node {
                        width: percent(100),
                        min_height: px(84.0),
                        padding: UiRect::axes(px(24.0), px(16.0)),
                        justify_content: JustifyContent::Center,
                        ..default()
                    },
                    BackgroundColor(theme.header_background),
                ))
                .with_children(|header| {
                    header.spawn((
                        Text::new("Tether Capture"),
                        TextFont {
                            font: font.clone(),
                            font_size: 30.0,
                            ..default()
                        },
                        TextColor(theme.text_primary),
                    ));

                    header.spawn((
                        Text::new("Conjunction Crew"),
                        TextFont {
                            font: font.clone(),
                            font_size: 14.0,
                            ..default()
                        },
                        TextColor(theme.text_muted),
                    ));
                });

            parent
                .spawn(Node {
                    width: percent(100),
                    height: percent(100),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Start,
                    padding: UiRect::axes(px(24.0), px(20.0)),
                    ..default()
                })
                .with_children(|body| {
                    body.spawn(Node {
                        width: theme.content_max_width,
                        max_width: percent(100),
                        flex_direction: FlexDirection::Column,
                        row_gap: px(16.0),
                        ..default()
                    })
                    .with_children(|content| {
                        content
                            .spawn((
                                Node {
                                    width: percent(100),
                                    flex_direction: FlexDirection::Column,
                                    row_gap: px(8.0),
                                    padding: UiRect::all(px(16.0)),
                                    ..default()
                                },
                                BackgroundColor(theme.panel_background_soft),
                            ))
                            .with_children(|workspace| {
                                workspace.spawn((
                                    Text::new("Working Directory"),
                                    TextFont {
                                        font: font.clone(),
                                        font_size: 17.0,
                                        ..default()
                                    },
                                    TextColor(theme.text_primary),
                                ));

                                workspace.spawn((
                                    Text::new("Set the location where your projects are stored"),
                                    TextFont {
                                        font: font.clone(),
                                        font_size: 12.0,
                                        ..default()
                                    },
                                    TextColor(theme.text_muted),
                                ));

                                workspace.spawn((
                                    Text::new(working_directory),
                                    TextFont {
                                        font: font.clone(),
                                        font_size: 13.0,
                                        ..default()
                                    },
                                    TextColor(theme.text_primary),
                                ));
                            });

                        content
                            .spawn(Node {
                                width: percent(100),
                                justify_content: JustifyContent::SpaceBetween,
                                align_items: AlignItems::Center,
                                margin: UiRect::top(px(10.0)),
                                ..default()
                            })
                            .with_children(|projects_header| {
                                projects_header.spawn((
                                    Text::new("Projects"),
                                    TextFont {
                                        font: font.clone(),
                                        font_size: 24.0,
                                        ..default()
                                    },
                                    TextColor(theme.text_primary),
                                ));

                                projects_header.spawn((
                                    Text::new(project_count_label),
                                    TextFont {
                                        font: font.clone(),
                                        font_size: 12.0,
                                        ..default()
                                    },
                                    TextColor(theme.text_muted),
                                ));
                            });

                        content
                            .spawn(Node {
                                width: percent(100),
                                flex_direction: FlexDirection::Row,
                                flex_wrap: FlexWrap::Wrap,
                                column_gap: px(12.0),
                                row_gap: px(12.0),
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
                                            width: px(340.0),
                                            max_width: percent(100),
                                            min_height: px(148.0),
                                            flex_direction: FlexDirection::Column,
                                            justify_content: JustifyContent::SpaceBetween,
                                            padding: UiRect::all(px(14.0)),
                                            row_gap: px(8.0),
                                            ..default()
                                        },
                                        BackgroundColor(theme.panel_background),
                                    ))
                                    .with_children(|button| {
                                        button.spawn((
                                            Text::new(project.title.clone()),
                                            TextFont {
                                                font: font.clone(),
                                                font_size: 18.0,
                                                ..default()
                                            },
                                            TextColor(theme.text_primary),
                                        ));

                                        button.spawn((
                                            Text::new(project.description.clone()),
                                            TextFont {
                                                font: font.clone(),
                                                font_size: 12.0,
                                                ..default()
                                            },
                                            TextColor(theme.text_muted),
                                        ));

                                        button
                                            .spawn(Node {
                                                width: percent(100),
                                                justify_content: JustifyContent::SpaceBetween,
                                                ..default()
                                            })
                                            .with_children(|meta| {
                                                meta.spawn((
                                                    Text::new(format!(
                                                        "Modified {}",
                                                        project.last_modified
                                                    )),
                                                    TextFont {
                                                        font: font.clone(),
                                                        font_size: 11.0,
                                                        ..default()
                                                    },
                                                    TextColor(theme.text_muted),
                                                ));

                                                meta.spawn((
                                                    Text::new(format!(
                                                        "{} captures",
                                                        project.capture_count
                                                    )),
                                                    TextFont {
                                                        font: font.clone(),
                                                        font_size: 11.0,
                                                        ..default()
                                                    },
                                                    TextColor(theme.text_accent),
                                                ));
                                            });
                                    });
                                }
                            });

                        content
                            .spawn(Node {
                                width: percent(100),
                                justify_content: JustifyContent::SpaceBetween,
                                align_items: AlignItems::Center,
                                margin: UiRect::top(px(10.0)),
                                ..default()
                            })
                            .with_children(|capture_plans_header| {
                                capture_plans_header.spawn((
                                    Text::new("Capture Plans"),
                                    TextFont {
                                        font: font.clone(),
                                        font_size: 24.0,
                                        ..default()
                                    },
                                    TextColor(theme.text_primary),
                                ));

                                capture_plans_header.spawn((
                                    Text::new(capture_plan_count_label),
                                    TextFont {
                                        font: font.clone(),
                                        font_size: 12.0,
                                        ..default()
                                    },
                                    TextColor(theme.text_muted),
                                ));
                            });

                        content
                            .spawn(Node {
                                width: percent(100),
                                flex_direction: FlexDirection::Row,
                                flex_wrap: FlexWrap::Wrap,
                                column_gap: px(12.0),
                                row_gap: px(12.0),
                                ..default()
                            })
                            .with_children(|list| {
                                for (plan_name, _plan) in &capture_plan_lib.plans {
                                    list.spawn((
                                        Node {
                                            width: px(340.0),
                                            max_width: percent(100),
                                            min_height: px(80.0),
                                            flex_direction: FlexDirection::Column,
                                            padding: UiRect::all(px(14.0)),
                                            row_gap: px(8.0),
                                            ..default()
                                        },
                                        BackgroundColor(theme.panel_background),
                                    ))
                                    .with_children(|card| {
                                        card.spawn((
                                            Text::new(plan_name.clone()),
                                            TextFont {
                                                font: font.clone(),
                                                font_size: 18.0,
                                                ..default()
                                            },
                                            TextColor(theme.text_primary),
                                        ));

                                        card.spawn((
                                            Text::new(plan_name.clone()),
                                            TextFont {
                                                font: font.clone(),
                                                font_size: 12.0,
                                                ..default()
                                            },
                                            TextColor(theme.text_muted),
                                        ));
                                    });
                                }
                            });
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
    mut interactions: Query<
        (&Interaction, &HomeProjectButton, &mut BackgroundColor),
        (Changed<Interaction>, With<Button>),
    >,
    mut events: MessageWriter<UiEvent>,
    screen: Res<State<crate::ui::state::UiScreen>>,
    theme: Res<UiTheme>,
) {
    if *screen.get() != crate::ui::state::UiScreen::Home {
        return;
    }

    for (interaction, project_button, mut background_color) in &mut interactions {
        match *interaction {
            Interaction::Pressed => {
                *background_color = BackgroundColor(theme.button_background_hover);
                events.write(UiEvent::OpenProject(project_button.project_id.clone()));
            }
            Interaction::Hovered => {
                *background_color = BackgroundColor(theme.panel_background_soft);
            }
            Interaction::None => {
                *background_color = BackgroundColor(theme.panel_background);
            }
        }
    }
}
