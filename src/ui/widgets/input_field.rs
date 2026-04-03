use bevy::ecs::prelude::MessageReader;
use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::prelude::*;

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
}

/// System: focus the pressed InputField, blur all others.
pub fn input_field_interaction(
    mut field_query: Query<(Entity, &Interaction, &mut InputField), With<Button>>,
) {
    let pressed_entity = field_query
        .iter()
        .find(|(_, i, _)| **i == Interaction::Pressed)
        .map(|(e, _, _)| e);

    if let Some(pressed) = pressed_entity {
        for (entity, _, mut field) in &mut field_query {
            if entity == pressed {
                field.focused = true;
                field.error = false;
            } else {
                field.focused = false;
            }
        }
    }
}

/// System: route keyboard input to the currently focused InputField.
pub fn input_field_keyboard(
    mut keyboard: MessageReader<KeyboardInput>,
    mut field_query: Query<&mut InputField>,
) {
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
                Key::Backspace => {
                    field.value.pop();
                }
                Key::Space => {
                    if !field.is_numeric {
                        field.value.push(' ');
                    }
                }
                Key::Character(ch) => {
                    for c in ch.chars() {
                        if c.is_control() {
                            continue;
                        }
                        if field.is_numeric {
                            if c.is_ascii_digit() || c == '.' || c == '-' {
                                field.value.push(c);
                            }
                        } else {
                            field.value.push(c);
                        }
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
    mut text_query: Query<&mut Text, With<InputFieldText>>,
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
        // Update text to reflect value / placeholder / cursor
        if let Ok(children) = children_query.get(entity) {
            for child in children.iter() {
                if let Ok(mut text) = text_query.get_mut(child) {
                    let display = if field.value.is_empty() {
                        if field.focused {
                            "|".to_string()
                        } else {
                            field.placeholder.clone()
                        }
                    } else if field.focused {
                        format!("{}|", field.value)
                    } else {
                        field.value.clone()
                    };
                    text.0 = display;
                    break;
                }
            }
        }
    }
}
