use bevy::camera::visibility::RenderLayers;
use bevy::ecs::hierarchy::ChildSpawnerCommands;
use bevy::ecs::observer::On;
use bevy::input::ButtonState;
use bevy::input::keyboard::KeyboardInput;
use bevy::input::mouse::MouseScrollUnit;
use bevy::picking::Pickable;
use bevy::picking::events::{Pointer, Scroll};
use bevy::prelude::*;
use bevy::ui_widgets::{ControlOrientation, CoreScrollbarThumb, Scrollbar};

use crate::components::user_interface::{
    CaptureGuidanceReadout, CaptureTelemetryReadout, OrbitLabel, TimeWarpReadout,
};
use crate::constants::{MAP_LAYER, UI_LAYER};
use crate::plugins::gpu_compute::{
    GpuComputeEpochOrigin, GpuElements, MAP_POINT_SCALE, eci_position_to_map,
    propagate_catalog_eci_state,
};
use crate::resources::capture_plan_form::{NewCapturePlanForm, SimPlanSyncState};
use crate::resources::capture_plans::CapturePlanLibrary;
use crate::resources::orbital_cache::OrbitalCache;
use crate::resources::space_catalog::{
    FilteredSpaceCatalogResults, SpaceCatalogUiState, SpaceObjectCatalog,
};
use crate::resources::working_directory::WorkingDirectory;
use crate::ui::events::UiEvent;
use crate::ui::state::SelectedProject;
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
pub struct TimeWarpIncreaseButton;

#[derive(Component)]
pub struct ToggleOriginButton;

#[derive(Component)]
pub struct CycleCameraButton;

#[derive(Component)]
pub struct SidebarPanel;

#[derive(Component)]
pub struct ExitSimConfirmModal;

#[derive(Component)]
pub struct ExitSimCancelButton;

#[derive(Component)]
pub struct ExitSimConfirmButton;

#[derive(Component)]
pub struct ViewEditPlanButton;

#[derive(Component)]
pub struct RestartPromptModal;

#[derive(Component)]
pub struct RestartSimButton;

#[derive(Component)]
pub struct DismissRestartButton;

#[derive(Component)]
pub struct RecompileLiveButton;

#[derive(Component)]
pub struct SyncIndicator;

#[derive(Component)]
pub struct CatalogToggleButton;

#[derive(Component)]
pub struct CatalogPanel;

#[derive(Component)]
pub struct CatalogSearchField;

#[derive(Component)]
pub struct CatalogSearchText;

#[derive(Component)]
pub struct CatalogResultsSummary;

#[derive(Component)]
pub struct CatalogResultsList;

#[derive(Component)]
pub struct CatalogPageLabel;

#[derive(Component)]
pub struct CatalogPrevPageButton;

#[derive(Component)]
pub struct CatalogNextPageButton;

#[derive(Component)]
pub struct CatalogPointsToggleButton;

#[derive(Component)]
pub struct CatalogResultButton {
    pub slot: usize,
    pub entry_index: Option<usize>,
}

#[derive(Component)]
pub struct CatalogResultText {
    pub slot: usize,
}

#[derive(Component)]
pub struct SelectedCatalogOverlay;

#[derive(Component)]
pub struct SelectedCatalogOverlayLabel;

const CATALOG_PAGE_SIZE: usize = 48;

