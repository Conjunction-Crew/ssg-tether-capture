use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;

use crate::constants::UI_LAYER;
use crate::resources::capture_plans::{load_plans_from_dir, CapturePlanLibrary};
use crate::resources::new_capture_plan_form::NewCapturePlanForm;
use crate::resources::working_directory::WorkingDirectory;
use crate::ui::events::UiEvent;
use crate::ui::state::UiScreen;
use crate::ui::theme::UiTheme;
use crate::ui::widgets::ScreenRoot;

#[derive(Component)]
pub struct HomeScreen;

#[derive(Component)]
pub struct HomeProjectButton {
    pub project_id: String,
}

#[derive(Component)]
pub struct EditCapturePlanButton {
    pub plan_id: String,
}

#[derive(Component)]
pub struct HomeWorkingDirectoryLabel;

#[derive(Component)]
pub struct ChangeDirectoryButton;

#[derive(Component)]
pub struct NewPlanButton;

pub fn spawn_home_screen(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    theme: Res<UiTheme>,
    mut capture_plan_lib: ResMut<CapturePlanLibrary>,
    working_directory: Res<WorkingDirectory>,
) {
    let example_plans = load_plans_from_dir(
        &std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/capture_plans"),
    );
    let user_plans =
        load_plans_from_dir(&std::path::PathBuf::from(&working_directory.path));

    capture_plan_lib.example_plans = example_plans;
    capture_plan_lib.user_plans = user_plans;
    capture_plan_lib.plans = capture_plan_lib
        .example_plans
        .iter()
        .chain(capture_plan_lib.user_plans.iter())
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    spawn_home_screen_inner(&mut commands, &asset_server, &theme, &capture_plan_lib, &working_directory.path);
}

