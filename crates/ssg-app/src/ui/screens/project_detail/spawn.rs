use super::*;

pub fn spawn_project_detail_screen(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    theme: Res<UiTheme>,
    selected_project: Res<SelectedProject>,
    orbital_cache: Res<OrbitalCache>,
    capture_plan_lib: Res<CapturePlanLibrary>,
    working_directory: Res<WorkingDirectory>,
    sim_state: Res<State<SimState>>,
) {
    let font = asset_server.load("fonts/FiraMono-Medium.ttf");

    let plan_id = selected_project.project_id.as_deref().unwrap_or("");

    let plan = capture_plan_lib.plans.get(plan_id);

    let project_title = plan
        .map(|p| p.name.clone())
        .unwrap_or_else(|| "No plan selected".to_string());

    let project_description = String::new();

    let project_directory = working_directory.path.clone();

    let project_file = if plan_id.is_empty() {
        "Unknown file".to_string()
    } else {
        format!("{plan_id}.json")
    };

    let tether_name = plan.map(|p| p.tether.as_str()).unwrap_or("");
    let tether_root_entity: Option<Entity> = orbital_cache
        .tethers
        .get(tether_name)
        .and_then(|v| v.first().copied());
    let capture_target_entity = orbital_cache.debris.get("Satellite1").copied();
    let capture_target_label = String::from("Satellite1");
    let capture_plan_id = plan_id.to_string();

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

                        left.spawn((
                            Node {
                                position_type: PositionType::Absolute,
                                left: px(12.0),
                                top: px(44.0),
                                bottom: px(0.0),
                                width: px(340.0),
                                min_height: px(0.0),
                                flex_direction: FlexDirection::Column,
                                row_gap: px(8.0),
                                ..default()
                            },
                        ))
                        .with_children(|catalog| {
                            catalog
                                .spawn((
                                    Button,
                                    CatalogToggleButton,
                                    Node {
                                        width: px(156.0),
                                        min_height: px(38.0),
                                        align_items: AlignItems::Center,
                                        justify_content: JustifyContent::Center,
                                        ..default()
                                    },
                                    BackgroundColor(theme.panel_background),
                                ))
                                .with_children(|button| {
                                    button.spawn((
                                        Text::new("Hide Catalog"),
                                        TextFont {
                                            font: font.clone(),
                                            font_size: 13.0,
                                            ..default()
                                        },
                                        TextColor(theme.text_primary),
                                    ));
                                });

                            catalog
                                .spawn((
                                    CatalogPanel,
                                    Node {
                                        width: percent(100),
                                        flex_grow: 1.0,
                                        min_height: px(0.0),
                                        flex_direction: FlexDirection::Column,
                                        row_gap: px(8.0),
                                        padding: UiRect::all(px(12.0)),
                                        ..default()
                                    },
                                    BackgroundColor(theme.panel_background),
                                ))
                                .with_children(|panel| {
                                    panel.spawn((
                                        Text::new("Space Catalog"),
                                        TextFont {
                                            font: font.clone(),
                                            font_size: 17.0,
                                            ..default()
                                        },
                                        TextColor(theme.text_primary),
                                    ));

                                    panel
                                        .spawn((
                                            Button,
                                            CatalogSearchField,
                                            Node {
                                                width: percent(100),
                                                min_height: px(38.0),
                                                align_items: AlignItems::Center,
                                                padding: UiRect::axes(px(10.0), px(8.0)),
                                                ..default()
                                            },
                                            BackgroundColor(theme.panel_background_soft),
                                        ))
                                        .with_children(|search| {
                                            search.spawn((
                                                CatalogSearchText,
                                                Text::new("Search by name or catalog id"),
                                                TextFont {
                                                    font: font.clone(),
                                                    font_size: 12.0,
                                                    ..default()
                                                },
                                                TextColor(theme.text_muted),
                                            ));
                                        });

                                    panel
                                        .spawn(Node {
                                            width: percent(100),
                                            flex_direction: FlexDirection::Row,
                                            column_gap: px(8.0),
                                            ..default()
                                        })
                                        .with_children(|row| {
                                            row
                                                .spawn((
                                                    Button,
                                                    CatalogPointsToggleButton,
                                                    Node {
                                                        flex_grow: 1.0,
                                                        min_height: px(34.0),
                                                        align_items: AlignItems::Center,
                                                        justify_content: JustifyContent::Center,
                                                        ..default()
                                                    },
                                                    BackgroundColor(theme.panel_background_soft),
                                                ))
                                                .with_children(|button| {
                                                    button.spawn((
                                                        Text::new("Hide Data Points"),
                                                        TextFont {
                                                            font: font.clone(),
                                                            font_size: 12.0,
                                                            ..default()
                                                        },
                                                        TextColor(theme.text_primary),
                                                    ));
                                                });

                                        });

                                    panel.spawn((
                                        CatalogResultsSummary,
                                        Text::new("Loading catalog..."),
                                        TextFont {
                                            font: font.clone(),
                                            font_size: 11.0,
                                            ..default()
                                        },
                                        TextColor(theme.text_muted),
                                    ));

                                    panel
                                        .spawn((
                                            CatalogResultsList,
                                            Interaction::default(),
                                            ScrollPosition::default(),
                                            Node {
                                                width: percent(100),
                                                flex_grow: 1.0,
                                                min_height: px(0.0),
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
                                                if let Ok(mut scroll_pos) =
                                                    scroll_query.get_mut(ev.entity)
                                                {
                                                    scroll_pos.0.y -= scroll_amount;
                                                    scroll_pos.0.y = scroll_pos.0.y.max(0.0);
                                                    if let (Ok(container), Ok(children)) = (
                                                        computed_nodes.get(ev.entity),
                                                        children_query.get(ev.entity),
                                                    )
                                                        && let Some(wrapper_height) = children
                                                            .iter()
                                                            .find(|c| wrapper_query.contains(*c))
                                                            .and_then(|w| computed_nodes.get(w).ok())
                                                            .map(|n| n.size().y)
                                                        {
                                                            let max_scroll =
                                                                (wrapper_height - container.size().y)
                                                                    .max(0.0);
                                                            scroll_pos.0.y =
                                                                scroll_pos.0.y.min(max_scroll);
                                                        }
                                                }
                                            },
                                        )
                                        .with_children(|catalog_viewport| {
                                            catalog_viewport
                                                .spawn((
                                                    ScrollContentWrapper,
                                                    Node {
                                                        width: percent(100),
                                                        flex_direction: FlexDirection::Column,
                                                        row_gap: px(4.0),
                                                        ..default()
                                                    },
                                                ))
                                                .with_children(|results| {
                                            for slot in 0..CATALOG_PAGE_SIZE {
                                                results
                                                    .spawn((
                                                        Button,
                                                        CatalogResultButton {
                                                            slot,
                                                            entry_index: None,
                                                        },
                                                        Node {
                                                            width: percent(100),
                                                            min_height: px(28.0),
                                                            padding: UiRect::axes(
                                                                px(8.0),
                                                                px(6.0),
                                                            ),
                                                            align_items: AlignItems::Center,
                                                            ..default()
                                                        },
                                                        BackgroundColor(theme.panel_background_soft),
                                                    ))
                                                    .with_children(|button| {
                                                        button.spawn((
                                                            CatalogResultText { slot },
                                                            Text::new(""),
                                                            TextFont {
                                                                font: font.clone(),
                                                                font_size: 11.0,
                                                                ..default()
                                                            },
                                                            TextColor(theme.text_primary),
                                                        ));
                                                    });
                                            }
                                                }); // close ScrollContentWrapper with_children
                                        }); // close catalog_viewport with_children

                                    panel
                                        .spawn(Node {
                                            width: percent(100),
                                            flex_direction: FlexDirection::Row,
                                            align_items: AlignItems::Center,
                                            justify_content: JustifyContent::SpaceBetween,
                                            column_gap: px(8.0),
                                            margin: UiRect::top(px(4.0)),
                                            ..default()
                                        })
                                        .with_children(|pager| {
                                            pager
                                                .spawn((
                                                    Button,
                                                    CatalogPrevPageButton,
                                                    Node {
                                                        min_width: px(56.0),
                                                        min_height: px(32.0),
                                                        align_items: AlignItems::Center,
                                                        justify_content: JustifyContent::Center,
                                                        ..default()
                                                    },
                                                    BackgroundColor(theme.panel_background_soft),
                                                ))
                                                .with_children(|button| {
                                                    button.spawn((
                                                        Text::new("Prev"),
                                                        TextFont {
                                                            font: font.clone(),
                                                            font_size: 12.0,
                                                            ..default()
                                                        },
                                                        TextColor(theme.text_primary),
                                                    ));
                                                });

                                            pager.spawn((
                                                CatalogPageLabel,
                                                Text::new("Page 1"),
                                                TextFont {
                                                    font: font.clone(),
                                                    font_size: 12.0,
                                                    ..default()
                                                },
                                                TextColor(theme.text_primary),
                                            ));

                                            pager
                                                .spawn((
                                                    Button,
                                                    CatalogNextPageButton,
                                                    Node {
                                                        min_width: px(56.0),
                                                        min_height: px(32.0),
                                                        align_items: AlignItems::Center,
                                                        justify_content: JustifyContent::Center,
                                                        ..default()
                                                    },
                                                    BackgroundColor(theme.panel_background_soft),
                                                ))
                                                .with_children(|button| {
                                                    button.spawn((
                                                        Text::new("Next"),
                                                        TextFont {
                                                            font: font.clone(),
                                                            font_size: 12.0,
                                                            ..default()
                                                        },
                                                        TextColor(theme.text_primary),
                                                    ));
                                                });
                                        });
                                });
                        });
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
                                    overflow: Overflow::scroll_y(),
                                    scrollbar_width: 8.0,
                                    ..default()
                                },
                                BackgroundColor(theme.panel_background_soft),
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
                                    if let Ok(mut scroll_pos) = scroll_query.get_mut(ev.entity) {
                                        scroll_pos.0.y -= scroll_amount;
                                        scroll_pos.0.y = scroll_pos.0.y.max(0.0);
                                        if let (Ok(container), Ok(children)) = (
                                            computed_nodes.get(ev.entity),
                                            children_query.get(ev.entity),
                                        )
                                            && let Some(wrapper_height) = children
                                                .iter()
                                                .find(|c| wrapper_query.contains(*c))
                                                .and_then(|w| computed_nodes.get(w).ok())
                                                .map(|n| n.size().y)
                                            {
                                                let max_scroll =
                                                    (wrapper_height - container.size().y).max(0.0);
                                                scroll_pos.0.y = scroll_pos.0.y.min(max_scroll);
                                            }
                                    }
                                },
                            )
                            .with_children(|sidebar_viewport| {
                                sidebar_viewport
                                    .spawn((
                                        ScrollContentWrapper,
                                        Node {
                                            width: percent(100.0),
                                            flex_direction: FlexDirection::Column,
                                            row_gap: px(10.0),
                                            padding: UiRect::all(px(12.0)),
                                            ..default()
                                        },
                                    ))
                                    .with_children(|sidebar| {
                        // === Project Information (collapsible) ===
                        spawn_collapsible_section(
                            sidebar,
                            &font,
                            &theme,
                            "Project Information",
                            CollapsibleSection::ProjectInformation,
                            |content| {
                                // Out-of-sync indicator (hidden by default)
                                content
                                    .spawn((
                                        SyncIndicator,
                                        Node {
                                            flex_direction: FlexDirection::Row,
                                            align_items: AlignItems::Center,
                                            column_gap: Val::Px(6.0),
                                            display: Display::None,
                                            ..default()
                                        },
                                    ))
                                    .with_children(|row| {
                                        // Yellow dot
                                        row.spawn((
                                            Node {
                                                width: Val::Px(8.0),
                                                height: Val::Px(8.0),
                                                ..default()
                                            },
                                            BackgroundColor(Color::srgb(1.0, 0.85, 0.0)),
                                        ));
                                        row.spawn((
                                            Text::new("Plan changed — sim out of sync"),
                                            TextFont {
                                                font: font.clone(),
                                                font_size: 11.0,
                                                ..default()
                                            },
                                            TextColor(Color::srgb(1.0, 0.85, 0.0)),
                                        ));
                                        // Apply Live inline button
                                        row.spawn((
                                            RecompileLiveButton,
                                            Button,
                                            Node {
                                                padding: UiRect::axes(Val::Px(6.0), Val::Px(2.0)),
                                                ..default()
                                            },
                                            BackgroundColor(theme.panel_background_soft),
                                        ))
                                        .with_child((
                                            Text::new("Apply Live"),
                                            TextFont {
                                                font: font.clone(),
                                                font_size: 10.0,
                                                ..default()
                                            },
                                            TextColor(theme.text_accent),
                                            Pickable::IGNORE,
                                        ));
                                        // Reset Sim inline button
                                        row.spawn((
                                            RestartSimButton,
                                            Button,
                                            Node {
                                                padding: UiRect::axes(Val::Px(6.0), Val::Px(2.0)),
                                                ..default()
                                            },
                                            BackgroundColor(theme.button_background),
                                        ))
                                        .with_child((
                                            Text::new("Reset Sim"),
                                            TextFont {
                                                font: font.clone(),
                                                font_size: 10.0,
                                                ..default()
                                            },
                                            TextColor(theme.text_primary),
                                            Pickable::IGNORE,
                                        ));
                                    });

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

                                // View / Edit Plan button
                                let is_example_plan = selected_project.project_id.as_ref()
                                    .map(|id| !capture_plan_lib.user_plans.contains_key(id.as_str()))
                                    .unwrap_or(false);
                                let view_edit_label = if is_example_plan { "View Plan" } else { "View / Edit Plan" };
                                content.spawn((
                                    Button,
                                    ViewEditPlanButton,
                                    Node {
                                        padding: UiRect::axes(Val::Px(14.0), Val::Px(7.0)),
                                        margin: UiRect::top(Val::Px(6.0)),
                                        align_self: AlignSelf::Start,
                                        ..default()
                                    },
                                    BackgroundColor(theme.button_background),
                                ))
                                .with_children(|btn| {
                                    btn.spawn((
                                        Text::new(view_edit_label),
                                        TextFont {
                                            font: font.clone(),
                                            font_size: 12.0,
                                            ..default()
                                        },
                                        TextColor(theme.button_text),
                                    ));
                                });
                            },
                        );

                        if *sim_state.get() == SimState::Setup {
                            spawn_collapsible_section(
                                sidebar,
                                &font,
                                &theme,
                                "Orbital Selection",
                                CollapsibleSection::OrbitalSelection,
                                |content| {
                                    content.spawn((
                                        Text::new(
                                            "Pick a catalog object on the map, assign it to target or chaser, or create a custom orbit.",
                                        ),
                                        TextFont {
                                            font: font.clone(),
                                            font_size: 12.0,
                                            ..default()
                                        },
                                        TextColor(theme.text_muted),
                                    ));

                                    content.spawn((
                                        CurrentCatalogSelectionText,
                                        Text::new("Selected catalog object: none"),
                                        TextFont {
                                            font: font.clone(),
                                            font_size: 11.0,
                                            ..default()
                                        },
                                        TextColor(theme.text_primary),
                                    ));

                                    spawn_orbital_selection_role_editor(
                                        content,
                                        &font,
                                        &theme,
                                        OrbitalSelectionRole::Target,
                                    );
                                    spawn_orbital_selection_role_editor(
                                        content,
                                        &font,
                                        &theme,
                                        OrbitalSelectionRole::Chaser,
                                    );
                                },
                            );
                        }

                        // === Time Warp (collapsible) ===

                        if *sim_state.get() == SimState::Running {
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
                    }

                        // === Simulation Controls (collapsible) ===
                        spawn_collapsible_section(
                            sidebar,
                            &font,
                            &theme,
                            "Simulation Controls",
                            CollapsibleSection::SimulationControls,
                            |content| {
                                if *sim_state.get() == SimState::Setup {
                                // Start Sim button
                                content
                                    .spawn((
                                        Button,
                                        StartSimButton,
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
                                            Text::new("Start Simulation"),
                                            TextFont {
                                                font: font.clone(),
                                                font_size: 14.0,
                                                ..default()
                                            },
                                            TextColor(theme.text_primary),
                                        ));
                                    });
                                }

                                if *sim_state.get() == SimState::Running {
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

                                content
                                    .spawn((
                                        Button,
                                        SatelliteIndicatorToggleButton,
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
                                            Text::new("Hide Satellite Indicator"),
                                            TextFont {
                                                font: font.clone(),
                                                font_size: 14.0,
                                                ..default()
                                            },
                                            TextColor(theme.text_primary),
                                        ));
                                    });

                                // Toggle Capture Gizmos button
                                content
                                    .spawn((
                                        Button,
                                        ToggleCaptureGizmosButton,
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
                                            Text::new("Toggle Capture Gizmos (C)"),
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
                                            Text::new("Cycle Target (Tab)"),
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

                                // Reset Sim button
                                content
                                    .spawn((
                                        Button,
                                        RestartSimButton,
                                        Node {
                                            width: percent(100),
                                            min_height: px(40.0),
                                            align_items: AlignItems::Center,
                                            justify_content: JustifyContent::Center,
                                            ..default()
                                        },
                                        BackgroundColor(theme.button_background),
                                    ))
                                    .with_children(|btn| {
                                        btn.spawn((
                                            Text::new("Reset Sim"),
                                            TextFont {
                                                font: font.clone(),
                                                font_size: 14.0,
                                                ..default()
                                            },
                                            TextColor(theme.button_text),
                                        ));
                                    });
                                }
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
                    }); // close ScrollContentWrapper with_children
                    }) // close sidebar_viewport with_children
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
                    entity: tether_root_entity,
                },
                Node {
                    position_type: PositionType::Absolute,
                    display: Display::None,
                    border: UiRect::all(px(2.0)),
                    ..default()
                },
                BackgroundColor(Color::NONE),
                BorderColor::all(theme.text_primary),
            ))
            .with_children(|overlay| {
                overlay.spawn((
                    Text::new("Tether1"),
                    TextFont {
                        font: font.clone(),
                        font_size: 12.0,
                        ..default()
                    },
                    TextColor(theme.text_primary),
                    Node {
                        position_type: PositionType::Absolute,
                        top: px(-28.0),
                        left: px(0.0),
                        padding: UiRect::axes(px(8.0), px(4.0)),
                        ..default()
                    },
                    BackgroundColor(theme.panel_background),
                ));
            });

            root.spawn((
                SelectedCatalogOverlay,
                Node {
                    position_type: PositionType::Absolute,
                    display: Display::None,
                    border: UiRect::all(px(2.0)),
                    ..default()
                },
                BackgroundColor(Color::NONE),
                BorderColor::all(theme.text_accent),
            ))
            .with_children(|overlay| {
                overlay.spawn((
                    SelectedCatalogOverlayLabel,
                    Text::new(""),
                    TextFont {
                        font: font.clone(),
                        font_size: 12.0,
                        ..default()
                    },
                    TextColor(theme.text_primary),
                    Node {
                        position_type: PositionType::Absolute,
                        top: px(-28.0),
                        left: px(0.0),
                        padding: UiRect::axes(px(8.0), px(4.0)),
                        ..default()
                    },
                    BackgroundColor(theme.panel_background),
                ));
            });

            root.spawn((
                SatelliteIndicatorOverlay {
                    entity: capture_target_entity,
                    label: capture_target_label,
                },
                Node {
                    position_type: PositionType::Absolute,
                    display: Display::None,
                    border: UiRect::all(px(2.0)),
                    ..default()
                },
                BackgroundColor(Color::NONE),
                BorderColor::all(theme.text_primary),
            ))
            .with_children(|overlay| {
                overlay.spawn((
                    SatelliteIndicatorOverlayLabel,
                    Text::new(""),
                    TextFont {
                        font: font.clone(),
                        font_size: 12.0,
                        ..default()
                    },
                    TextColor(theme.text_primary),
                    Node {
                        position_type: PositionType::Absolute,
                        top: px(-28.0),
                        left: px(0.0),
                        padding: UiRect::axes(px(8.0), px(4.0)),
                        ..default()
                    },
                    BackgroundColor(theme.panel_background),
                ));
            });
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