pub fn spawn_project_detail_screen(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    theme: Res<UiTheme>,
    selected_project: Res<SelectedProject>,
    orbital_cache: Res<OrbitalCache>,
    capture_plan_lib: Res<CapturePlanLibrary>,
    working_directory: Res<WorkingDirectory>,
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
                                                row_gap: px(4.0),
                                                overflow: Overflow::scroll_y(),
                                                scrollbar_width: 8.0,
                                                ..default()
                                            },
                                        ))
                                        .observe(
                                            |mut ev: On<Pointer<Scroll>>,
                                             mut query: Query<&mut ScrollPosition>| {
                                                ev.propagate(false);
                                                let scroll_amount = match ev.event.unit {
                                                    MouseScrollUnit::Line => ev.event.y * 24.0,
                                                    MouseScrollUnit::Pixel => ev.event.y,
                                                };
                                                if let Ok(mut scroll_pos) = query.get_mut(ev.entity)
                                                {
                                                    scroll_pos.0.y -= scroll_amount;
                                                    scroll_pos.0.y = scroll_pos.0.y.max(0.0);
                                                }
                                            },
                                        )
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
                                        });

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
                    entity: tether_root_entity,
                },
                Text::new("─ Tether1"),
                TextFont {
                    font: font.clone(),
                    font_size: 14.0,
                    ..default()
                },
                TextColor(theme.text_primary),
                Node {
                    position_type: PositionType::Absolute,
                    ..default()
                },
            ));

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
    modals: Query<Entity, With<ExitSimConfirmModal>>,
) {
    for entity in &roots {
        commands.entity(entity).despawn();
    }
    for entity in &modals {
        commands.entity(entity).despawn();
    }
}

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
            RenderLayers::layer(crate::constants::UI_LAYER),
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
            RenderLayers::layer(crate::constants::UI_LAYER),
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

pub fn reset_space_catalog_ui_state(
    mut catalog_ui: ResMut<SpaceCatalogUiState>,
    mut filtered_results: ResMut<FilteredSpaceCatalogResults>,
) {
    *catalog_ui = SpaceCatalogUiState::default();
    filtered_results.0.clear();
}

pub fn refresh_space_catalog_results(
    catalog: Res<SpaceObjectCatalog>,
    mut catalog_ui: ResMut<SpaceCatalogUiState>,
    mut filtered_results: ResMut<FilteredSpaceCatalogResults>,
) {
    let should_refresh = catalog.is_changed() || catalog_ui.is_changed();
    if !should_refresh {
        return;
    }

    let search = catalog_ui.search_text.trim().to_lowercase();
    filtered_results.0.clear();
    filtered_results.0.reserve(catalog.entries.len());

    for (index, entry) in catalog.entries.iter().enumerate() {
        if search.is_empty() || entry.search_blob.contains(&search) {
            filtered_results.0.push(index);
        }
    }

    let max_page = filtered_results.0.len().saturating_sub(1) / CATALOG_PAGE_SIZE;
    catalog_ui.page = catalog_ui.page.min(max_page);

    if catalog_ui
        .selected_index
        .is_some_and(|selected_index| selected_index >= catalog.entries.len())
    {
        catalog_ui.selected_index = None;
    }
}

pub fn catalog_keyboard_input(
    mut keyboard_events: MessageReader<KeyboardInput>,
    filtered_results: Res<FilteredSpaceCatalogResults>,
    mut catalog_ui: ResMut<SpaceCatalogUiState>,
) {
    if !catalog_ui.search_focused {
        return;
    }

    let mut search_changed = false;

    for event in keyboard_events.read() {
        if event.state != ButtonState::Pressed || event.repeat {
            continue;
        }

        match event.key_code {
            KeyCode::Backspace => {
                search_changed |= catalog_ui.search_text.pop().is_some();
            }
            KeyCode::Escape => {
                catalog_ui.search_focused = false;
            }
            KeyCode::Enter | KeyCode::NumpadEnter => {
                let page_start = catalog_ui.page * CATALOG_PAGE_SIZE;
                if let Some(&entry_index) = filtered_results.0.get(page_start) {
                    catalog_ui.selected_index = Some(entry_index);
                    catalog_ui.search_focused = false;
                }
            }
            _ => {}
        }

        if let Some(text) = event.text.as_deref() {
            for character in text.chars().filter(|character| !character.is_control()) {
                catalog_ui.search_text.push(character);
                search_changed = true;
            }
        }
    }

    if search_changed {
        catalog_ui.page = 0;
    }
}

