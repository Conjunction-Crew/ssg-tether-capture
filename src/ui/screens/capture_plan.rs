use bevy::camera::visibility::RenderLayers;
use bevy::ecs::hierarchy::ChildSpawnerCommands;
use bevy::ecs::observer::On;
use bevy::input::mouse::MouseScrollUnit;
use bevy::picking::Pickable;
use bevy::picking::events::{Pointer, Scroll};
use bevy::prelude::*;
use bevy::ui_widgets::{ControlOrientation, CoreScrollbarThumb, Scrollbar};

use crate::resources::capture_plan_form::{NewCapturePlanForm, TransitionForm, UnitSystem};
use crate::ui::events::UiEvent;
use crate::ui::state::UiScreen;
use crate::ui::theme::UiTheme;
use crate::ui::widgets::{InputField, InputFieldText};

// ── Marker components ──────────────────────────────────────────────────────

#[derive(Component)]
pub struct CapturePlanModal;

#[derive(Component)]
pub struct NewPlanCancelButton;

#[derive(Component)]
pub struct NewPlanSaveButton;

#[derive(Component)]
pub struct AddApproachTransitionButton;

#[derive(Component)]
pub struct RemoveApproachTransitionButton(pub usize);

#[derive(Component)]
pub struct AddTerminalTransitionButton;

#[derive(Component)]
pub struct RemoveTerminalTransitionButton(pub usize);

#[derive(Component)]
pub struct ConfirmOverwriteButton;

#[derive(Component)]
pub struct CancelOverwriteButton;

#[derive(Component)]
pub struct CapturePlanScrollBody;

#[derive(Component)]
pub struct UnitMetricButton;

#[derive(Component)]
pub struct UnitImperialButton;

// ── Field ID tags so the keyboard system knows which field to update ──────

#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub enum FormFieldId {
    PlanName,
    TetherName,
    TetherLength,
    ApproachMaxVelocity,
    ApproachMaxForce,
    ApproachTransitionTo(usize),
    ApproachTransitionDistanceKind(usize),
    ApproachTransitionDistanceValue(usize),
    TerminalMaxVelocity,
    TerminalMaxForce,
    TerminalShrinkRate,
    TerminalTransitionTo(usize),
    TerminalTransitionDistanceKind(usize),
    TerminalTransitionDistanceValue(usize),
    CaptureMaxVelocity,
    CaptureMaxForce,
    CaptureShrinkRate,
}

#[derive(Component, Debug, Clone)]
pub struct TetherTypeRadioButton(pub String);

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
                let (to_id, kind_id, val_id) = if is_approach {
                    (
                        FormFieldId::ApproachTransitionTo(i),
                        FormFieldId::ApproachTransitionDistanceKind(i),
                        FormFieldId::ApproachTransitionDistanceValue(i),
                    )
                } else {
                    (
                        FormFieldId::TerminalTransitionTo(i),
                        FormFieldId::TerminalTransitionDistanceKind(i),
                        FormFieldId::TerminalTransitionDistanceValue(i),
                    )
                };

                field_row(
                    row,
                    "To State",
                    to_id,
                    "e.g. terminal",
                    &t.to,
                    false,
                    false,
                    font,
                    theme,
                );
                field_row(
                    row,
                    "Condition (less_than / greater_than)",
                    kind_id,
                    "less_than",
                    &t.distance_kind,
                    false,
                    false,
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
                                        row_gap: Val::Px(20.0),
                                        padding: UiRect::all(Val::Px(20.0)),
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
                                        if let Ok(mut scroll_pos) = query.get_mut(ev.entity) {
                                            scroll_pos.0.y = (scroll_pos.0.y - scroll_amount).max(0.0);
                                        }
                                    },
                                )
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

                                })
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

// ── Sync form fields from InputField components back into the resource ────

