use bevy::ecs::hierarchy::ChildSpawnerCommands;
use bevy::input::mouse::MouseScrollUnit;
use bevy::picking::Pickable;
use bevy::picking::events::{Pointer, Scroll};
use bevy::prelude::*;

use crate::resources::capture_log::{CaptureLog, CaptureLogUiState, LogEntry, LogLevel};
use crate::ui::theme::UiTheme;
use crate::ui::widgets::ClipboardRes;

// ─── Marker Components ────────────────────────────────────────────────────────

#[derive(Component)]
pub struct TerminalPanel;

#[derive(Component)]
pub struct TerminalLogViewport;

#[derive(Component)]
pub struct TerminalLogWrapper;

/// Marks each dynamically-spawned log row. `entry_index` is the row's position
/// in the currently-rendered filtered list.
#[derive(Component)]
pub struct TerminalLogRow {
    pub entry_index: usize,
}

#[derive(Component)]
pub struct TerminalToggleButton;

#[derive(Component)]
pub struct LogLevelFilterButton {
    pub level: LogLevel,
}

#[derive(Component)]
pub struct TerminalClearButton;

// ─── Spawn Helper ─────────────────────────────────────────────────────────────

/// Appends the capture-log terminal panel as a flex child of `parent`.
///
/// The viewport is hidden by default; the toggle button opens/closes it.
pub fn spawn_terminal_panel(
    parent: &mut ChildSpawnerCommands,
    font: &Handle<Font>,
    theme: &UiTheme,
) {
    parent
        .spawn((
            TerminalPanel,
            Node {
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                flex_shrink: 0.0,
                border: UiRect::top(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(theme.panel_background),
            BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.08)),
        ))
        .with_children(|panel| {
            // ── Header row ───────────────────────────────────────────────────
            panel
                .spawn(Node {
                    width: Val::Percent(100.0),
                    height: Val::Px(36.0),
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    padding: UiRect::axes(Val::Px(12.0), Val::Px(0.0)),
                    column_gap: Val::Px(6.0),
                    ..default()
                })
                .with_children(|header| {
                    // Title
                    header.spawn((
                        Text::new("Capture Log"),
                        TextFont {
                            font: font.clone(),
                            font_size: 12.0,
                            ..default()
                        },
                        TextColor(theme.text_accent),
                    ));

                    // Flexible spacer
                    header.spawn(Node {
                        flex_grow: 1.0,
                        ..default()
                    });

                    // Log-level filter toggle buttons (all active by default)
                    for (level, label) in [
                        (LogLevel::Error, "ERR"),
                        (LogLevel::Warn, "WARN"),
                        (LogLevel::Info, "INFO"),
                        (LogLevel::Debug, "DBG"),
                    ] {
                        header
                            .spawn((
                                Button,
                                LogLevelFilterButton { level },
                                Node {
                                    min_width: Val::Px(38.0),
                                    height: Val::Px(24.0),
                                    align_items: AlignItems::Center,
                                    justify_content: JustifyContent::Center,
                                    padding: UiRect::horizontal(Val::Px(4.0)),
                                    ..default()
                                },
                                BackgroundColor(theme.button_background),
                            ))
                            .with_child((
                                Text::new(label),
                                TextFont {
                                    font: font.clone(),
                                    font_size: 10.0,
                                    ..default()
                                },
                                TextColor(theme.button_text),
                                Pickable::IGNORE,
                            ));
                    }

                    // Clear button
                    header
                        .spawn((
                            Button,
                            TerminalClearButton,
                            Node {
                                min_width: Val::Px(48.0),
                                height: Val::Px(24.0),
                                align_items: AlignItems::Center,
                                justify_content: JustifyContent::Center,
                                ..default()
                            },
                            BackgroundColor(theme.panel_background_soft),
                        ))
                        .with_child((
                            Text::new("Clear"),
                            TextFont {
                                font: font.clone(),
                                font_size: 11.0,
                                ..default()
                            },
                            TextColor(theme.text_muted),
                            Pickable::IGNORE,
                        ));

                    // Toggle open/close button
                    header
                        .spawn((
                            Button,
                            TerminalToggleButton,
                            Node {
                                width: Val::Px(32.0),
                                height: Val::Px(24.0),
                                align_items: AlignItems::Center,
                                justify_content: JustifyContent::Center,
                                ..default()
                            },
                            BackgroundColor(theme.panel_background_soft),
                        ))
                        .with_child((
                            Text::new("^"),
                            TextFont {
                                font: font.clone(),
                                font_size: 11.0,
                                ..default()
                            },
                            TextColor(theme.text_muted),
                            Pickable::IGNORE,
                        ));
                });

            // ── Log viewport (hidden by default) ─────────────────────────────
            panel
                .spawn((
                    TerminalLogViewport,
                    Interaction::default(),
                    ScrollPosition::default(),
                    Node {
                        display: Display::None,
                        width: Val::Percent(100.0),
                        height: Val::Px(200.0),
                        flex_direction: FlexDirection::Column,
                        overflow: Overflow::scroll_y(),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.02, 0.04, 0.08, 0.97)),
                ))
                .observe(
                    |mut ev: On<Pointer<Scroll>>,
                     mut scroll_query: Query<&mut ScrollPosition, With<TerminalLogViewport>>,
                     computed_nodes: Query<&ComputedNode>,
                     children_query: Query<&Children>,
                     wrapper_query: Query<(), With<TerminalLogWrapper>>,
                     mut log_ui: ResMut<CaptureLogUiState>| {
                        ev.propagate(false);
                        let scroll_amount = match ev.event.unit {
                            MouseScrollUnit::Line => ev.event.y * 20.0,
                            MouseScrollUnit::Pixel => ev.event.y,
                        };
                        if let Ok(mut scroll_pos) = scroll_query.get_mut(ev.entity) {
                            scroll_pos.0.y -= scroll_amount;
                            scroll_pos.0.y = scroll_pos.0.y.max(0.0);
                            if let (Ok(container), Ok(children)) =
                                (computed_nodes.get(ev.entity), children_query.get(ev.entity))
                            {
                                if let Some(wrapper_height) = children
                                    .iter()
                                    .find(|c| wrapper_query.contains(*c))
                                    .and_then(|w| computed_nodes.get(w).ok())
                                    .map(|n| n.size().y)
                                {
                                    let max_scroll = (wrapper_height - container.size().y).max(0.0);
                                    scroll_pos.0.y = scroll_pos.0.y.min(max_scroll);

                                    // Scrolling up puts the user in manual control.
                                    if scroll_amount < 0.0 {
                                        log_ui.is_user_scrolled = true;
                                    }
                                    // Reaching the bottom resumes auto-scroll.
                                    if scroll_pos.0.y >= max_scroll - 1.0 {
                                        log_ui.is_user_scrolled = false;
                                    }
                                }
                            }
                        }
                    },
                )
                .with_children(|viewport| {
                    viewport.spawn((
                        TerminalLogWrapper,
                        Node {
                            width: Val::Percent(100.0),
                            flex_direction: FlexDirection::Column,
                            padding: UiRect::all(Val::Px(4.0)),
                            ..default()
                        },
                    ));
                });
        });
}