pub fn catalog_interactions(
    mut interactions: Query<
        (
            &Interaction,
            Option<&CatalogToggleButton>,
            Option<&CatalogSearchField>,
            Option<&CatalogPointsToggleButton>,
            Option<&CatalogPrevPageButton>,
            Option<&CatalogNextPageButton>,
            Option<&CatalogResultButton>,
            &mut BackgroundColor,
        ),
        (
            Changed<Interaction>,
            With<Button>,
            Or<(
                With<CatalogToggleButton>,
                With<CatalogSearchField>,
                With<CatalogPointsToggleButton>,
                With<CatalogPrevPageButton>,
                With<CatalogNextPageButton>,
                With<CatalogResultButton>,
            )>,
        ),
    >,
    filtered_results: Res<FilteredSpaceCatalogResults>,
    mut catalog_ui: ResMut<SpaceCatalogUiState>,
    theme: Res<UiTheme>,
) {
    for (
        interaction,
        toggle_button,
        search_field,
        points_toggle,
        prev_page,
        next_page,
        result_button,
        mut background_color,
    ) in &mut interactions
    {
        match *interaction {
            Interaction::Pressed => {
                *background_color = BackgroundColor(theme.button_background_hover);

                if search_field.is_some() {
                    catalog_ui.search_focused = true;
                    continue;
                }

                catalog_ui.search_focused = false;

                if toggle_button.is_some() {
                    catalog_ui.show_catalog = !catalog_ui.show_catalog;
                } else if points_toggle.is_some() {
                    catalog_ui.show_points = !catalog_ui.show_points;
                } else if prev_page.is_some() {
                    catalog_ui.page = catalog_ui.page.saturating_sub(1);
                } else if next_page.is_some() {
                    let max_page = filtered_results.0.len().saturating_sub(1) / CATALOG_PAGE_SIZE;
                    catalog_ui.page = (catalog_ui.page + 1).min(max_page);
                } else if let Some(result_button) = result_button {
                    if let Some(entry_index) = result_button.entry_index {
                        if catalog_ui.selected_index == Some(entry_index) {
                            catalog_ui.selected_index = None;
                        } else {
                            catalog_ui.selected_index = Some(entry_index);
                        }
                    }
                }
            }
            Interaction::Hovered => {
                *background_color = BackgroundColor(theme.button_background);
            }
            Interaction::None => {
                let base_color = if search_field.is_some() && catalog_ui.search_focused {
                    theme.button_background
                } else if let Some(result_button) = result_button {
                    if result_button.entry_index == catalog_ui.selected_index {
                        theme.button_background
                    } else {
                        theme.panel_background_soft
                    }
                } else {
                    theme.panel_background_soft
                };

                *background_color = BackgroundColor(base_color);
            }
        }
    }
}