pub fn sync_form_fields(
    field_query: Query<(&InputField, &FormFieldId), Changed<InputField>>,
    mut form: ResMut<NewCapturePlanForm>,
) {
    for (field, id) in &field_query {
        match id {
            FormFieldId::PlanName => form.plan_name = field.value.clone(),
            FormFieldId::TetherName => form.tether_name = field.value.clone(),
            FormFieldId::TetherLength => form.tether_length = field.value.clone(),
            FormFieldId::ApproachMaxVelocity => form.approach_max_velocity = field.value.clone(),
            FormFieldId::ApproachMaxForce => form.approach_max_force = field.value.clone(),
            FormFieldId::TerminalMaxVelocity => form.terminal_max_velocity = field.value.clone(),
            FormFieldId::TerminalMaxForce => form.terminal_max_force = field.value.clone(),
            FormFieldId::TerminalShrinkRate => form.terminal_shrink_rate = field.value.clone(),
            FormFieldId::CaptureMaxVelocity => form.capture_max_velocity = field.value.clone(),
            FormFieldId::CaptureMaxForce => form.capture_max_force = field.value.clone(),
            FormFieldId::CaptureShrinkRate => form.capture_shrink_rate = field.value.clone(),
            FormFieldId::ApproachTransitionTo(i) => {
                if let Some(t) = form.approach_transitions.get_mut(*i) {
                    t.to = field.value.clone();
                }
            }
            FormFieldId::ApproachTransitionDistanceKind(i) => {
                if let Some(t) = form.approach_transitions.get_mut(*i) {
                    t.distance_kind = field.value.clone();
                }
            }
            FormFieldId::ApproachTransitionDistanceValue(i) => {
                if let Some(t) = form.approach_transitions.get_mut(*i) {
                    t.distance_value = field.value.clone();
                }
            }
            FormFieldId::TerminalTransitionTo(i) => {
                if let Some(t) = form.terminal_transitions.get_mut(*i) {
                    t.to = field.value.clone();
                }
            }
            FormFieldId::TerminalTransitionDistanceKind(i) => {
                if let Some(t) = form.terminal_transitions.get_mut(*i) {
                    t.distance_kind = field.value.clone();
                }
            }
            FormFieldId::TerminalTransitionDistanceValue(i) => {
                if let Some(t) = form.terminal_transitions.get_mut(*i) {
                    t.distance_value = field.value.clone();
                }
            }
        }
    }
}

// ── Button interaction system ─────────────────────────────────────────────

pub fn capture_plan_interactions(
    mut buttons: Query<
        (
            &Interaction,
            &mut BackgroundColor,
            Option<&NewPlanCancelButton>,
            Option<&NewPlanSaveButton>,
            Option<&AddApproachTransitionButton>,
            Option<&AddTerminalTransitionButton>,
            Option<&RemoveApproachTransitionButton>,
            Option<&RemoveTerminalTransitionButton>,
            Option<&ConfirmOverwriteButton>,
            Option<&CancelOverwriteButton>,
            Option<&UnitMetricButton>,
            Option<&UnitImperialButton>,
        ),
        (Changed<Interaction>, With<Button>),
    >,
    mut events: MessageWriter<UiEvent>,
    screen: Res<State<UiScreen>>,
    form: Res<NewCapturePlanForm>,
    theme: Res<UiTheme>,
) {
    if *screen.get() != UiScreen::Home && *screen.get() != UiScreen::Sim {
        return;
    }
    if !form.open {
        return;
    }

    for (
        interaction,
        mut bg,
        cancel,
        save,
        add_approach,
        add_terminal,
        remove_approach,
        remove_terminal,
        confirm_overwrite,
        cancel_overwrite,
        unit_metric,
        unit_imperial,
    ) in &mut buttons
    {
        if cancel.is_some() {
            match *interaction {
                Interaction::Pressed => {
                    events.write(UiEvent::CloseNewCapturePlanForm);
                }
                Interaction::Hovered => *bg = BackgroundColor(theme.panel_background_soft),
                Interaction::None => *bg = BackgroundColor(theme.panel_background),
            }
        } else if save.is_some() {
            match *interaction {
                Interaction::Pressed => {
                    events.write(UiEvent::SaveCapturePlan);
                }
                Interaction::Hovered => *bg = BackgroundColor(theme.button_background_hover),
                Interaction::None => *bg = BackgroundColor(theme.button_background),
            }
        } else if add_approach.is_some() {
            match *interaction {
                Interaction::Pressed => {
                    events.write(UiEvent::AddApproachTransition);
                }
                Interaction::Hovered => *bg = BackgroundColor(theme.panel_background),
                Interaction::None => *bg = BackgroundColor(theme.panel_background_soft),
            }
        } else if add_terminal.is_some() {
            match *interaction {
                Interaction::Pressed => {
                    events.write(UiEvent::AddTerminalTransition);
                }
                Interaction::Hovered => *bg = BackgroundColor(theme.panel_background),
                Interaction::None => *bg = BackgroundColor(theme.panel_background_soft),
            }
        } else if let Some(btn) = remove_approach {
            match *interaction {
                Interaction::Pressed => {
                    events.write(UiEvent::RemoveApproachTransition(btn.0));
                }
                Interaction::Hovered => *bg = BackgroundColor(Color::srgba(0.75, 0.2, 0.2, 0.65)),
                Interaction::None => *bg = BackgroundColor(Color::srgba(0.6, 0.15, 0.15, 0.5)),
            }
        } else if let Some(btn) = remove_terminal {
            match *interaction {
                Interaction::Pressed => {
                    events.write(UiEvent::RemoveTerminalTransition(btn.0));
                }
                Interaction::Hovered => *bg = BackgroundColor(Color::srgba(0.75, 0.2, 0.2, 0.65)),
                Interaction::None => *bg = BackgroundColor(Color::srgba(0.6, 0.15, 0.15, 0.5)),
            }
        } else if confirm_overwrite.is_some() {
            match *interaction {
                Interaction::Pressed => {
                    events.write(UiEvent::ConfirmOverwriteCapturePlan);
                }
                Interaction::Hovered => *bg = BackgroundColor(Color::srgb(0.8, 0.25, 0.25)),
                Interaction::None => *bg = BackgroundColor(Color::srgb(0.7, 0.2, 0.2)),
            }
        } else if cancel_overwrite.is_some() {
            match *interaction {
                Interaction::Pressed => {
                    events.write(UiEvent::CancelOverwriteCapturePlan);
                }
                Interaction::Hovered => *bg = BackgroundColor(theme.panel_background),
                Interaction::None => *bg = BackgroundColor(theme.panel_background_soft),
            }
        } else if unit_metric.is_some() {
            match *interaction {
                Interaction::Pressed => {
                    events.write(UiEvent::SetUnitSystem(UnitSystem::Metric));
                }
                Interaction::Hovered => *bg = BackgroundColor(theme.button_background_hover),
                Interaction::None => {}
            }
        } else if unit_imperial.is_some() {
            match *interaction {
                Interaction::Pressed => {
                    events.write(UiEvent::SetUnitSystem(UnitSystem::Imperial));
                }
                Interaction::Hovered => *bg = BackgroundColor(theme.button_background_hover),
                Interaction::None => {}
            }
        }
    }
}