pub fn spawn_home_screen_inner(
    commands: &mut Commands,
    asset_server: &AssetServer,
    theme: &UiTheme,
    capture_plan_lib: &CapturePlanLibrary,
    working_directory_path: &str,
) {
    let font: Handle<Font> = asset_server.load("fonts/FiraMono-Medium.ttf");
    let user_plan_count_label = format!(
        "{} plan{} in working directory",
        capture_plan_lib.user_plans.len(),
        if capture_plan_lib.user_plans.len() == 1 {
            ""
        } else {
            "s"
        }
    );
    let example_plan_count_label = format!(
        "{} example plan{}",
        capture_plan_lib.example_plans.len(),
        if capture_plan_lib.example_plans.len() == 1 {
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
    let working_directory = working_directory_path.to_string();

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
                    align_items: AlignItems::Center,
                    ..default()
                },
                BackgroundColor(theme.header_background),
            ))
            .with_children(|header| {
                header
                    .spawn(Node {
                        flex_direction: FlexDirection::Row,
                        align_items: AlignItems::Center,
                        column_gap: px(14.0),
                        ..default()
                    })
                    .with_children(|row| {
                        row.spawn((
                            ImageNode::new(
                                asset_server.load("logo/tether-capture.iconset/icon_128x128.png"),
                            ),
                            Node {
                                height: px(50.0),
                                width: Val::Auto,
                                ..default()
                            },
                        ));

                        row.spawn(Node {
                            flex_direction: FlexDirection::Column,
                            row_gap: px(2.0),
                            ..default()
                        })
                        .with_children(|text_col| {
                            text_col.spawn((
                                Text::new("Tether Capture"),
                                TextFont {
                                    font: font.clone(),
                                    font_size: 30.0,
                                    ..default()
                                },
                                TextColor(theme.text_primary),
                            ));

                            text_col.spawn((
                                Text::new("Conjunction Crew"),
                                TextFont {
                                    font: font.clone(),
                                    font_size: 14.0,
                                    ..default()
                                },
                                TextColor(theme.text_muted),
                            ));
                        });
                    });
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
                            .with_children(|header| {
                                header.spawn((
                                    Text::new("My Capture Plans"),
                                    TextFont {
                                        font: font.clone(),
                                        font_size: 24.0,
                                        ..default()
                                    },
                                    TextColor(theme.text_primary),
                                ));

                                header
                                    .spawn(Node {
                                        align_items: AlignItems::Center,
                                        column_gap: px(10.0),
                                        ..default()
                                    })
                                    .with_children(|right| {
                                        right.spawn((
                                            Text::new(user_plan_count_label),
                                            TextFont {
                                                font: font.clone(),
                                                font_size: 12.0,
                                                ..default()
                                            },
                                            TextColor(theme.text_muted),
                                        ));

                                        right
                                            .spawn((
                                                Button,
                                                NewPlanButton,
                                                Node {
                                                    padding: UiRect::axes(px(12.0), px(6.0)),
                                                    ..default()
                                                },
                                                BackgroundColor(theme.button_background),
                                            ))
                                            .with_children(|btn| {
                                                btn.spawn((
                                                    Text::new("+ New Plan"),
                                                    TextFont {
                                                        font: font.clone(),
                                                        font_size: 13.0,
                                                        ..default()
                                                    },
                                                    TextColor(theme.button_text),
                                                ));
                                            });
                                    });
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
                                for (plan_name, plan) in &capture_plan_lib.user_plans {
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
                                        // Top row: plan name + edit icon button
                                        button
                                            .spawn(Node {
                                                width: percent(100),
                                                flex_direction: FlexDirection::Row,
                                                justify_content: JustifyContent::SpaceBetween,
                                                align_items: AlignItems::Center,
                                                ..default()
                                            })
                                            .with_children(|row| {
                                                row.spawn((
                                                    Text::new(plan.name.clone()),
                                                    TextFont {
                                                        font: font.clone(),
                                                        font_size: 18.0,
                                                        ..default()
                                                    },
                                                    TextColor(theme.text_primary),
                                                ));

                                                row.spawn((
                                                    Button,
                                                    EditCapturePlanButton {
                                                        plan_id: plan_name.clone(),
                                                    },
                                                    Node {
                                                        min_width: px(36.0),
                                                        min_height: px(28.0),
                                                        align_items: AlignItems::Center,
                                                        justify_content: JustifyContent::Center,
                                                        padding: UiRect::axes(px(6.0), px(0.0)),
                                                        ..default()
                                                    },
                                                    BackgroundColor(theme.panel_background_soft),
                                                ))
                                                .with_children(|btn| {
                                                    btn.spawn((
                                                        Text::new("edit"),
                                                        TextFont {
                                                            font: font.clone(),
                                                            font_size: 11.0,
                                                            ..default()
                                                        },
                                                        TextColor(theme.text_muted),
                                                    ));
                                                });
                                            });

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

                        content
                            .spawn(Node {
                                width: percent(100),
                                justify_content: JustifyContent::SpaceBetween,
                                align_items: AlignItems::Center,
                                margin: UiRect::top(px(10.0)),
                                ..default()
                            })
                            .with_children(|header| {
                                header.spawn((
                                    Text::new("Example Capture Plans"),
                                    TextFont {
                                        font: font.clone(),
                                        font_size: 24.0,
                                        ..default()
                                    },
                                    TextColor(theme.text_primary),
                                ));

                                header.spawn((
                                    Text::new(example_plan_count_label),
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
                                for (plan_name, plan) in &capture_plan_lib.example_plans {
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
                                            Text::new(plan.name.clone()),
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
        (Changed<Interaction>, With<Button>, Without<ChangeDirectoryButton>, Without<NewPlanButton>, Without<EditCapturePlanButton>),
    >,
    mut change_dir_interactions: Query<
        (&Interaction, &mut BackgroundColor),
        (Changed<Interaction>, With<Button>, With<ChangeDirectoryButton>, Without<NewPlanButton>, Without<EditCapturePlanButton>),
    >,
    mut new_plan_interactions: Query<
        (&Interaction, &mut BackgroundColor),
        (Changed<Interaction>, With<Button>, With<NewPlanButton>, Without<EditCapturePlanButton>),
    >,
    mut edit_interactions: Query<
        (&Interaction, &EditCapturePlanButton, &mut BackgroundColor),
        (Changed<Interaction>, With<Button>),
    >,
    mut events: MessageWriter<UiEvent>,
    screen: Res<State<UiScreen>>,
    theme: Res<UiTheme>,
    form: Res<NewCapturePlanForm>,
) {
    if *screen.get() != UiScreen::Home {
        return;
    }
    if form.open {
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

    for (interaction, mut background_color) in &mut new_plan_interactions {
        match *interaction {
            Interaction::Pressed => {
                events.write(UiEvent::OpenNewCapturePlanForm);
            }
            Interaction::Hovered => {
                *background_color = BackgroundColor(theme.button_background_hover);
            }
            Interaction::None => {
                *background_color = BackgroundColor(theme.button_background);
            }
        }
    }

    for (interaction, edit_button, mut background_color) in &mut edit_interactions {
        match *interaction {
            Interaction::Pressed => {
                events.write(UiEvent::EditCapturePlan(edit_button.plan_id.clone()));
            }
            Interaction::Hovered => {
                *background_color = BackgroundColor(theme.panel_background);
            }
            Interaction::None => {
                *background_color = BackgroundColor(theme.panel_background_soft);
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
