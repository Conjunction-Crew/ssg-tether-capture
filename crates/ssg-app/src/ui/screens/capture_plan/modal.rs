use super::*;

// ── Helpers ────────────────────────────────────────────────────────────────

fn section_header<'a>(
    parent: &mut ChildSpawnerCommands<'a>,
    label: &str,
    font: &Handle<Font>,
    theme: &UiTheme,
) {
    parent
        .spawn((
            Node {
                width: Val::Percent(100.0),
                padding: UiRect::axes(Val::Px(0.0), Val::Px(4.0)),
                border: UiRect::bottom(Val::Px(1.0)),
                ..default()
            },
            BorderColor::all(theme.panel_background),
            BackgroundColor(Color::NONE),
        ))
        .with_children(|row| {
            row.spawn((
                Text::new(label),
                TextFont {
                    font: font.clone(),
                    font_size: 15.0,
                    ..default()
                },
                TextColor(theme.text_accent),
            ));
        });
}

fn field_row<'a>(
    parent: &mut ChildSpawnerCommands<'a>,
    label: &str,
    field_id: FormFieldId,
    placeholder: &str,
    value: &str,
    is_numeric: bool,
    has_error: bool,
    font: &Handle<Font>,
    theme: &UiTheme,
) {
    let border_color = if has_error {
        Color::srgb(0.9, 0.3, 0.3)
    } else {
        theme.panel_background
    };

    parent
        .spawn(Node {
            width: Val::Percent(100.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(4.0),
            ..default()
        })
        .with_children(|col| {
            col.spawn((
                Text::new(label),
                TextFont {
                    font: font.clone(),
                    font_size: 11.0,
                    ..default()
                },
                TextColor(theme.text_muted),
            ));

            let display = if value.is_empty() {
                placeholder.to_string()
            } else {
                value.to_string()
            };
            let text_color = if value.is_empty() {
                theme.text_muted
            } else {
                theme.text_primary
            };

            col.spawn((
                Button,
                InputField {
                    value: value.to_string(),
                    cursor_pos: value.len(),
                    focused: false,
                    placeholder: placeholder.to_string(),
                    is_numeric,
                    error: has_error,
                    selection_anchor: None,
                },
                field_id,
                Node {
                    width: Val::Percent(100.0),
                    padding: UiRect::axes(Val::Px(10.0), Val::Px(8.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BorderColor::all(border_color),
                BackgroundColor(theme.panel_background_soft),
            ))
            .with_children(|btn| {
                btn.spawn((
                    InputFieldText,
                    Text::new(display),
                    TextFont {
                        font: font.clone(),
                        font_size: 13.0,
                        ..default()
                    },
                    TextColor(text_color),
                ));
            });
        });
}

fn spawn_dropdown<'a>(
    parent: &mut ChildSpawnerCommands<'a>,
    field: DropdownField,
    is_approach: bool,
    index: usize,
    current_value: &str,
    options: &[&str],
    font: &Handle<Font>,
    theme: &UiTheme,
) {
    let label = match field {
        DropdownField::To => "To State",
        DropdownField::Condition => "Condition",
    };
    parent
        .spawn(Node {
            width: Val::Percent(100.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(2.0),
            ..default()
        })
        .with_children(|col| {
            col.spawn((
                Text::new(label),
                TextFont {
                    font: font.clone(),
                    font_size: 11.0,
                    ..default()
                },
                TextColor(theme.text_muted),
            ));
            col.spawn((
                Button,
                DropdownTrigger {
                    is_approach,
                    index,
                    field,
                },
                Node {
                    width: Val::Percent(100.0),
                    padding: UiRect::axes(Val::Px(8.0), Val::Px(6.0)),
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::Center,
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BorderColor::all(Color::srgba(0.3, 0.4, 0.6, 0.3)),
                BackgroundColor(theme.panel_background_soft),
            ))
            .with_children(|trigger| {
                trigger.spawn((
                    DropdownTriggerLabel,
                    Text::new(if current_value.is_empty() {
                        "—"
                    } else {
                        current_value
                    }),
                    TextFont {
                        font: font.clone(),
                        font_size: 12.0,
                        ..default()
                    },
                    TextColor(theme.text_primary),
                    Pickable::IGNORE,
                ));
                trigger.spawn((
                    Text::new("▼"),
                    TextFont {
                        font: font.clone(),
                        font_size: 9.0,
                        ..default()
                    },
                    TextColor(theme.text_muted),
                    Pickable::IGNORE,
                ));
            });
            col.spawn((
                DropdownOptions {
                    is_approach,
                    index,
                    field,
                },
                Node {
                    display: Display::None,
                    width: Val::Percent(100.0),
                    flex_direction: FlexDirection::Column,
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BorderColor::all(Color::srgba(0.3, 0.4, 0.6, 0.3)),
                BackgroundColor(Color::srgba(0.047, 0.063, 0.110, 1.0)),
            ))
            .with_children(|opts| {
                for opt in options {
                    let selected = *opt == current_value;
                    opts.spawn((
                        Button,
                        DropdownOption {
                            is_approach,
                            index,
                            field,
                            value: opt.to_string(),
                        },
                        Node {
                            width: Val::Percent(100.0),
                            padding: UiRect::axes(Val::Px(10.0), Val::Px(6.0)),
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        BackgroundColor(if selected {
                            theme.button_background
                        } else {
                            Color::srgba(0.0, 0.0, 0.0, 0.0)
                        }),
                    ))
                    .with_child((
                        Text::new(*opt),
                        TextFont {
                            font: font.clone(),
                            font_size: 12.0,
                            ..default()
                        },
                        TextColor(if selected {
                            theme.button_text
                        } else {
                            theme.text_primary
                        }),
                        Pickable::IGNORE,
                    ));
                }
            });
        });
}

fn transition_rows<'a>(
    parent: &mut ChildSpawnerCommands<'a>,
    transitions: &[TransitionForm],
    is_approach: bool,
    dist_unit: &str,
    font: &Handle<Font>,
    theme: &UiTheme,
) {
    for (i, t) in transitions.iter().enumerate() {
        parent
            .spawn((
                Node {
                    width: Val::Percent(100.0),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(6.0),
                    padding: UiRect::all(Val::Px(10.0)),
                    border: UiRect::left(Val::Px(2.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.071, 0.102, 0.173, 0.4)),
                BorderColor::all(Color::srgba(0.38, 0.66, 0.99, 0.3)),
            ))
            .with_children(|row| {
                // Header row with "Transition N" + remove button
                row.spawn(Node {
                    width: Val::Percent(100.0),
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::Center,
                    ..default()
                })
                .with_children(|hdr| {
                    hdr.spawn((
                        Text::new(format!("Transition {}", i + 1)),
                        TextFont {
                            font: font.clone(),
                            font_size: 12.0,
                            ..default()
                        },
                        TextColor(theme.text_muted),
                    ));

                    if is_approach {
                        hdr.spawn((
                            Button,
                            RemoveApproachTransitionButton(i),
                            Node {
                                padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
                                ..default()
                            },
                            BackgroundColor(Color::srgba(0.6, 0.15, 0.15, 0.5)),
                        ))
                        .with_children(|btn| {
                            btn.spawn((
                                Text::new("×"),
                                TextFont {
                                    font: font.clone(),
                                    font_size: 13.0,
                                    ..default()
                                },
                                TextColor(Color::srgb(1.0, 0.65, 0.65)),
                            ));
                        });
                    } else {
                        hdr.spawn((
                            Button,
                            RemoveTerminalTransitionButton(i),
                            Node {
                                padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
                                ..default()
                            },
                            BackgroundColor(Color::srgba(0.6, 0.15, 0.15, 0.5)),
                        ))
                        .with_children(|btn| {
                            btn.spawn((
                                Text::new("×"),
                                TextFont {
                                    font: font.clone(),
                                    font_size: 13.0,
                                    ..default()
                                },
                                TextColor(Color::srgb(1.0, 0.65, 0.65)),
                            ));
                        });
                    }
                });

                // Fields: To, Kind, Value
                let val_id = if is_approach {
                    FormFieldId::ApproachTransitionDistanceValue(i)
                } else {
                    FormFieldId::TerminalTransitionDistanceValue(i)
                };

                spawn_dropdown(
                    row,
                    DropdownField::To,
                    is_approach,
                    i,
                    &t.to,
                    &["approach", "terminal", "capture"],
                    font,
                    theme,
                );

                spawn_dropdown(
                    row,
                    DropdownField::Condition,
                    is_approach,
                    i,
                    &t.distance_kind,
                    &["less_than", "greater_than"],
                    font,
                    theme,
                );

                field_row(
                    row,
                    &format!("Distance ({dist_unit})"),
                    val_id,
                    "50.0",
                    &t.distance_value,
                    true,
                    false,
                    font,
                    theme,
                );
            });
    }
}

// ── spawn / cleanup / interactions ────────────────────────────────────────

pub fn spawn_capture_plan_modal(
    commands: &mut Commands,
    asset_server: &AssetServer,
    theme: &UiTheme,
    form: &NewCapturePlanForm,
    render_layer: usize,
    initial_scroll_y: f32,
) {
    let font: Handle<Font> = asset_server.load("fonts/FiraMono-Medium.ttf");
    let has_errors = !form.validation_errors.is_empty();
    let (vel_unit, force_unit, dist_unit) = match form.unit_system {
        UnitSystem::Metric => ("m/s", "N", "m"),
        UnitSystem::Imperial => ("ft/s", "lbf", "ft"),
    };

    commands
        .spawn((
            CapturePlanModal,
            RenderLayers::layer(render_layer),
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
        ))
        .with_children(|overlay| {
            overlay
                .spawn((
                    Node {
                        width: Val::Px(680.0),
                        max_width: Val::Percent(94.0),
                        max_height: Val::Percent(88.0),
                        flex_direction: FlexDirection::Column,
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.059, 0.078, 0.133, 1.0)),
                ))
                .with_children(|panel| {
                    // ── Title bar ────────────────────────────────────────
                    panel
                        .spawn((
                            Node {
                                width: Val::Percent(100.0),
                                justify_content: JustifyContent::SpaceBetween,
                                align_items: AlignItems::Center,
                                padding: UiRect::all(Val::Px(20.0)),
                                ..default()
                            },
                            BackgroundColor(theme.header_background),
                        ))
                        .with_children(|bar| {
                            let form_title = if form.read_only {
                                "View Capture Plan"
                            } else if form.editing_plan_id.is_some() {
                                "Edit Capture Plan"
                            } else {
                                "New Capture Plan"
                            };
                            bar.spawn((
                                Text::new(form_title),
                                TextFont {
                                    font: font.clone(),
                                    font_size: 20.0,
                                    ..default()
                                },
                                TextColor(theme.text_primary),
                            ));

                            bar.spawn(Node {
                                align_items: AlignItems::Center,
                                column_gap: Val::Px(10.0),
                                ..default()
                            })
                            .with_children(|right| {
                                if !form.read_only {
                                    right.spawn((
                                        Button,
                                        NewPlanSaveButton,
                                        Node {
                                            padding: UiRect::axes(Val::Px(14.0), Val::Px(7.0)),
                                            ..default()
                                        },
                                        BackgroundColor(theme.button_background),
                                    ))
                                    .with_children(|btn| {
                                        btn.spawn((
                                            Text::new("Save"),
                                            TextFont {
                                                font: font.clone(),
                                                font_size: 13.0,
                                                ..default()
                                            },
                                            TextColor(theme.button_text),
                                        ));
                                    });
                                }
                                right.spawn((
                                    Button,
                                    NewPlanCancelButton,
                                    Node {
                                        padding: UiRect::axes(Val::Px(14.0), Val::Px(7.0)),
                                        ..default()
                                    },
                                    BackgroundColor(theme.panel_background),
                                ))
                                .with_children(|btn| {
                                    btn.spawn((
                                        Text::new("× Cancel"),
                                        TextFont {
                                            font: font.clone(),
                                            font_size: 13.0,
                                            ..default()
                                        },
                                        TextColor(theme.text_muted),
                                    ));
                                });
                            });
                        });

                    // ── Scroll body ──────────────────────────────────────
                    panel
                        .spawn(Node {
                            width: Val::Percent(100.0),
                            flex_grow: 1.0,
                            min_height: Val::Px(0.0),
                            flex_direction: FlexDirection::Row,
                            ..default()
                        })
                        .with_children(|scroll_row| {
                            let scroll_id = scroll_row
                                .spawn((
                                    CapturePlanScrollBody,
                                    Interaction::default(),
                                    ScrollPosition(Vec2::new(0.0, initial_scroll_y)),
                                    Node {
                                        flex_grow: 1.0,
                                        min_height: Val::Px(0.0),
                                        flex_direction: FlexDirection::Column,
                                        overflow: Overflow::scroll_y(),
                                        scrollbar_width: 8.0,
                                        ..default()
                                    },
                                ))
                                .observe(
                                    |mut ev: On<Pointer<Scroll>>,
                                     mut scroll_query: Query<&mut ScrollPosition>,
                                     computed_nodes: Query<&ComputedNode>,
                                     children_query: Query<&Children>,
                                     wrapper_query: Query<(), With<ScrollContentWrapper>>| {
                                        ev.propagate(false);
                                        let scroll_amount = match ev.event.unit {
                                            MouseScrollUnit::Line => ev.event.y * 24.0,
                                            MouseScrollUnit::Pixel => ev.event.y,
                                        };
                                        if let Ok(mut scroll_pos) = scroll_query.get_mut(ev.entity)
                                        {
                                            scroll_pos.0.y =
                                                (scroll_pos.0.y - scroll_amount).max(0.0);
                                            if let Ok(container) = computed_nodes.get(ev.entity) {
                                                let content_height = children_query
                                                    .get(ev.entity)
                                                    .ok()
                                                    .and_then(|ch| {
                                                        ch.iter().find(|c| {
                                                            wrapper_query.contains(*c)
                                                        })
                                                    })
                                                    .and_then(|w| computed_nodes.get(w).ok())
                                                    .map(|n| n.size().y)
                                                    .unwrap_or(0.0);
                                                let max_scroll =
                                                    (content_height - container.size().y).max(0.0);
                                                scroll_pos.0.y =
                                                    scroll_pos.0.y.min(max_scroll);
                                            }
                                        }
                                    },
                                )
                                .with_children(|scroll_viewport| {
                                    scroll_viewport
                                        .spawn((
                                            ScrollContentWrapper,
                                            Node {
                                                width: Val::Percent(100.0),
                                                flex_direction: FlexDirection::Column,
                                                row_gap: Val::Px(20.0),
                                                padding: UiRect::all(Val::Px(20.0)),
                                                ..default()
                                            },
                                        ))
                                        .with_children(|body| {
                            // ── Validation errors ────────────────────────
                            if has_errors {
                                body.spawn((
                                    Node {
                                        width: Val::Percent(100.0),
                                        flex_direction: FlexDirection::Column,
                                        row_gap: Val::Px(4.0),
                                        padding: UiRect::all(Val::Px(12.0)),
                                        ..default()
                                    },
                                    BackgroundColor(Color::srgba(0.6, 0.1, 0.1, 0.5)),
                                ))
                                .with_children(|err_box| {
                                    err_box.spawn((
                                        Text::new("Please fix the following errors:"),
                                        TextFont {
                                            font: font.clone(),
                                            font_size: 12.0,
                                            ..default()
                                        },
                                        TextColor(Color::srgb(1.0, 0.7, 0.7)),
                                    ));
                                    for err in &form.validation_errors {
                                        err_box.spawn((
                                            Text::new(format!("• {}", err)),
                                            TextFont {
                                                font: font.clone(),
                                                font_size: 12.0,
                                                ..default()
                                            },
                                            TextColor(Color::srgb(1.0, 0.7, 0.7)),
                                        ));
                                    }
                                });
                            }

                            // ── UNITS ─────────────────────────────────────
                            body.spawn(Node {
                                width: Val::Percent(100.0),
                                flex_direction: FlexDirection::Column,
                                row_gap: Val::Px(8.0),
                                ..default()
                            })
                            .with_children(|sec| {
                                sec.spawn((
                                    Text::new("Units"),
                                    TextFont { font: font.clone(), font_size: 11.0, ..default() },
                                    TextColor(theme.text_muted),
                                ));
                                sec.spawn(Node {
                                    flex_direction: FlexDirection::Row,
                                    column_gap: Val::Px(6.0),
                                    ..default()
                                })
                                .with_children(|row| {
                                    let metric_bg = if form.unit_system == UnitSystem::Metric {
                                        theme.button_background
                                    } else {
                                        theme.panel_background_soft
                                    };
                                    let imperial_bg = if form.unit_system == UnitSystem::Imperial {
                                        theme.button_background
                                    } else {
                                        theme.panel_background_soft
                                    };
                                    let metric_text = if form.unit_system == UnitSystem::Metric {
                                        theme.button_text
                                    } else {
                                        theme.text_muted
                                    };
                                    let imperial_text = if form.unit_system == UnitSystem::Imperial {
                                        theme.button_text
                                    } else {
                                        theme.text_muted
                                    };
                                    row.spawn((
                                        Button,
                                        UnitMetricButton,
                                        Node {
                                            padding: UiRect::axes(Val::Px(14.0), Val::Px(6.0)),
                                            ..default()
                                        },
                                        BackgroundColor(metric_bg),
                                    ))
                                    .with_children(|btn| {
                                        btn.spawn((
                                            Text::new("m (metric)"),
                                            TextFont { font: font.clone(), font_size: 12.0, ..default() },
                                            TextColor(metric_text),
                                        ));
                                    });
                                    row.spawn((
                                        Button,
                                        UnitImperialButton,
                                        Node {
                                            padding: UiRect::axes(Val::Px(14.0), Val::Px(6.0)),
                                            ..default()
                                        },
                                        BackgroundColor(imperial_bg),
                                    ))
                                    .with_children(|btn| {
                                        btn.spawn((
                                            Text::new("ft (imperial)"),
                                            TextFont { font: font.clone(), font_size: 12.0, ..default() },
                                            TextColor(imperial_text),
                                        ));
                                    });
                                });
                            });

                            // ── GENERAL ──────────────────────────────────
                            body.spawn(Node {
                                width: Val::Percent(100.0),
                                flex_direction: FlexDirection::Column,
                                row_gap: Val::Px(10.0),
                                ..default()
                            })
                            .with_children(|sec| {
                                section_header(sec, "General", &font, theme);
                                field_row(sec, "Plan Name *", FormFieldId::PlanName, "My Capture Plan", &form.plan_name, false, false, &font, theme);
                            });

                            // ── TETHER ───────────────────────────────────
                            body.spawn(Node {
                                width: Val::Percent(100.0),
                                flex_direction: FlexDirection::Column,
                                row_gap: Val::Px(10.0),
                                ..default()
                            })
                            .with_children(|sec| {
                                section_header(sec, "Tether", &font, theme);
                                field_row(sec, "Tether Name *", FormFieldId::TetherName, "Tether1", &form.tether_name, false, false, &font, theme);

                                // Tether Type radio select
                                sec.spawn(Node {
                                    width: Val::Percent(100.0),
                                    flex_direction: FlexDirection::Column,
                                    row_gap: Val::Px(4.0),
                                    ..default()
                                })
                                .with_children(|col| {
                                    col.spawn((
                                        Text::new("Tether Type *"),
                                        TextFont {
                                            font: font.clone(),
                                            font_size: 11.0,
                                            ..default()
                                        },
                                        TextColor(theme.text_muted),
                                    ));
                                    col.spawn(Node {
                                        flex_direction: FlexDirection::Row,
                                        column_gap: Val::Px(6.0),
                                        ..default()
                                    })
                                    .with_children(|row| {
                                        let tether_bg = if form.tether_type == "tether" {
                                            theme.button_background
                                        } else {
                                            theme.panel_background_soft
                                        };
                                        let tether_text = if form.tether_type == "tether" {
                                            theme.button_text
                                        } else {
                                            theme.text_muted
                                        };
                                        row.spawn((
                                            Button,
                                            TetherTypeRadioButton("tether".to_string()),
                                            Node {
                                                padding: UiRect::axes(Val::Px(14.0), Val::Px(6.0)),
                                                ..default()
                                            },
                                            BackgroundColor(tether_bg),
                                        ))
                                        .with_children(|btn| {
                                            btn.spawn((
                                                Text::new("Tether"),
                                                TextFont { font: font.clone(), font_size: 12.0, ..default() },
                                                TextColor(tether_text),
                                            ));
                                        });
                                        // Net option — disabled, coming soon
                                        row.spawn((
                                            Node {
                                                padding: UiRect::axes(Val::Px(14.0), Val::Px(6.0)),
                                                ..default()
                                            },
                                            BackgroundColor(Color::srgba(0.071, 0.102, 0.173, 0.3)),
                                        ))
                                        .with_children(|btn| {
                                            btn.spawn((
                                                Text::new("Net (coming soon)"),
                                                TextFont { font: font.clone(), font_size: 12.0, ..default() },
                                                TextColor(Color::srgba(0.60, 0.66, 0.78, 0.4)),
                                            ));
                                        });
                                    });
                                });

                                field_row(sec, "Tether Length * (m)", FormFieldId::TetherLength, "20.0", &form.tether_length, true, false, &font, theme);
                            });

                            // ── APPROACH STATE ───────────────────────────
                            body.spawn(Node {
                                width: Val::Percent(100.0),
                                flex_direction: FlexDirection::Column,
                                row_gap: Val::Px(10.0),
                                ..default()
                            })
                            .with_children(|sec| {
                                section_header(sec, "Approach State", &font, theme);
                                field_row(sec, &format!("Max Velocity * ({vel_unit})"), FormFieldId::ApproachMaxVelocity, "1.0", &form.approach_max_velocity, true, false, &font, theme);
                                field_row(sec, &format!("Max Force * ({force_unit})"), FormFieldId::ApproachMaxForce, "2.0", &form.approach_max_force, true, false, &font, theme);

                                sec.spawn((
                                    Text::new("Transitions"),
                                    TextFont { font: font.clone(), font_size: 12.0, ..default() },
                                    TextColor(theme.text_muted),
                                ));
                                transition_rows(sec, &form.approach_transitions, true, dist_unit, &font, theme);

                                sec.spawn((
                                    Button,
                                    AddApproachTransitionButton,
                                    Node {
                                        padding: UiRect::axes(Val::Px(12.0), Val::Px(6.0)),
                                        align_self: AlignSelf::Start,
                                        ..default()
                                    },
                                    BackgroundColor(theme.panel_background_soft),
                                ))
                                .with_children(|btn| {
                                    btn.spawn((
                                        Text::new("+ Add Transition"),
                                        TextFont { font: font.clone(), font_size: 12.0, ..default() },
                                        TextColor(theme.text_accent),
                                    ));
                                });
                            });

                            // ── TERMINAL STATE ───────────────────────────
                            body.spawn(Node {
                                width: Val::Percent(100.0),
                                flex_direction: FlexDirection::Column,
                                row_gap: Val::Px(10.0),
                                ..default()
                            })
                            .with_children(|sec| {
                                section_header(sec, "Terminal State", &font, theme);
                                field_row(sec, &format!("Max Velocity * ({vel_unit})"), FormFieldId::TerminalMaxVelocity, "0.2", &form.terminal_max_velocity, true, false, &font, theme);
                                field_row(sec, &format!("Max Force * ({force_unit})"), FormFieldId::TerminalMaxForce, "2.0", &form.terminal_max_force, true, false, &font, theme);
                                field_row(sec, &format!("Shrink Rate * ({vel_unit})"), FormFieldId::TerminalShrinkRate, "0.125", &form.terminal_shrink_rate, true, false, &font, theme);

                                sec.spawn((
                                    Text::new("Transitions"),
                                    TextFont { font: font.clone(), font_size: 12.0, ..default() },
                                    TextColor(theme.text_muted),
                                ));
                                transition_rows(sec, &form.terminal_transitions, false, dist_unit, &font, theme);

                                sec.spawn((
                                    Button,
                                    AddTerminalTransitionButton,
                                    Node {
                                        padding: UiRect::axes(Val::Px(12.0), Val::Px(6.0)),
                                        align_self: AlignSelf::Start,
                                        ..default()
                                    },
                                    BackgroundColor(theme.panel_background_soft),
                                ))
                                .with_children(|btn| {
                                    btn.spawn((
                                        Text::new("+ Add Transition"),
                                        TextFont { font: font.clone(), font_size: 12.0, ..default() },
                                        TextColor(theme.text_accent),
                                    ));
                                });
                            });

                            // ── CAPTURE STATE ────────────────────────────
                            body.spawn(Node {
                                width: Val::Percent(100.0),
                                flex_direction: FlexDirection::Column,
                                row_gap: Val::Px(10.0),
                                ..default()
                            })
                            .with_children(|sec| {
                                section_header(sec, "Capture State", &font, theme);
                                field_row(sec, &format!("Max Velocity * ({vel_unit})"), FormFieldId::CaptureMaxVelocity, "0.1", &form.capture_max_velocity, true, false, &font, theme);
                                field_row(sec, &format!("Max Force * ({force_unit})"), FormFieldId::CaptureMaxForce, "2.0", &form.capture_max_force, true, false, &font, theme);
                                field_row(sec, &format!("Shrink Rate * ({vel_unit})"), FormFieldId::CaptureShrinkRate, "0.025", &form.capture_shrink_rate, true, false, &font, theme);
                            });

                                }); // close with_children(|body|
                                }) // close with_children(|scroll_viewport|
                                .id();

                            scroll_row
                                .spawn((
                                    Scrollbar::new(
                                        scroll_id,
                                        ControlOrientation::Vertical,
                                        20.0,
                                    ),
                                    Node {
                                        width: Val::Px(8.0),
                                        ..default()
                                    },
                                    BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.05)),
                                ))
                                .with_child((
                                    CoreScrollbarThumb,
                                    Node {
                                        width: Val::Percent(100.0),
                                        ..default()
                                    },
                                    BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.3)),
                                ));
                        });

                    // ── Overwrite confirmation sub-modal ─────────────────
                    if let Some(conflict_path) = &form.overwrite_conflict_path {
                        panel
                            .spawn((
                                Node {
                                    position_type: PositionType::Absolute,
                                    width: Val::Percent(100.0),
                                    height: Val::Percent(100.0),
                                    justify_content: JustifyContent::Center,
                                    align_items: AlignItems::Center,
                                    ..default()
                                },
                                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.5)),
                                ZIndex(20),
                            ))
                            .with_children(|overlay| {
                                overlay
                                    .spawn((
                                        Node {
                                            width: Val::Px(460.0),
                                            max_width: Val::Percent(90.0),
                                            flex_direction: FlexDirection::Column,
                                            row_gap: Val::Px(14.0),
                                            padding: UiRect::all(Val::Px(24.0)),
                                            ..default()
                                        },
                                        BackgroundColor(theme.panel_background),
                                    ))
                                    .with_children(|dlg| {
                                        dlg.spawn((
                                            Text::new("Overwrite Existing Plan?"),
                                            TextFont { font: font.clone(), font_size: 17.0, ..default() },
                                            TextColor(theme.text_primary),
                                        ));

                                        dlg.spawn((
                                            Text::new("A plan already exists at:"),
                                            TextFont { font: font.clone(), font_size: 12.0, ..default() },
                                            TextColor(theme.text_muted),
                                        ));

                                        dlg.spawn((
                                            Text::new(conflict_path.clone()),
                                            TextFont { font: font.clone(), font_size: 12.0, ..default() },
                                            TextColor(theme.text_accent),
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
                                                CancelOverwriteButton,
                                                Node {
                                                    padding: UiRect::axes(Val::Px(14.0), Val::Px(7.0)),
                                                    ..default()
                                                },
                                                BackgroundColor(theme.panel_background_soft),
                                            ))
                                            .with_children(|btn| {
                                                btn.spawn((
                                                    Text::new("Keep Original Name"),
                                                    TextFont { font: font.clone(), font_size: 13.0, ..default() },
                                                    TextColor(theme.text_primary),
                                                ));
                                            });

                                            btns.spawn((
                                                Button,
                                                ConfirmOverwriteButton,
                                                Node {
                                                    padding: UiRect::axes(Val::Px(14.0), Val::Px(7.0)),
                                                    ..default()
                                                },
                                                BackgroundColor(Color::srgb(0.7, 0.2, 0.2)),
                                            ))
                                            .with_children(|btn| {
                                                btn.spawn((
                                                    Text::new("Overwrite"),
                                                    TextFont { font: font.clone(), font_size: 13.0, ..default() },
                                                    TextColor(theme.button_text),
                                                ));
                                            });
                                        });
                                    });
                            });
                    }
                });
        });
}