pub fn tether_type_radio_interactions(
    mut buttons: Query<
        (&Interaction, &TetherTypeRadioButton, &mut BackgroundColor),
        (Changed<Interaction>, With<Button>),
    >,
    mut form: ResMut<NewCapturePlanForm>,
    theme: Res<UiTheme>,
) {
    if !form.open {
        return;
    }
    for (interaction, radio, mut bg) in &mut buttons {
        match *interaction {
            Interaction::Pressed => {
                form.tether_type = radio.0.clone();
            }
            Interaction::Hovered => *bg = BackgroundColor(theme.button_background_hover),
            Interaction::None => {
                if form.tether_type == radio.0 {
                    *bg = BackgroundColor(theme.button_background);
                } else {
                    *bg = BackgroundColor(theme.panel_background_soft);
                }
            }
        }
    }
}

// ── Validation and serialization helpers ─────────────────────────────────

pub fn validate_form(form: &NewCapturePlanForm) -> Vec<String> {
    let mut errors: Vec<String> = Vec::new();

    let require_text = |val: &str, label: &str, errs: &mut Vec<String>| {
        if val.trim().is_empty() {
            errs.push(format!("{} is required", label));
        }
    };
    let require_number = |val: &str, label: &str, errs: &mut Vec<String>| {
        if val.trim().is_empty() {
            errs.push(format!("{} is required", label));
        } else if val.parse::<f64>().is_err() {
            errs.push(format!("{} must be a number", label));
        }
    };

    require_text(&form.plan_name, "Plan Name", &mut errors);
    require_text(&form.tether_name, "Tether Name", &mut errors);

    if form.tether_length.trim().is_empty() {
        errors.push("Tether Length is required".to_string());
    } else {
        match form.tether_length.parse::<f64>() {
            Ok(v) if v > 0.0 => {}
            Ok(_) => errors.push("Tether Length must be greater than zero".to_string()),
            Err(_) => errors.push("Tether Length must be a number".to_string()),
        }
    }

    require_number(
        &form.approach_max_velocity,
        "Approach Max Velocity",
        &mut errors,
    );
    require_number(&form.approach_max_force, "Approach Max Force", &mut errors);
    require_number(
        &form.terminal_max_velocity,
        "Terminal Max Velocity",
        &mut errors,
    );
    require_number(&form.terminal_max_force, "Terminal Max Force", &mut errors);
    require_number(
        &form.terminal_shrink_rate,
        "Terminal Shrink Rate",
        &mut errors,
    );
    require_number(
        &form.capture_max_velocity,
        "Capture Max Velocity",
        &mut errors,
    );
    require_number(&form.capture_max_force, "Capture Max Force", &mut errors);
    require_number(
        &form.capture_shrink_rate,
        "Capture Shrink Rate",
        &mut errors,
    );

    for (i, t) in form.approach_transitions.iter().enumerate() {
        if t.to.trim().is_empty() {
            errors.push(format!(
                "Approach Transition {} 'To State' is required",
                i + 1
            ));
        }
        if t.distance_kind.trim().is_empty() {
            errors.push(format!(
                "Approach Transition {} condition is required",
                i + 1
            ));
        }
        if t.distance_value.trim().is_empty() {
            errors.push(format!(
                "Approach Transition {} distance value is required",
                i + 1
            ));
        } else if t.distance_value.parse::<f64>().is_err() {
            errors.push(format!(
                "Approach Transition {} distance value must be a number",
                i + 1
            ));
        }
    }

    for (i, t) in form.terminal_transitions.iter().enumerate() {
        if t.to.trim().is_empty() {
            errors.push(format!(
                "Terminal Transition {} 'To State' is required",
                i + 1
            ));
        }
        if t.distance_kind.trim().is_empty() {
            errors.push(format!(
                "Terminal Transition {} condition is required",
                i + 1
            ));
        }
        if t.distance_value.trim().is_empty() {
            errors.push(format!(
                "Terminal Transition {} distance value is required",
                i + 1
            ));
        } else if t.distance_value.parse::<f64>().is_err() {
            errors.push(format!(
                "Terminal Transition {} distance value must be a number",
                i + 1
            ));
        }
    }

    errors
}