fn spawn_orbital_selection_role_editor(
    parent: &mut ChildSpawnerCommands,
    font: &Handle<Font>,
    theme: &UiTheme,
    role: OrbitalSelectionRole,
) {
    parent
        .spawn((
            Node {
                width: percent(100),
                flex_direction: FlexDirection::Column,
                row_gap: px(8.0),
                padding: UiRect::all(px(10.0)),
                ..default()
            },
            BackgroundColor(theme.panel_background_soft),
        ))
        .with_children(|card| {
            card.spawn((
                Text::new(format!("{} Orbit", orbital_selection_role_label(role))),
                TextFont {
                    font: font.clone(),
                    font_size: 14.0,
                    ..default()
                },
                TextColor(theme.text_accent),
            ));

            card.spawn((
                OrbitalSelectionSummaryText { role },
                Text::new("No orbit selected"),
                TextFont {
                    font: font.clone(),
                    font_size: 11.0,
                    ..default()
                },
                TextColor(theme.text_primary),
            ));

            card.spawn(Node {
                width: percent(100),
                flex_direction: FlexDirection::Row,
                column_gap: px(8.0),
                ..default()
            })
            .with_children(|buttons| {
                buttons
                    .spawn((
                        Button,
                        UseCatalogSelectionButton { role },
                        Node {
                            flex_grow: 1.0,
                            min_height: px(34.0),
                            align_items: AlignItems::Center,
                            justify_content: JustifyContent::Center,
                            padding: UiRect::axes(px(8.0), px(6.0)),
                            ..default()
                        },
                        BackgroundColor(theme.panel_background),
                    ))
                    .with_children(|button| {
                        button.spawn((
                            Text::new("Use Selected"),
                            TextFont {
                                font: font.clone(),
                                font_size: 11.0,
                                ..default()
                            },
                            TextColor(theme.text_primary),
                        ));
                    });

                buttons
                    .spawn((
                        Button,
                        CreateCustomOrbitButton { role },
                        Node {
                            flex_grow: 1.0,
                            min_height: px(34.0),
                            align_items: AlignItems::Center,
                            justify_content: JustifyContent::Center,
                            padding: UiRect::axes(px(8.0), px(6.0)),
                            ..default()
                        },
                        BackgroundColor(theme.panel_background),
                    ))
                    .with_children(|button| {
                        button.spawn((
                            Text::new("Create Custom"),
                            TextFont {
                                font: font.clone(),
                                font_size: 11.0,
                                ..default()
                            },
                            TextColor(theme.text_primary),
                        ));
                    });
            });

            card.spawn((
                OrbitalSelectionSliderGroup { role },
                Node {
                    width: percent(100),
                    display: Display::None,
                    flex_direction: FlexDirection::Column,
                    row_gap: px(8.0),
                    ..default()
                },
            ))
            .with_children(|sliders| {
                for field in ORBITAL_ELEMENT_FIELDS {
                    spawn_orbital_element_slider(sliders, font, theme, role, field);
                }
            });
        });
}

