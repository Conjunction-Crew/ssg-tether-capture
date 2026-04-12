use bevy::prelude::*;

pub mod input_field;
pub use input_field::{
    ClipboardRes, InputField, InputFieldText, input_field_display, input_field_interaction,
    input_field_keyboard,
};

pub mod terminal_log;
pub use terminal_log::{
    LogLevelFilterButton, TerminalClearButton, TerminalLogRow, TerminalLogViewport,
    TerminalLogWrapper, TerminalPanel, TerminalSaveButton, TerminalToggleButton,
    log_level_filter_interaction, spawn_terminal_panel, sync_terminal_log_display,
    terminal_clear_interaction, terminal_keyboard_input, terminal_row_selection_interaction,
    terminal_save_interaction, terminal_toggle_interaction,
};

#[derive(Component)]
pub struct ScreenRoot;