pub fn generate_filename(plan_name: &str) -> String {
    // Characters illegal on Windows (superset of what POSIX disallows)
    const ILLEGAL: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*'];
    let sanitized: String = plan_name
        .to_lowercase()
        .chars()
        .map(|c| {
            if c == ' ' {
                '_'
            } else if ILLEGAL.contains(&c) || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    format!("{}.json", sanitized)
}

fn unit_conv_linear(s: &str, unit: UnitSystem) -> f64 {
    let v = s.parse::<f64>().unwrap_or(0.0);
    if unit == UnitSystem::Imperial {
        v * 0.3048
    } else {
        v
    }
}

fn unit_conv_force(s: &str, unit: UnitSystem) -> f64 {
    let v = s.parse::<f64>().unwrap_or(0.0);
    if unit == UnitSystem::Imperial {
        v * 4.44822
    } else {
        v
    }
}

pub fn build_capture_plan_json(form: &NewCapturePlanForm) -> serde_json::Value {
    use serde_json::{Value, json};

    let unit = form.unit_system;
    let make_transitions = |transitions: &[TransitionForm]| -> Value {
        let arr: Vec<Value> = transitions
            .iter()
            .map(|t| {
                let distance_val = unit_conv_linear(&t.distance_value, unit);
                let dist = json!({ t.distance_kind.clone(): distance_val });
                json!({ "to": t.to.trim(), "distance": dist })
            })
            .collect();
        Value::Array(arr)
    };

    let approach_transitions = make_transitions(&form.approach_transitions);
    let terminal_transitions = make_transitions(&form.terminal_transitions);

    json!({
        "name": form.plan_name.trim(),
        "tether": form.tether_name.trim(),
        "device": {
            "type": form.tether_type.trim(),
            "tether_length": form.tether_length.parse::<f64>().unwrap_or(20.0)
        },
        "states": [
            {
                "id": "approach",
                "parameters": {
                    "max_velocity": unit_conv_linear(&form.approach_max_velocity, unit),
                    "max_force": unit_conv_force(&form.approach_max_force, unit)
                },
                "transitions": approach_transitions
            },
            {
                "id": "terminal",
                "parameters": {
                    "max_velocity": unit_conv_linear(&form.terminal_max_velocity, unit),
                    "max_force": unit_conv_force(&form.terminal_max_force, unit),
                    "shrink_rate": unit_conv_linear(&form.terminal_shrink_rate, unit)
                },
                "transitions": terminal_transitions
            },
            {
                "id": "capture",
                "parameters": {
                    "max_velocity": unit_conv_linear(&form.capture_max_velocity, unit),
                    "max_force": unit_conv_force(&form.capture_max_force, unit),
                    "shrink_rate": unit_conv_linear(&form.capture_shrink_rate, unit)
                }
            }
        ]
    })
}