// ─── Interaction Systems ──────────────────────────────────────────────────────

/// Toggles the log viewport open/closed and updates the button label.
pub fn terminal_toggle_interaction(
    mut toggles: Query<
        (&Interaction, &mut BackgroundColor, &Children),
        (
            Changed<Interaction>,
            With<Button>,
            With<TerminalToggleButton>,
        ),
    >,
    mut viewports: Query<&mut Node, With<TerminalLogViewport>>,
    mut texts: Query<&mut Text>,
    mut log_ui: ResMut<CaptureLogUiState>,
    theme: Res<UiTheme>,
) {
    for (interaction, mut bg, children) in &mut toggles {
        match *interaction {
            Interaction::Pressed => {
                log_ui.is_open = !log_ui.is_open;
                let is_open = log_ui.is_open;
                for mut node in &mut viewports {
                    node.display = if is_open {
                        Display::Flex
                    } else {
                        Display::None
                    };
                }
                for child in children.iter() {
                    if let Ok(mut text) = texts.get_mut(child) {
                        text.0 = if is_open {
                            "v".to_string()
                        } else {
                            "^".to_string()
                        };
                    }
                }
                *bg = BackgroundColor(theme.button_background_hover);
            }
            Interaction::Hovered => *bg = BackgroundColor(theme.button_background),
            Interaction::None => *bg = BackgroundColor(theme.panel_background_soft),
        }
    }
}