pub fn sync_space_catalog_ui(
    camera: Single<&RenderLayers, With<Camera3d>>,
    catalog: Res<SpaceObjectCatalog>,
    filtered_results: Res<FilteredSpaceCatalogResults>,
    catalog_ui: Res<SpaceCatalogUiState>,
    theme: Res<UiTheme>,
    mut text_queries: ParamSet<(
        Query<&mut Text>,
        Query<&mut Text, With<CatalogResultsSummary>>,
        Query<&mut Text, With<CatalogPageLabel>>,
        Query<(&mut Text, &CatalogResultText)>,
    )>,
    mut text_colors: Query<&mut TextColor>,
    mut node_queries: ParamSet<(
        Query<(&mut Node, &Children), With<CatalogToggleButton>>,
        Query<&mut Node, (With<CatalogPanel>, Without<CatalogToggleButton>)>,
        Query<
            (
                &mut Node,
                &mut CatalogResultButton,
                &Interaction,
                &mut BackgroundColor,
            ),
            With<CatalogResultButton>,
        >,
    )>,
    search_texts: Query<Entity, With<CatalogSearchText>>,
    point_toggle_buttons: Query<&Children, With<CatalogPointsToggleButton>>,
) {
    let map_visible = camera.intersects(&RenderLayers::layer(MAP_LAYER));

    for (mut node, children) in &mut node_queries.p0() {
        node.display = if map_visible {
            Display::Flex
        } else {
            Display::None
        };

        {
            let mut texts = text_queries.p0();
            for child in children.iter() {
                if let Ok(mut text) = texts.get_mut(child) {
                    text.0 = if catalog_ui.show_catalog {
                        "Hide Catalog".to_string()
                    } else {
                        "Show Catalog".to_string()
                    };
                }
            }
        }
    }

    for mut node in &mut node_queries.p1() {
        node.display = if map_visible && catalog_ui.show_catalog {
            Display::Flex
        } else {
            Display::None
        };
    }

    for entity in &search_texts {
        {
            let mut texts = text_queries.p0();
            if let Ok(mut text) = texts.get_mut(entity) {
                let is_empty = catalog_ui.search_text.is_empty();
                text.0 = if is_empty {
                    "Search by name or catalog id".to_string()
                } else {
                    catalog_ui.search_text.clone()
                };
            }
        }

        if let Ok(mut color) = text_colors.get_mut(entity) {
            color.0 = if catalog_ui.search_text.is_empty() {
                theme.text_muted
            } else {
                theme.text_primary
            };
        }
    }

    let page_count = filtered_results
        .0
        .len()
        .saturating_add(CATALOG_PAGE_SIZE - 1)
        / CATALOG_PAGE_SIZE;
    let page_number = if page_count == 0 {
        0
    } else {
        catalog_ui.page + 1
    };
    let page_start = if filtered_results.0.is_empty() {
        0
    } else {
        catalog_ui.page * CATALOG_PAGE_SIZE + 1
    };
    let page_end =
        (catalog_ui.page * CATALOG_PAGE_SIZE + CATALOG_PAGE_SIZE).min(filtered_results.0.len());

    for mut text in &mut text_queries.p1() {
        text.0 = format!(
            "{} objects loaded · showing {}-{} of {}",
            catalog.entries.len(),
            page_start,
            page_end,
            filtered_results.0.len()
        );
    }

    for mut text in &mut text_queries.p2() {
        text.0 = if page_count == 0 {
            "Page 0 / 0".to_string()
        } else {
            format!("Page {} / {}", page_number, page_count)
        };
    }

    for children in &point_toggle_buttons {
        {
            let mut texts = text_queries.p0();
            for child in children.iter() {
                if let Ok(mut text) = texts.get_mut(child) {
                    text.0 = if catalog_ui.show_points {
                        "Hide Data Points".to_string()
                    } else {
                        "Show Data Points".to_string()
                    };
                }
            }
        }
    }

    for (mut node, mut row, interaction, mut background_color) in &mut node_queries.p2() {
        let slot_index = catalog_ui.page * CATALOG_PAGE_SIZE + row.slot;

        if let Some(&entry_index) = filtered_results.0.get(slot_index) {
            row.entry_index = Some(entry_index);
            node.display = Display::Flex;

            let entry = &catalog.entries[entry_index];
            let base_color = if *interaction == Interaction::Hovered {
                theme.button_background
            } else if Some(entry_index) == catalog_ui.selected_index {
                theme.button_background
            } else {
                theme.panel_background_soft
            };
            *background_color = BackgroundColor(base_color);

            for (mut text, label) in &mut text_queries.p3() {
                if label.slot == row.slot {
                    text.0 = entry.display_label();
                }
            }
        } else {
            row.entry_index = None;
            node.display = Display::None;

            for (mut text, label) in &mut text_queries.p3() {
                if label.slot == row.slot {
                    text.0.clear();
                }
            }
        }
    }
}

