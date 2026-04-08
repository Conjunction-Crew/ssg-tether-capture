use bevy::ecs::prelude::MessageReader;
use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::prelude::*;
use bevy::ui::UiGlobalTransform;

/// Non-send resource wrapping the system clipboard.
pub struct ClipboardRes(pub arboard::Clipboard);

impl Default for ClipboardRes {
    fn default() -> Self {
        Self(arboard::Clipboard::new().expect("Failed to open system clipboard"))
    }
}

/// Marker for the text display child inside an InputField node.
#[derive(Component)]
pub struct InputFieldText;

/// A clickable focusable form field backed by keyboard input.
#[derive(Component, Debug, Default, Clone)]
pub struct InputField {
    pub value: String,
    pub focused: bool,
    pub placeholder: String,
    /// When true only digits, `.`, and `-` are accepted.
    pub is_numeric: bool,
    /// Set by validation; cleared when the user starts typing.
    pub error: bool,
    /// Byte offset of the cursor inside `value`. Always on a char boundary.
    pub cursor_pos: usize,
    /// Byte offset of the fixed (anchor) end of a selection.
    /// `None` means no active selection.
    pub selection_anchor: Option<usize>,
}

impl InputField {
    /// Return the selected byte range `(start, end)`, or `None` if no selection.
    pub fn selection_range(&self) -> Option<(usize, usize)> {
        self.selection_anchor.map(|anchor| {
            let lo = self.cursor_pos.min(anchor);
            let hi = self.cursor_pos.max(anchor);
            (lo, hi)
        })
    }

    /// Delete any active selection. Returns `true` if text was deleted.
    fn delete_selection(&mut self) -> bool {
        if let Some((lo, hi)) = self.selection_range() {
            if lo < hi {
                self.value.drain(lo..hi);
                self.cursor_pos = lo;
                self.selection_anchor = None;
                return true;
            }
        }
        self.selection_anchor = None;
        false
    }

    /// Insert a string at the cursor, replacing any active selection first.
    /// Characters are filtered through `is_numeric` if set.
    fn insert_str(&mut self, s: &str) {
        self.delete_selection();
        for c in s.chars() {
            if c.is_control() {
                continue;
            }
            if self.is_numeric && !(c.is_ascii_digit() || c == '.' || c == '-') {
                continue;
            }
            self.value.insert(self.cursor_pos, c);
            self.cursor_pos += c.len_utf8();
        }
    }

    /// Move the cursor left by one char. Returns the new position.
    fn prev_char_boundary(&self) -> usize {
        if self.cursor_pos == 0 {
            return 0;
        }
        let mut pos = self.cursor_pos - 1;
        while !self.value.is_char_boundary(pos) {
            pos -= 1;
        }
        pos
    }

    /// Move the cursor right by one char. Returns the new position.
    fn next_char_boundary(&self) -> usize {
        if self.cursor_pos >= self.value.len() {
            return self.value.len();
        }
        let mut pos = self.cursor_pos + 1;
        while !self.value.is_char_boundary(pos) {
            pos += 1;
        }
        pos
    }
}