/// Toggles individual log-level filters and forces a display rebuild.
pub fn log_level_filter_interaction(
    mut buttons: Query<
        (&Interaction, &LogLevelFilterButton, &mut BackgroundColor),
        (Changed<Interaction>, With<Button>),
    >,
    mut log_ui: ResMut<CaptureLogUiState>,
    theme: Res<UiTheme>,
) {
    for (interaction, filter_btn, mut bg) in &mut buttons {
        match *interaction {
            Interaction::Pressed => {
                if log_ui.active_filters.contains(&filter_btn.level) {
                    log_ui.active_filters.remove(&filter_btn.level);
                } else {
                    log_ui.active_filters.insert(filter_btn.level);
                }
                // The mismatch with last_rendered_filter will trigger a rebuild.
                *bg = BackgroundColor(if log_ui.active_filters.contains(&filter_btn.level) {
                    theme.button_background
                } else {
                    theme.panel_background_soft
                });
            }
            Interaction::Hovered => *bg = BackgroundColor(theme.button_background_hover),
            Interaction::None => {
                *bg = BackgroundColor(if log_ui.active_filters.contains(&filter_btn.level) {
                    theme.button_background
                } else {
                    theme.panel_background_soft
                });
            }
        }
    }
}

/// Clears all log entries and resets the display.
pub fn terminal_clear_interaction(
    mut buttons: Query<
        (&Interaction, &mut BackgroundColor),
        (
            Changed<Interaction>,
            With<Button>,
            With<TerminalClearButton>,
        ),
    >,
    mut capture_log: ResMut<CaptureLog>,
    mut log_ui: ResMut<CaptureLogUiState>,
    theme: Res<UiTheme>,
) {
    for (interaction, mut bg) in &mut buttons {
        match *interaction {
            Interaction::Pressed => {
                capture_log.clear();
                log_ui.last_rendered_count = 0;
                log_ui.selected_rows = None;
                log_ui.selection_anchor = None;
                *bg = BackgroundColor(theme.button_background_hover);
            }
            Interaction::Hovered => *bg = BackgroundColor(theme.button_background),
            Interaction::None => *bg = BackgroundColor(theme.panel_background_soft),
        }
    }
}

/// Handles click and shift-click to select log rows.
pub fn terminal_row_selection_interaction(
    mut rows: Query<
        (&Interaction, &TerminalLogRow, &mut BackgroundColor),
        (Changed<Interaction>, With<Button>),
    >,
    keys: Res<ButtonInput<KeyCode>>,
    mut log_ui: ResMut<CaptureLogUiState>,
) {
    for (interaction, row, mut bg) in &mut rows {
        let idx = row.entry_index;
        match *interaction {
            Interaction::Pressed => {
                let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
                if shift {
                    if let Some(anchor) = log_ui.selection_anchor {
                        let lo = anchor.min(idx);
                        let hi = anchor.max(idx);
                        log_ui.selected_rows = Some((lo, hi));
                    } else {
                        log_ui.selection_anchor = Some(idx);
                        log_ui.selected_rows = Some((idx, idx));
                    }
                } else {
                    log_ui.selection_anchor = Some(idx);
                    log_ui.selected_rows = Some((idx, idx));
                }
                *bg = BackgroundColor(Color::srgba(0.38, 0.66, 0.99, 0.35));
            }
            Interaction::Hovered => {
                let is_selected = log_ui
                    .selected_rows
                    .map(|(lo, hi)| idx >= lo && idx <= hi)
                    .unwrap_or(false);
                if !is_selected {
                    *bg = BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.06));
                }
            }
            Interaction::None => {
                let is_selected = log_ui
                    .selected_rows
                    .map(|(lo, hi)| idx >= lo && idx <= hi)
                    .unwrap_or(false);
                *bg = BackgroundColor(if is_selected {
                    Color::srgba(0.38, 0.66, 0.99, 0.20)
                } else {
                    Color::NONE
                });
            }
        }
    }
}

/// Handles Ctrl+A (select all) and Ctrl+C (copy selection to clipboard).
pub fn terminal_keyboard_input(
    mut log_ui: ResMut<CaptureLogUiState>,
    capture_log: Res<CaptureLog>,
    keys: Res<ButtonInput<KeyCode>>,
    mut clipboard: NonSendMut<ClipboardRes>,
) {
    if !log_ui.is_open {
        return;
    }

    let ctrl = keys.pressed(KeyCode::ControlLeft)
        || keys.pressed(KeyCode::ControlRight)
        || keys.pressed(KeyCode::SuperLeft)
        || keys.pressed(KeyCode::SuperRight);
    if !ctrl {
        return;
    }

    let filtered: Vec<&LogEntry> = capture_log
        .entries
        .iter()
        .filter(|e| log_ui.active_filters.contains(&e.level))
        .collect();

    if keys.just_pressed(KeyCode::KeyA) {
        if !filtered.is_empty() {
            log_ui.selection_anchor = Some(0);
            log_ui.selected_rows = Some((0, filtered.len() - 1));
        }
    }

    if keys.just_pressed(KeyCode::KeyC) {
        if let Some((sel_lo, sel_hi)) = log_ui.selected_rows {
            let lo = sel_lo.min(sel_hi);
            let hi = sel_lo.max(sel_hi);
            let mut text = String::new();
            for idx in lo..=hi {
                if let Some(entry) = filtered.get(idx) {
                    text.push_str(&format!(
                        "[{}] [{:>4}] {}: {}\n",
                        entry.timestamp,
                        entry.level.label(),
                        entry.source,
                        entry.message,
                    ));
                }
            }
            if !text.is_empty() {
                let _ = clipboard.0.set_text(text);
            }
        }
    }
}

