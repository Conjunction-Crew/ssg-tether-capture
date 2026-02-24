use bevy::prelude::*;

#[derive(Resource, Debug, Clone)]
pub struct UiTheme {
	pub background: Color,
	pub panel_background: Color,
	pub text_primary: Color,
	pub text_muted: Color,
	pub button_background: Color,
	pub button_text: Color,
	pub content_max_width: Val,
}

impl Default for UiTheme {
	fn default() -> Self {
		Self {
			background: Color::srgba(0.03, 0.04, 0.07, 0.96),
			panel_background: Color::srgba(0.08, 0.10, 0.15, 0.95),
			text_primary: Color::srgb(0.94, 0.95, 0.98),
			text_muted: Color::srgb(0.72, 0.75, 0.80),
			button_background: Color::srgb(0.15, 0.20, 0.33),
			button_text: Color::srgb(0.95, 0.96, 0.99),
			content_max_width: px(1080.0),
		}
	}
}