/// System: focus the pressed InputField, blur all others.
pub fn input_field_interaction(
    mut field_query: Query<
        (Entity, &Interaction, &mut InputField, &UiGlobalTransform, &ComputedNode),
        With<Button>,
    >,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    form: Res<crate::resources::capture_plan_form::NewCapturePlanForm>,
) {
    if form.read_only {
        return;
    }
    let pressed_entity = field_query
        .iter()
        .find(|(_, i, _, _, _)| **i == Interaction::Pressed)
        .map(|(e, _, _, _, _)| e);

    // Get physical cursor position for character placement estimation.
    // UiGlobalTransform and ComputedNode.size() both use physical pixels.
    let maybe_cursor = windows.single().ok().and_then(|w| {
        w.physical_cursor_position()
    });

    if let Some(pressed) = pressed_entity {
        for (entity, _, mut field, transform, computed) in &mut field_query {
            if entity == pressed {
                // Estimate click position within the text.
                // UiGlobalTransform.affine().translation is the node center in physical pixels.
                if let Some(cursor) = maybe_cursor {
                    let node_center_x = transform.affine().translation.x;
                    let node_half_w = computed.size().x / 2.0;
                    let node_left = node_center_x - node_half_w;
                    let x_in_node = cursor.x - node_left;
                    // Subtract left padding (min_inset.x); FiraMono 13px ≈ 7.8 logical px/char
                    let x_in_text = (x_in_node - computed.padding.min_inset.x).max(0.0);
                    let char_width_physical = 7.8 / computed.inverse_scale_factor;
                    let char_idx = (x_in_text / char_width_physical) as usize;
                    field.cursor_pos = field
                        .value
                        .char_indices()
                        .nth(char_idx)
                        .map(|(b, _)| b)
                        .unwrap_or(field.value.len());
                } else if !field.focused {
                    // Fallback: place cursor at end when we have no cursor position
                    field.cursor_pos = field.value.len();
                }
                field.selection_anchor = None;
                field.focused = true;
                field.error = false;
            } else {
                field.focused = false;
                field.selection_anchor = None;
            }
        }
    }
}

/// System: route keyboard input to the currently focused InputField.
pub fn input_field_keyboard(
    mut keyboard: MessageReader<KeyboardInput>,
    mut field_query: Query<&mut InputField>,
    keys: Res<ButtonInput<KeyCode>>,
    mut clipboard: NonSendMut<ClipboardRes>,
    form: Res<crate::resources::capture_plan_form::NewCapturePlanForm>,
) {
    if form.read_only {
        // Drain events so they don't accumulate
        for _ in keyboard.read() {}
        return;
    }
    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    let act = keys.pressed(KeyCode::ControlLeft)
        || keys.pressed(KeyCode::ControlRight)
        || keys.pressed(KeyCode::SuperLeft)
        || keys.pressed(KeyCode::SuperRight);

    let events: Vec<KeyboardInput> = keyboard.read().cloned().collect();
    for event in &events {
        if event.state != ButtonState::Pressed {
            continue;
        }
        for mut field in &mut field_query {
            if !field.focused {
                continue;
            }

            match &event.logical_key {
                // ── Deletion ──────────────────────────────────────────
                Key::Backspace => {
                    if !field.delete_selection() {
                        let prev = field.prev_char_boundary();
                        let cursor = field.cursor_pos;
                        if prev < cursor {
                            field.value.drain(prev..cursor);
                            field.cursor_pos = prev;
                        }
                    }
                    field.error = false;
                }
                Key::Delete => {
                    if !field.delete_selection() {
                        let next = field.next_char_boundary();
                        let cursor = field.cursor_pos;
                        if next > cursor {
                            field.value.drain(cursor..next);
                        }
                    }
                    field.error = false;
                }

                // ── Navigation ────────────────────────────────────────
                Key::ArrowLeft => {
                    if shift {
                        if field.selection_anchor.is_none() {
                            field.selection_anchor = Some(field.cursor_pos);
                        }
                        field.cursor_pos = field.prev_char_boundary();
                    } else {
                        if let Some((lo, _)) = field.selection_range() {
                            field.cursor_pos = lo;
                        } else {
                            field.cursor_pos = field.prev_char_boundary();
                        }
                        field.selection_anchor = None;
                    }
                }
                Key::ArrowRight => {
                    if shift {
                        if field.selection_anchor.is_none() {
                            field.selection_anchor = Some(field.cursor_pos);
                        }
                        field.cursor_pos = field.next_char_boundary();
                    } else {
                        if let Some((_, hi)) = field.selection_range() {
                            field.cursor_pos = hi;
                        } else {
                            field.cursor_pos = field.next_char_boundary();
                        }
                        field.selection_anchor = None;
                    }
                }
                Key::Home => {
                    if shift {
                        if field.selection_anchor.is_none() {
                            field.selection_anchor = Some(field.cursor_pos);
                        }
                    } else {
                        field.selection_anchor = None;
                    }
                    field.cursor_pos = 0;
                }
                Key::End => {
                    if shift {
                        if field.selection_anchor.is_none() {
                            field.selection_anchor = Some(field.cursor_pos);
                        }
                    } else {
                        field.selection_anchor = None;
                    }
                    field.cursor_pos = field.value.len();
                }

                // ── Space ─────────────────────────────────────────────
                Key::Space => {
                    if !field.is_numeric {
                        field.insert_str(" ");
                        field.error = false;
                    }
                }

                // ── Characters (includes Ctrl/Cmd shortcuts) ──────────
                Key::Character(ch) => {
                    // Single-char shortcuts when action modifier is held
                    if act && ch.len() == 1 {
                        match ch.chars().next().unwrap().to_ascii_lowercase() {
                            'a' => {
                                // Select all
                                field.selection_anchor = Some(0);
                                field.cursor_pos = field.value.len();
                            }
                            'c' => {
                                // Copy selection
                                if let Some((lo, hi)) = field.selection_range() {
                                    let selected = field.value[lo..hi].to_string();
                                    let _ = clipboard.0.set_text(selected);
                                }
                            }
                            'x' => {
                                // Cut selection
                                if let Some((lo, hi)) = field.selection_range() {
                                    let selected = field.value[lo..hi].to_string();
                                    let _ = clipboard.0.set_text(selected);
                                    field.delete_selection();
                                    field.error = false;
                                }
                            }
                            'v' => {
                                // Paste
                                if let Ok(text) = clipboard.0.get_text() {
                                    field.insert_str(&text);
                                    field.error = false;
                                }
                            }
                            _ => {}
                        }
                    } else if !act {
                        field.insert_str(ch);
                        field.error = false;
                    }
                }

                _ => {}
            }
            break; // only update one focused field
        }
    }
}