fn spawn_orbital_element_slider(
    parent: &mut ChildSpawnerCommands,
    font: &Handle<Font>,
    theme: &UiTheme,
    role: OrbitalSelectionRole,
    field: OrbitalElementField,
) {
    let spec = orbital_element_slider_spec(field);
    parent
        .spawn(Node {
            width: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: px(4.0),
            ..default()
        })
        .with_children(|row| {
            row.spawn(Node {
                width: percent(100),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::SpaceBetween,
                column_gap: px(8.0),
                ..default()
            })
            .with_children(|header| {
                header.spawn((
                    Text::new(spec.label),
                    TextFont {
                        font: font.clone(),
                        font_size: 10.0,
                        ..default()
                    },
                    TextColor(theme.text_muted),
                ));

                header.spawn((
                    OrbitalElementSliderValueText { role, field },
                    Text::new("--"),
                    TextFont {
                        font: font.clone(),
                        font_size: 10.0,
                        ..default()
                    },
                    TextColor(theme.text_primary),
                ));
            });

            row.spawn((
                OrbitalElementSlider { role, field },
                Slider {
                    track_click: TrackClick::Drag,
                },
                SliderValue(spec.default_value),
                SliderRange::new(spec.min, spec.max),
                SliderStep(spec.step),
                SliderPrecision(spec.precision),
                Interaction::default(),
                Pickable::default(),
                Node {
                    width: percent(100),
                    height: px(20.0),
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                BackgroundColor(Color::NONE),
            ))
            .observe(orbital_element_slider_changed)
            .with_children(|slider| {
                slider.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(0.0),
                        top: px(8.0),
                        width: percent(100),
                        height: px(4.0),
                        border_radius: BorderRadius::MAX,
                        ..default()
                    },
                    BackgroundColor(theme.panel_background),
                    Pickable::IGNORE,
                ));

                slider.spawn((
                    OrbitalElementSliderFill { role, field },
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(0.0),
                        top: px(8.0),
                        width: percent(0),
                        height: px(4.0),
                        border_radius: BorderRadius::MAX,
                        ..default()
                    },
                    BackgroundColor(theme.button_background),
                    Pickable::IGNORE,
                ));

                slider.spawn((
                    SliderThumb,
                    OrbitalElementSliderThumb { role, field },
                    Node {
                        position_type: PositionType::Absolute,
                        left: percent(0),
                        top: px(3.0),
                        width: px(14.0),
                        height: px(14.0),
                        border: UiRect::all(px(2.0)),
                        border_radius: BorderRadius::MAX,
                        ..default()
                    },
                    BackgroundColor(theme.text_primary),
                    BorderColor::all(theme.button_background),
                    Pickable::IGNORE,
                ));
            });
        });
}

