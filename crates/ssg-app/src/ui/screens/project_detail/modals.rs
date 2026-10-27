use super::*;

pub fn spawn_restart_prompt_modal(commands: &mut Commands, font: &Handle<Font>, theme: &UiTheme) {
    commands
        .spawn((
            RestartPromptModal,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.65)),
            Pickable::default(),
            ZIndex(100),
            RenderLayers::layer(crate::render::layers::UI_LAYER),
        ))
        .with_children(|overlay| {
            overlay
                .spawn((
                    Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(14.0),
                        padding: UiRect::all(Val::Px(28.0)),
                        width: Val::Px(420.0),
                        max_width: Val::Percent(90.0),
                        ..default()
                    },
                    BackgroundColor(theme.panel_background),
                ))
                .with_children(|dlg| {
                    dlg.spawn((
                        Text::new("Plan Changed"),
                        TextFont { font: font.clone(), font_size: 20.0, ..default() },
                        TextColor(theme.text_primary),
                    ));

                    dlg.spawn((
                        Text::new("The capture plan has been updated. Restart the simulation to apply changes?"),
                        TextFont { font: font.clone(), font_size: 13.0, ..default() },
                        TextColor(theme.text_muted),
                    ));

                    dlg.spawn(Node {
                        width: Val::Percent(100.0),
                        justify_content: JustifyContent::End,
                        column_gap: Val::Px(10.0),
                        ..default()
                    })
                    .with_children(|btns| {
                        btns.spawn((
                            Button,
                            DismissRestartButton,
                            Node {
                                padding: UiRect::axes(Val::Px(14.0), Val::Px(8.0)),
                                ..default()
                            },
                            BackgroundColor(theme.panel_background_soft),
                        ))
                        .with_children(|btn| {
                            btn.spawn((
                                Text::new("Continue"),
                                TextFont { font: font.clone(), font_size: 13.0, ..default() },
                                TextColor(theme.text_primary),
                            ));
                        });

                        btns.spawn((
                            Button,
                            RecompileLiveButton,
                            Node {
                                padding: UiRect::axes(Val::Px(14.0), Val::Px(8.0)),
                                ..default()
                            },
                            BackgroundColor(theme.panel_background_soft),
                        ))
                        .with_children(|btn| {
                            btn.spawn((
                                Text::new("Apply Live"),
                                TextFont { font: font.clone(), font_size: 13.0, ..default() },
                                TextColor(theme.text_accent),
                            ));
                        });

                        btns.spawn((
                            Button,
                            RestartSimButton,
                            Node {
                                padding: UiRect::axes(Val::Px(14.0), Val::Px(8.0)),
                                ..default()
                            },
                            BackgroundColor(theme.button_background),
                        ))
                        .with_children(|btn| {
                            btn.spawn((
                                Text::new("Restart Sim"),
                                TextFont { font: font.clone(), font_size: 13.0, ..default() },
                                TextColor(theme.button_text),
                            ));
                        });
                    });
                });
        });
}

pub fn spawn_exit_confirm_modal(commands: &mut Commands, font: &Handle<Font>, theme: &UiTheme) {
    commands
        .spawn((
            ExitSimConfirmModal,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.65)),
            Pickable::default(),
            ZIndex(100),
            RenderLayers::layer(crate::render::layers::UI_LAYER),
        ))
        .with_children(|overlay| {
            overlay
                .spawn((
                    Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(14.0),
                        padding: UiRect::all(Val::Px(28.0)),
                        width: Val::Px(420.0),
                        max_width: Val::Percent(90.0),
                        ..default()
                    },
                    BackgroundColor(theme.panel_background),
                ))
                .with_children(|dlg| {
                    dlg.spawn((
                        Text::new("Exit Simulation?"),
                        TextFont {
                            font: font.clone(),
                            font_size: 20.0,
                            ..default()
                        },
                        TextColor(theme.text_primary),
                    ));

                    dlg.spawn((
                        Text::new(
                            "Your orbital simulation will be reset. Are you sure you want to exit?",
                        ),
                        TextFont {
                            font: font.clone(),
                            font_size: 13.0,
                            ..default()
                        },
                        TextColor(theme.text_muted),
                    ));

                    dlg.spawn(Node {
                        width: Val::Percent(100.0),
                        justify_content: JustifyContent::End,
                        column_gap: Val::Px(10.0),
                        ..default()
                    })
                    .with_children(|btns| {
                        btns.spawn((
                            Button,
                            ExitSimCancelButton,
                            Node {
                                padding: UiRect::axes(Val::Px(14.0), Val::Px(8.0)),
                                ..default()
                            },
                            BackgroundColor(theme.panel_background_soft),
                        ))
                        .with_children(|btn| {
                            btn.spawn((
                                Text::new("Cancel"),
                                TextFont {
                                    font: font.clone(),
                                    font_size: 13.0,
                                    ..default()
                                },
                                TextColor(theme.text_primary),
                            ));
                        });

                        btns.spawn((
                            Button,
                            ExitSimConfirmButton,
                            Node {
                                padding: UiRect::axes(Val::Px(14.0), Val::Px(8.0)),
                                ..default()
                            },
                            BackgroundColor(theme.button_background),
                        ))
                        .with_children(|btn| {
                            btn.spawn((
                                Text::new("Exit Sim"),
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
        });
}