/// System: update the InputFieldText child and border to reflect the current value/focus.
pub fn input_field_display(
    mut field_query: Query<(Entity, &InputField, &mut BorderColor), Changed<InputField>>,
    children_query: Query<&Children>,
    mut text_query: Query<(&mut Text, &mut TextColor), With<InputFieldText>>,
) {
    for (entity, field, mut border) in &mut field_query {
        // Update border to reflect focus / error state
        *border = if field.focused {
            BorderColor::all(Color::srgb(0.38, 0.66, 0.99))
        } else if field.error {
            BorderColor::all(Color::srgb(0.9, 0.3, 0.3))
        } else {
            BorderColor::all(Color::srgba(0.059, 0.078, 0.133, 0.88))
        };

        // Update text content and color to reflect value / placeholder / cursor / selection
        if let Ok(children) = children_query.get(entity) {
            for child in children.iter() {
                if let Ok((mut text, mut color)) = text_query.get_mut(child) {
                    let showing_placeholder = !field.focused && field.value.is_empty();
                    text.0 = build_display_text(field);
                    color.0 = if showing_placeholder {
                        // gray for placeholder
                        Color::srgb(0.60, 0.66, 0.78)
                    } else {
                        // white for user-entered text
                        Color::srgb(0.94, 0.95, 0.98)
                    };
                    break;
                }
            }
        }
    }
}

fn build_display_text(field: &InputField) -> String {
    if !field.focused {
        return if field.value.is_empty() {
            field.placeholder.clone()
        } else {
            field.value.clone()
        };
    }

    if field.value.is_empty() {
        return "|".to_string();
    }

    // Clamp positions defensively
    let cursor = field.cursor_pos.min(field.value.len());

    match field.selection_range() {
        None => {
            // No selection: insert cursor marker at cursor_pos
            let before = &field.value[..cursor];
            let after = &field.value[cursor..];
            format!("{before}|{after}")
        }
        Some((lo, hi)) => {
            // Show selected region with brackets; cursor marker at active end
            let before_sel = &field.value[..lo];
            let selected = &field.value[lo..hi];
            let after_sel = &field.value[hi..];
            if cursor == hi {
                // Cursor is at the end of selection
                format!("{before_sel}[{selected}]|{after_sel}")
            } else {
                // Cursor is at the start of selection
                format!("{before_sel}|[{selected}]{after_sel}")
            }
        }
    }
}