pub fn update_selected_catalog_overlay(
    camera: Single<(&Camera, &GlobalTransform, &RenderLayers), With<Camera3d>>,
    catalog: Res<SpaceObjectCatalog>,
    catalog_ui: Res<SpaceCatalogUiState>,
    gpu_elements: Res<GpuElements>,
    epoch_origin: Res<GpuComputeEpochOrigin>,
    world_time: Res<crate::resources::world_time::WorldTime>,
    overlay: Single<(&mut Node, &Children), With<SelectedCatalogOverlay>>,
    mut label_texts: Query<&mut Text, With<SelectedCatalogOverlayLabel>>,
) {
    let (camera, camera_transform, render_layers) = camera.into_inner();
    let (mut overlay_node, overlay_children) = overlay.into_inner();

    if !render_layers.intersects(&RenderLayers::layer(MAP_LAYER)) {
        overlay_node.display = Display::None;
        return;
    }

    let Some(selected_index) = catalog_ui.selected_index else {
        overlay_node.display = Display::None;
        return;
    };

    let Some(entry) = catalog.entries.get(selected_index) else {
        overlay_node.display = Display::None;
        return;
    };
    let Some(element) = gpu_elements.0.get(entry.gpu_index) else {
        overlay_node.display = Display::None;
        return;
    };

    let current_epoch_offset_seconds = epoch_origin
        .0
        .map_or(0.0, |origin| (world_time.epoch - origin) as f32);
    let Some((position_eci, _velocity_eci)) =
        propagate_catalog_eci_state(element, current_epoch_offset_seconds)
    else {
        overlay_node.display = Display::None;
        return;
    };

    let map_position = eci_position_to_map(position_eci);
    let half_extent = MAP_POINT_SCALE * 0.5;

    let corners = [
        map_position + Vec3::new(-half_extent, -half_extent, -half_extent),
        map_position + Vec3::new(-half_extent, -half_extent, half_extent),
        map_position + Vec3::new(-half_extent, half_extent, -half_extent),
        map_position + Vec3::new(-half_extent, half_extent, half_extent),
        map_position + Vec3::new(half_extent, -half_extent, -half_extent),
        map_position + Vec3::new(half_extent, -half_extent, half_extent),
        map_position + Vec3::new(half_extent, half_extent, -half_extent),
        map_position + Vec3::new(half_extent, half_extent, half_extent),
    ];

    let mut min = Vec2::splat(f32::INFINITY);
    let mut max = Vec2::splat(f32::NEG_INFINITY);
    let mut any_projected = false;

    for corner in corners {
        if let Ok(viewport_position) = camera.world_to_viewport(camera_transform, corner) {
            min = min.min(viewport_position);
            max = max.max(viewport_position);
            any_projected = true;
        }
    }

    if !any_projected {
        overlay_node.display = Display::None;
        return;
    }

    let padding = 6.0;
    let size = (max - min).max(Vec2::splat(18.0)) + Vec2::splat(padding * 2.0);

    overlay_node.display = Display::Flex;
    overlay_node.left = px(min.x - padding);
    overlay_node.top = px(min.y - padding);
    overlay_node.width = px(size.x);
    overlay_node.height = px(size.y);

    for child in overlay_children.iter() {
        if let Ok(mut text) = label_texts.get_mut(child) {
            text.0 = entry.display_label();
        }
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
            Option<&ExitSimCancelButton>,
            Option<&ExitSimConfirmButton>,
            &mut BackgroundColor,
        ),
        (Changed<Interaction>, With<Button>),
    >,
    mut commands: Commands,
    exit_modal_query: Query<Entity, With<ExitSimConfirmModal>>,
    mut events: MessageWriter<UiEvent>,
    screen: Res<State<crate::ui::state::UiScreen>>,
    theme: Res<UiTheme>,
    form: Res<NewCapturePlanForm>,
    restart_modal_query: Query<Entity, With<RestartPromptModal>>,
) {
    if *screen.get() != crate::ui::state::UiScreen::Sim {
        return;
    }

    let any_modal_open =
        form.open || !exit_modal_query.is_empty() || !restart_modal_query.is_empty();

    for (
        interaction,
        back_button,
        capture_button,
        map_view_button,
        toggle_origin_button,
        time_warp_decrease,
        time_warp_increase,
        cycle_camera_button,
        exit_cancel_button,
        exit_confirm_button,
        mut background_color,
    ) in &mut interactions
    {
        match *interaction {
            Interaction::Pressed => {
                *background_color = BackgroundColor(theme.button_background_hover);
                // Exit modal buttons are always allowed
                if exit_cancel_button.is_some() {
                    for entity in &exit_modal_query {
                        commands.entity(entity).despawn();
                    }
                    events.write(UiEvent::CancelExitConfirm);
                } else if exit_confirm_button.is_some() {
                    events.write(UiEvent::BackToHome);
                } else if any_modal_open {
                    // Block all other buttons when a modal is open
                } else if back_button.is_some() {
                    events.write(UiEvent::ShowExitConfirm);
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

pub fn view_edit_plan_interactions(
    mut buttons: Query<
        (&Interaction, &mut BackgroundColor),
        (Changed<Interaction>, With<Button>, With<ViewEditPlanButton>),
    >,
    mut events: MessageWriter<UiEvent>,
    selected_project: Res<SelectedProject>,
    theme: Res<UiTheme>,
    form: Res<NewCapturePlanForm>,
    exit_modal: Query<Entity, With<ExitSimConfirmModal>>,
    restart_modal: Query<Entity, With<RestartPromptModal>>,
) {
    let any_modal_open = form.open || !exit_modal.is_empty() || !restart_modal.is_empty();

    for (interaction, mut bg) in &mut buttons {
        match *interaction {
            Interaction::Pressed => {
                if !any_modal_open {
                    if let Some(plan_id) = &selected_project.project_id {
                        events.write(UiEvent::EditCapturePlan(plan_id.clone()));
                    }
                }
            }
            Interaction::Hovered => *bg = BackgroundColor(theme.button_background_hover),
            Interaction::None => *bg = BackgroundColor(theme.button_background),
        }
    }
}

pub fn restart_prompt_interactions(
    mut buttons: Query<
        (
            &Interaction,
            Option<&RestartSimButton>,
            Option<&DismissRestartButton>,
            Option<&RecompileLiveButton>,
            &mut BackgroundColor,
        ),
        (Changed<Interaction>, With<Button>),
    >,
    mut commands: Commands,
    modal_query: Query<Entity, With<RestartPromptModal>>,
    mut sync_state: ResMut<SimPlanSyncState>,
    mut next_screen: ResMut<NextState<crate::ui::state::UiScreen>>,
    mut capture_plan_lib: ResMut<CapturePlanLibrary>,
    theme: Res<UiTheme>,
) {
    for (interaction, restart_btn, dismiss_btn, recompile_btn, mut bg) in &mut buttons {
        if restart_btn.is_none() && dismiss_btn.is_none() && recompile_btn.is_none() {
            continue;
        }
        match *interaction {
            Interaction::Pressed => {
                // Despawn modal
                for entity in &modal_query {
                    commands.entity(entity).despawn();
                }
                if restart_btn.is_some() {
                    sync_state.in_sync = true;
                    sync_state.restart_requested = true;
                    // Transition Home → Sim to trigger full cleanup and re-setup
                    next_screen.set(crate::ui::state::UiScreen::Home);
                } else if recompile_btn.is_some() {
                    // Apply changes live without restarting the sim.
                    let refreshed: Vec<_> = capture_plan_lib
                        .plans
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect();
                    for (id, plan) in refreshed {
                        capture_plan_lib.insert_plan(id, plan);
                    }
                    sync_state.in_sync = true;
                } else if dismiss_btn.is_some() {
                    sync_state.in_sync = false;
                }
            }
            Interaction::Hovered => *bg = BackgroundColor(theme.button_background),
            Interaction::None => {
                if restart_btn.is_some() {
                    *bg = BackgroundColor(theme.button_background);
                } else {
                    *bg = BackgroundColor(theme.panel_background_soft);
                }
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
    form: Res<NewCapturePlanForm>,
    exit_modal: Query<Entity, With<ExitSimConfirmModal>>,
    restart_modal: Query<Entity, With<RestartPromptModal>>,
) {
    let any_modal_open = form.open || !exit_modal.is_empty() || !restart_modal.is_empty();
    if any_modal_open {
        return;
    }
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

pub fn update_sync_indicator(
    mut indicators: Query<&mut Node, With<SyncIndicator>>,
    sync_state: Res<SimPlanSyncState>,
) {
    for mut node in &mut indicators {
        node.display = if sync_state.in_sync {
            Display::None
        } else {
            Display::Flex
        };
    }
}