fn orbital_element_slider_changed(
    value_change: On<ValueChange<f32>>,
    mut commands: Commands,
    sliders: Query<&OrbitalElementSlider>,
    mut selection: ResMut<OrbitalSelectionState>,
) {
    let Ok(slider) = sliders.get(value_change.source) else {
        return;
    };

    if let Some(selected) = selected_orbital_object_mut(&mut selection, slider.role) {
        write_slider_value_to_elements(&mut selected.elements, slider.field, value_change.value);
        // Edited elements are no longer the catalog record (F-27).
        selected.gp = None;
    }

    commands
        .entity(value_change.source)
        .insert(SliderValue(value_change.value));
}

pub(super) struct OrbitalElementSliderSpec {
    label: &'static str,
    min: f32,
    max: f32,
    step: f32,
    precision: i32,
    pub(super) default_value: f32,
    suffix: &'static str,
}

pub(super) fn orbital_element_slider_spec(field: OrbitalElementField) -> OrbitalElementSliderSpec {
    match field {
        OrbitalElementField::SemiMajorAxis => OrbitalElementSliderSpec {
            label: "Semi-major axis",
            min: 6_400.0,
            max: 80_000.0,
            step: 10.0,
            precision: 0,
            default_value: (ISS_ORBIT[0] / 1_000.0) as f32,
            suffix: "km",
        },
        OrbitalElementField::Eccentricity => OrbitalElementSliderSpec {
            label: "Eccentricity",
            min: 0.0,
            max: 0.95,
            step: 0.001,
            precision: 4,
            default_value: ISS_ORBIT[1] as f32,
            suffix: "",
        },
        OrbitalElementField::Inclination => OrbitalElementSliderSpec {
            label: "Inclination",
            min: 0.0,
            max: 180.0,
            step: 0.1,
            precision: 2,
            default_value: (ISS_ORBIT[2] as f32).to_degrees(),
            suffix: "deg",
        },
        OrbitalElementField::Raan => OrbitalElementSliderSpec {
            label: "RAAN",
            min: 0.0,
            max: 360.0,
            step: 0.1,
            precision: 2,
            default_value: (ISS_ORBIT[3] as f32).to_degrees(),
            suffix: "deg",
        },
        OrbitalElementField::ArgPerigee => OrbitalElementSliderSpec {
            label: "Argument of perigee",
            min: 0.0,
            max: 360.0,
            step: 0.1,
            precision: 2,
            default_value: (ISS_ORBIT[4] as f32).to_degrees(),
            suffix: "deg",
        },
        OrbitalElementField::MeanAnomaly => OrbitalElementSliderSpec {
            label: "Mean anomaly",
            min: 0.0,
            max: 360.0,
            step: 0.1,
            precision: 2,
            default_value: (ISS_ORBIT[5] as f32).to_degrees(),
            suffix: "deg",
        },
    }
}