// ─── Display Sync System ──────────────────────────────────────────────────────

/// Rebuilds the log row list whenever new entries arrive or filters change,
/// and auto-scrolls to the bottom unless the user has manually scrolled up.
pub fn sync_terminal_log_display(
    mut commands: Commands,
    capture_log: Res<CaptureLog>,
    mut log_ui: ResMut<CaptureLogUiState>,
    wrapper_query: Query<Entity, With<TerminalLogWrapper>>,
    row_entities: Query<Entity, With<TerminalLogRow>>,
    mut viewport_query: Query<&mut ScrollPosition, With<TerminalLogViewport>>,
    asset_server: Res<AssetServer>,
    theme: Res<UiTheme>,
) {
    let needs_rebuild = capture_log.entries.len() != log_ui.last_rendered_count
        || log_ui.active_filters != log_ui.last_rendered_filter;
    if !needs_rebuild {
        return;
    }

    let font: Handle<Font> = asset_server.load("fonts/FiraMono-Medium.ttf");

    let filtered: Vec<&LogEntry> = capture_log
        .entries
        .iter()
        .filter(|e| log_ui.active_filters.contains(&e.level))
        .collect();

    // Clamp selection to the new filtered list size.
    if let Some((lo, hi)) = log_ui.selected_rows {
        if lo >= filtered.len() {
            log_ui.selected_rows = None;
            log_ui.selection_anchor = None;
        } else if hi >= filtered.len() {
            log_ui.selected_rows = Some((lo, filtered.len().saturating_sub(1)));
        }
    }

    // Despawn all existing row entities before rebuilding.
    for entity in &row_entities {
        commands.entity(entity).despawn();
    }

    if let Ok(wrapper_entity) = wrapper_query.single() {
        commands.entity(wrapper_entity).with_children(|content| {
            for (filtered_idx, entry) in filtered.iter().enumerate() {
                let text_color = level_color(entry.level, &theme);

                let row_text = format!(
                    "[{}] [{:>4}] {}: {}",
                    entry.timestamp,
                    entry.level.label(),
                    entry.source,
                    entry.message,
                );

                let is_selected = log_ui
                    .selected_rows
                    .map(|(lo, hi)| filtered_idx >= lo && filtered_idx <= hi)
                    .unwrap_or(false);

                content
                    .spawn((
                        Button,
                        TerminalLogRow {
                            entry_index: filtered_idx,
                        },
                        Interaction::default(),
                        Node {
                            width: Val::Percent(100.0),
                            min_height: Val::Px(18.0),
                            padding: UiRect::axes(Val::Px(8.0), Val::Px(1.0)),
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        BackgroundColor(if is_selected {
                            Color::srgba(0.38, 0.66, 0.99, 0.20)
                        } else {
                            Color::NONE
                        }),
                    ))
                    .with_child((
                        Text::new(row_text),
                        TextFont {
                            font: font.clone(),
                            font_size: 11.0,
                            ..default()
                        },
                        TextColor(text_color),
                        Pickable::IGNORE,
                    ));
            }
        });
    }

    // Auto-scroll to newest entry unless the user has scrolled up.
    if !log_ui.is_user_scrolled {
        for mut scroll_pos in &mut viewport_query {
            scroll_pos.0.y = f32::MAX;
        }
    }

    log_ui.last_rendered_count = capture_log.entries.len();
    log_ui.last_rendered_filter = log_ui.active_filters.clone();
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn level_color(level: LogLevel, theme: &UiTheme) -> Color {
    match level {
        LogLevel::Error => Color::srgb(1.0, 0.35, 0.35),
        LogLevel::Warn => Color::srgb(1.0, 0.85, 0.0),
        LogLevel::Info => theme.text_primary,
        LogLevel::Debug => theme.text_muted,
    }
}
