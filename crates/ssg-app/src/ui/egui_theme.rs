//! egui `Visuals` derived from [`UiTheme`] (P8 S1): one colour source for the
//! bevy_ui overlays and every egui panel and window.

use bevy::prelude::*;
use bevy_egui::egui;
use bevy_egui::{EguiContext, PrimaryEguiContext};

use crate::ui::theme::UiTheme;

/// A Bevy colour as an egui sRGB colour (unmultiplied alpha).
pub fn color32_from(color: Color) -> egui::Color32 {
    let [r, g, b, a] = color.to_srgba().to_u8_array();
    egui::Color32::from_rgba_unmultiplied(r, g, b, a)
}

/// Dark egui visuals with the theme's fills and text colours.
pub fn visuals_from_theme(theme: &UiTheme) -> egui::Visuals {
    let mut visuals = egui::Visuals::dark();

    visuals.extreme_bg_color = color32_from(theme.background);
    visuals.panel_fill = color32_from(theme.panel_background);
    visuals.window_fill = color32_from(theme.panel_background);
    visuals.faint_bg_color = color32_from(theme.panel_background_soft);

    visuals.override_text_color = Some(color32_from(theme.text_primary));
    visuals.weak_text_color = Some(color32_from(theme.text_muted));
    visuals.hyperlink_color = color32_from(theme.text_accent);
    visuals.selection.bg_fill = color32_from(theme.text_accent);

    let widgets = &mut visuals.widgets;
    widgets.noninteractive.fg_stroke.color = color32_from(theme.text_primary);
    // Open collapsing headers and combo boxes carry the header colour.
    widgets.open.bg_fill = color32_from(theme.header_background);
    widgets.open.weak_bg_fill = color32_from(theme.header_background);
    for (state, fill) in [
        (&mut widgets.inactive, theme.button_background),
        (&mut widgets.hovered, theme.button_background_hover),
        (&mut widgets.active, theme.button_background_hover),
    ] {
        state.bg_fill = color32_from(fill);
        state.weak_bg_fill = color32_from(fill);
        state.fg_stroke.color = color32_from(theme.button_text);
    }

    visuals
}

/// Applies the theme's visuals once to each new primary egui context.
pub fn apply_egui_theme(
    theme: Res<UiTheme>,
    mut contexts: Query<&mut EguiContext, Added<PrimaryEguiContext>>,
) {
    for mut context in &mut contexts {
        let ctx = context.get_mut();
        ctx.set_theme(egui::Theme::Dark);
        ctx.set_visuals_of(egui::Theme::Dark, visuals_from_theme(&theme));
    }
}