pub(super) fn selected_orbital_object(
    selection: Option<&OrbitalSelectionState>,
    role: OrbitalSelectionRole,
) -> Option<&SelectedOrbitalObject> {
    match role {
        OrbitalSelectionRole::Target => selection.and_then(|selection| selection.target.as_ref()),
        OrbitalSelectionRole::Chaser => selection.and_then(|selection| selection.chaser.as_ref()),
    }
}

fn selected_orbital_object_mut(
    selection: &mut OrbitalSelectionState,
    role: OrbitalSelectionRole,
) -> Option<&mut SelectedOrbitalObject> {
    match role {
        OrbitalSelectionRole::Target => selection.target.as_mut(),
        OrbitalSelectionRole::Chaser => selection.chaser.as_mut(),
    }
}

pub(super) fn orbital_selection_role_label(role: OrbitalSelectionRole) -> &'static str {
    match role {
        OrbitalSelectionRole::Target => "Target",
        OrbitalSelectionRole::Chaser => "Chaser",
    }
}

pub(super) fn orbital_selection_source_label(selected: &SelectedOrbitalObject) -> String {
    match &selected.source {
        OrbitalSelectionSource::Catalog { label, .. } => format!("Catalog: {label}"),
        OrbitalSelectionSource::Custom { label } => label.clone(),
    }
}

