use std::fs;
use std::path::PathBuf;

use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;

use crate::constants::UI_LAYER;
use crate::resources::capture_plans::CapturePlanLibrary;
use crate::resources::working_directory::WorkingDirectory;
use crate::ui::events::UiEvent;
use crate::ui::state::{ProjectCatalog, UiScreen};
use crate::ui::theme::UiTheme;
use crate::ui::widgets::ScreenRoot;

#[derive(Component)]
pub struct HomeScreen;

#[derive(Component)]
pub struct HomeProjectButton {
    pub project_id: String,
}

#[derive(Component)]
pub struct HomeWorkingDirectoryLabel;

#[derive(Component)]
pub struct ChangeDirectoryButton;

pub fn load_capture_plans(capture_plan_lib: &mut CapturePlanLibrary) {
    let plans_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/capture_plans");

    for plan_file_result in fs::read_dir(&plans_dir).expect("failed to read capture_plans dir") {
        if let Ok(plan_file) = plan_file_result {
            let path = plan_file.path();
            if path
                .extension()
                .is_some_and(|extension| extension == "json")
            {
                if let Ok(raw_json) = fs::read_to_string(&path) {
                    if let Ok(plan) = serde_json::from_str(&raw_json) {
                        if let Some(plan_id) = path.file_stem() {
                            capture_plan_lib.plans.insert(
                                String::from(plan_id.to_str().expect("failed to get plan name!")),
                                plan,
                            );
                        }
                    } else {
                        println!("Failed to parse plan json");
                    }
                }
            }
        }
    }
}

pub fn spawn_home_screen(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    theme: Res<UiTheme>,
    projects: Res<ProjectCatalog>,
    mut capture_plan_lib: ResMut<CapturePlanLibrary>,
    working_directory: Res<WorkingDirectory>,
) {
    load_capture_plans(&mut capture_plan_lib);

    println!(
        "num plans loaded (plugin): {}",
        capture_plan_lib.plans.len()
    );
    let font = asset_server.load("fonts/FiraMono-Medium.ttf");
    let project_count_label = format!(
        "{} project{} in workspace",
        projects.projects.len(),
        if projects.projects.len() == 1 {
            ""
        } else {
            "s"
        }
    );
    let capture_plan_count_label = format!(
        "{} capture plan{} in workspace",
        capture_plan_lib.plans.len(),
        if capture_plan_lib.plans.len() == 1 {
            ""
        } else {
            "s"
        }
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
                                    HomeWorkingDirectoryLabel,
                                    Text::new(working_directory),
                                    TextFont {
                                        font: font.clone(),
                                        font_size: 13.0,
                                        ..default()
                                    },
                                    TextColor(theme.text_primary),
                                ));

                                workspace
                                    .spawn(Node {
                                        width: percent(100),
                                        justify_content: JustifyContent::End,
                                        margin: UiRect::top(px(4.0)),
                                        ..default()
                                    })
                                    .with_children(|row| {
                                        row.spawn((
                                            Button,
                                            ChangeDirectoryButton,
                                            Node {
                                                padding: UiRect::axes(px(14.0), px(7.0)),
                                                ..default()
                                            },
                                            BackgroundColor(theme.panel_background),
                                        ))
                                        .with_children(|btn| {
                                            btn.spawn((
                                                Text::new("Change Directory"),
                                                TextFont {
                                                    font: font.clone(),
                                                    font_size: 12.0,
                                                    ..default()
                                                },
                                                TextColor(theme.text_muted),
                                            ));
                                        });
                                    });
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
                                for (plan_name, plan) in &capture_plan_lib.plans {
                                    list.spawn((
                                        Button,
                                        HomeProjectButton {
                                            project_id: plan_name.clone(),
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
                                            Text::new(plan_name.clone()),
                                            TextFont {
                                                font: font.clone(),
                                                font_size: 18.0,
                                                ..default()
                                            },
                                            TextColor(theme.text_primary),
                                        ));

                                        button.spawn((
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
    mut project_interactions: Query<
        (&Interaction, &HomeProjectButton, &mut BackgroundColor),
        (Changed<Interaction>, With<Button>, Without<ChangeDirectoryButton>),
    >,
    mut change_dir_interactions: Query<
        (&Interaction, &mut BackgroundColor),
        (Changed<Interaction>, With<Button>, With<ChangeDirectoryButton>),
    >,
    mut events: MessageWriter<UiEvent>,
    screen: Res<State<UiScreen>>,
    theme: Res<UiTheme>,
) {
    if *screen.get() != UiScreen::Home {
        return;
    }

    for (interaction, project_button, mut background_color) in &mut project_interactions {
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

    for (interaction, mut background_color) in &mut change_dir_interactions {
        match *interaction {
            Interaction::Pressed => {
                events.write(UiEvent::ChangeWorkingDirectory);
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

pub fn update_home_working_directory_label(
    working_directory: Res<WorkingDirectory>,
    mut label_query: Query<&mut Text, With<HomeWorkingDirectoryLabel>>,
) {
    if !working_directory.is_changed() {
        return;
    }
    for mut text in &mut label_query {
        text.0 = working_directory.path.clone();
    }
}
