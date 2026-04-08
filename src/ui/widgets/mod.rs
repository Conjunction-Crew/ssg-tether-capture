use bevy::prelude::*;

pub mod input_field;
pub use input_field::{
    input_field_display, input_field_interaction, input_field_keyboard, ClipboardRes, InputField,
    InputFieldText,
};

#[derive(Component)]
pub struct ScreenRoot;