fn slider_value_from_elements(
    elements: &EditableOrbitalElements,
    field: OrbitalElementField,
) -> f32 {
    match field {
        OrbitalElementField::SemiMajorAxis => (elements.semi_major_axis_m / 1_000.0) as f32,
        OrbitalElementField::Eccentricity => elements.eccentricity as f32,
        OrbitalElementField::Inclination => elements.inclination_rad.to_degrees() as f32,
        OrbitalElementField::Raan => normalized_degrees(elements.raan_rad),
        OrbitalElementField::ArgPerigee => normalized_degrees(elements.arg_perigee_rad),
        OrbitalElementField::MeanAnomaly => normalized_degrees(elements.mean_anomaly_rad),
    }
}

pub(super) fn clamped_slider_value_from_elements(
    elements: &EditableOrbitalElements,
    field: OrbitalElementField,
) -> f32 {
    let spec = orbital_element_slider_spec(field);
    slider_value_from_elements(elements, field).clamp(spec.min, spec.max)
}

fn write_slider_value_to_elements(
    elements: &mut EditableOrbitalElements,
    field: OrbitalElementField,
    value: f32,
) {
    match field {
        OrbitalElementField::SemiMajorAxis => {
            elements.semi_major_axis_m = value as f64 * 1_000.0;
        }
        OrbitalElementField::Eccentricity => {
            elements.eccentricity = value as f64;
        }
        OrbitalElementField::Inclination => {
            elements.inclination_rad = (value as f64).to_radians();
        }
        OrbitalElementField::Raan => {
            elements.raan_rad = (value as f64).to_radians();
        }
        OrbitalElementField::ArgPerigee => {
            elements.arg_perigee_rad = (value as f64).to_radians();
        }
        OrbitalElementField::MeanAnomaly => {
            elements.mean_anomaly_rad = (value as f64).to_radians();
        }
    }
}

