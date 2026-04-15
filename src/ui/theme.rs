use bevy::prelude::*;

#[derive(Resource, Debug, Clone)]
pub struct UiTheme {
    pub background: Color,
    pub header_background: Color,
    pub panel_background: Color,
    pub panel_background_soft: Color,
    pub text_primary: Color,
    pub text_muted: Color,
    pub text_accent: Color,
    pub button_background: Color,
    pub button_background_hover: Color,
    pub button_text: Color,
    pub content_max_width: Val,
}

impl Default for UiTheme {
    fn default() -> Self {
        Self {
            background: Color::srgb(0.008, 0.024, 0.090),
            header_background: Color::srgba(0.020, 0.039, 0.102, 0.72),
            panel_background: Color::srgba(0.059, 0.078, 0.133, 0.88),
            panel_background_soft: Color::srgba(0.071, 0.102, 0.173, 0.64),
            text_primary: Color::srgb(0.94, 0.95, 0.98),
            text_muted: Color::srgb(0.60, 0.66, 0.78),
            text_accent: Color::srgb(0.38, 0.66, 0.99),
            button_background: Color::srgb(0.137, 0.286, 0.914),
            button_background_hover: Color::srgb(0.118, 0.251, 0.812),
            button_text: Color::srgb(0.95, 0.96, 0.99),
            content_max_width: px(1080.0),
        }
    }
}
