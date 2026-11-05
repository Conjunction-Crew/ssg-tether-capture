use bevy_egui::egui;

/// Orbital Selection, shown before Start. S6 ports the cards and sliders; until
/// then Start stays disabled unless the project restored both orbits.
pub(super) fn orbital_selection_section(ui: &mut egui::Ui) {
    ui.weak(
        egui::RichText::new(
            "Pick a catalog object on the map, assign it to target or chaser, or create a custom orbit.",
        )
        .size(12.0),
    );
}