fn normalized_degrees(radians: f64) -> f32 {
    let mut degrees = radians.to_degrees() % 360.0;
    if degrees < 0.0 {
        degrees += 360.0;
    }
    degrees as f32
}

pub(super) fn slider_fraction(
    elements: &EditableOrbitalElements,
    field: OrbitalElementField,
) -> f32 {
    let spec = orbital_element_slider_spec(field);
    let value = slider_value_from_elements(elements, field);
    if spec.max <= spec.min {
        0.0
    } else {
        ((value - spec.min) / (spec.max - spec.min)).clamp(0.0, 1.0)
    }
}

pub(super) fn format_orbital_element_value(
    elements: &EditableOrbitalElements,
    field: OrbitalElementField,
) -> String {
    let spec = orbital_element_slider_spec(field);
    let value = slider_value_from_elements(elements, field);
    let precision = spec.precision.max(0) as usize;
    if spec.suffix.is_empty() {
        format!("{value:.precision$}")
    } else {
        format!("{value:.precision$} {}", spec.suffix)
    }
}

pub fn cleanup_project_detail_screen(
    mut commands: Commands,
    roots: Query<Entity, With<SimScreen>>,
    modals: Query<Entity, With<ExitSimConfirmModal>>,
) {
    for entity in &roots {
        commands.entity(entity).despawn();
    }
    for entity in &modals {
        commands.entity(entity).despawn();
    }
}
