use bevy::prelude::*;

pub mod input_field;
pub use input_field::{
    ClipboardRes, InputField, InputFieldText, input_field_display, input_field_interaction,
    input_field_keyboard,
};

#[derive(Component)]
pub struct ScreenRoot;
