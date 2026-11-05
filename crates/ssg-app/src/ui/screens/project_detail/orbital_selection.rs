//! Orbital Selection on egui (P8 S6): the Target and Chaser cards with their six
//! element sliders. The slider specs and the conversions are rc.3's.

use bevy::prelude::*;
use bevy_egui::egui;

use crate::ui::catalog_state::{OrbitalElementField, OrbitalSelectionRole};
use crate::ui::events::UiEvent;
use crate::ui::form::{UnitSystem, show};
use brahe::Epoch;
use ssg_sim::orbit::ISS_ORBIT;
use ssg_sim::orbit::catalog::{SpaceCatalogEntry, SpaceObjectCatalog};
use ssg_sim::orbit::selection::{
    EditableOrbitalElements, OrbitalSelectionSource, OrbitalSelectionState, SelectedOrbitalObject,
};

const ORBITAL_ELEMENT_FIELDS: [OrbitalElementField; 6] = [
    OrbitalElementField::SemiMajorAxis,
    OrbitalElementField::Eccentricity,
    OrbitalElementField::Inclination,
    OrbitalElementField::Raan,
    OrbitalElementField::ArgPerigee,
    OrbitalElementField::MeanAnomaly,
];

/// What the Orbital Selection section shows.
#[derive(Debug, Clone, Default)]
pub struct OrbitalView {
    /// The selected catalog object's label.
    pub catalog_selection: Option<String>,
    pub target: Option<SelectedOrbitalObject>,
    pub chaser: Option<SelectedOrbitalObject>,
    pub unit_system: UnitSystem,
}

struct OrbitalElementSliderSpec {
    label: &'static str,
    min: f32,
    max: f32,
    precision: usize,
    suffix: &'static str,
}

fn orbital_element_slider_spec(field: OrbitalElementField) -> OrbitalElementSliderSpec {
    let (label, min, max, precision, suffix) = match field {
        OrbitalElementField::SemiMajorAxis => ("Semi-major axis", 6_400.0, 80_000.0, 0, "km"),
        OrbitalElementField::Eccentricity => ("Eccentricity", 0.0, 0.95, 4, ""),
        OrbitalElementField::Inclination => ("Inclination", 0.0, 180.0, 2, "deg"),
        OrbitalElementField::Raan => ("RAAN", 0.0, 360.0, 2, "deg"),
        OrbitalElementField::ArgPerigee => ("Argument of perigee", 0.0, 360.0, 2, "deg"),
        OrbitalElementField::MeanAnomaly => ("Mean anomaly", 0.0, 360.0, 2, "deg"),
    };
    OrbitalElementSliderSpec {
        label,
        min,
        max,
        precision,
        suffix,
    }
}

fn normalized_degrees(radians: f64) -> f32 {
    let mut degrees = radians.to_degrees() % 360.0;
    if degrees < 0.0 {
        degrees += 360.0;
    }
    degrees as f32
}

pub(crate) fn slider_value_from_elements(
    elements: &EditableOrbitalElements,
    field: OrbitalElementField,
) -> f32 {
    match field {
        OrbitalElementField::SemiMajorAxis => (elements.semi_major_axis_m / 1_000.0) as f32,
        OrbitalElementField::Eccentricity => elements.eccentricity as f32,
        OrbitalElementField::Inclination => elements.inclination_rad.to_degrees() as f32,
        OrbitalElementField::Raan => normalized_degrees(elements.raan_rad),
        OrbitalElementField::ArgPerigee => normalized_degrees(elements.arg_perigee_rad),
        OrbitalElementField::MeanAnomaly => normalized_degrees(elements.mean_anomaly_rad),
    }
}

pub(crate) fn write_slider_value_to_elements(
    elements: &mut EditableOrbitalElements,
    field: OrbitalElementField,
    value: f32,
) {
    let value = value as f64;
    match field {
        OrbitalElementField::SemiMajorAxis => elements.semi_major_axis_m = value * 1_000.0,
        OrbitalElementField::Eccentricity => elements.eccentricity = value,
        OrbitalElementField::Inclination => elements.inclination_rad = value.to_radians(),
        OrbitalElementField::Raan => elements.raan_rad = value.to_radians(),
        OrbitalElementField::ArgPerigee => elements.arg_perigee_rad = value.to_radians(),
        OrbitalElementField::MeanAnomaly => elements.mean_anomaly_rad = value.to_radians(),
    }
}

/// The value beside a slider, in `units` (the slider itself stays in km).
fn format_orbital_element_value(
    elements: &EditableOrbitalElements,
    field: OrbitalElementField,
    units: UnitSystem,
) -> String {
    let spec = orbital_element_slider_spec(field);
    let value = slider_value_from_elements(elements, field);
    let precision = spec.precision;
    if matches!(field, OrbitalElementField::SemiMajorAxis) {
        show(units.long_length(value as f64), precision)
    } else if spec.suffix.is_empty() {
        format!("{value:.precision$}")
    } else {
        format!("{value:.precision$} {}", spec.suffix)
    }
}

fn role_label(role: OrbitalSelectionRole) -> &'static str {
    match role {
        OrbitalSelectionRole::Target => "Target",
        OrbitalSelectionRole::Chaser => "Chaser",
    }
}

fn source_label(selected: &SelectedOrbitalObject) -> String {
    match &selected.source {
        OrbitalSelectionSource::Catalog { label, .. } => format!("Catalog: {label}"),
        OrbitalSelectionSource::Custom { label } => label.clone(),
    }
}

/// `Use Selected`: the orbit a catalog entry gives.
pub(crate) fn catalog_pick(
    catalog_index: usize,
    entry: &SpaceCatalogEntry,
) -> SelectedOrbitalObject {
    SelectedOrbitalObject {
        source: OrbitalSelectionSource::Catalog {
            catalog_index,
            gpu_index: entry.gpu_index,
            label: entry.display_label(),
        },
        elements: EditableOrbitalElements::from_gp(&entry.gp),
        gp: Some(entry.gp.clone()),
        mass_kg: None,
        inertia_kg_m2: None,
    }
}

/// `Create Custom`: the ISS defaults at the sim's start epoch.
pub(crate) fn custom_orbit(role: OrbitalSelectionRole, epoch: Epoch) -> SelectedOrbitalObject {
    SelectedOrbitalObject {
        source: OrbitalSelectionSource::Custom {
            label: format!("Custom {}", role_label(role)),
        },
        elements: EditableOrbitalElements {
            semi_major_axis_m: ISS_ORBIT[0],
            eccentricity: ISS_ORBIT[1],
            inclination_rad: ISS_ORBIT[2],
            raan_rad: ISS_ORBIT[3],
            arg_perigee_rad: ISS_ORBIT[4],
            mean_anomaly_rad: ISS_ORBIT[5],
            epoch,
        },
        gp: None,
        mass_kg: None,
        inertia_kg_m2: None,
    }
}

fn slot(
    selection: &mut OrbitalSelectionState,
    role: OrbitalSelectionRole,
) -> &mut Option<SelectedOrbitalObject> {
    match role {
        OrbitalSelectionRole::Target => &mut selection.target,
        OrbitalSelectionRole::Chaser => &mut selection.chaser,
    }
}

pub(crate) fn set_orbit(
    selection: &mut OrbitalSelectionState,
    role: OrbitalSelectionRole,
    selected: SelectedOrbitalObject,
) {
    *slot(selection, role) = Some(selected);
}

pub(crate) fn set_orbital_element(
    selection: &mut OrbitalSelectionState,
    role: OrbitalSelectionRole,
    field: OrbitalElementField,
    value: f32,
) {
    if let Some(selected) = slot(selection, role) {
        write_slider_value_to_elements(&mut selected.elements, field, value);
        // Edited elements are no longer the catalog record (F-27).
        selected.gp = None;
    }
}

/// A restored body that still carries its catalog record becomes a catalog
/// selection again, so the map highlights it (F-59). The entry is matched by
/// NORAD id; the label stays the manifest's, so `project.json` does not change.
pub(crate) fn restore_catalog_source(
    selected: &mut SelectedOrbitalObject,
    entries: &[SpaceCatalogEntry],
) {
    let OrbitalSelectionSource::Custom { label } = &selected.source else {
        return;
    };
    let Some(gp) = &selected.gp else {
        return;
    };
    if let Some((catalog_index, entry)) = entries
        .iter()
        .enumerate()
        .find(|(_, entry)| entry.norad_id == gp.norad_id)
    {
        selected.source = OrbitalSelectionSource::Catalog {
            catalog_index,
            gpu_index: entry.gpu_index,
            label: label.clone(),
        };
    }
}

/// On entering the sim, once the session has loaded the catalog (F-59).
pub fn restore_catalog_sources(
    mut selection: ResMut<OrbitalSelectionState>,
    catalog: Res<SpaceObjectCatalog>,
) {
    let selection = &mut *selection;
    for selected in [&mut selection.target, &mut selection.chaser]
        .into_iter()
        .flatten()
    {
        restore_catalog_source(selected, &catalog.entries);
    }
}

fn element_slider(
    ui: &mut egui::Ui,
    role: OrbitalSelectionRole,
    field: OrbitalElementField,
    elements: &EditableOrbitalElements,
    units: UnitSystem,
    out: &mut Vec<UiEvent>,
) {
    let spec = orbital_element_slider_spec(field);
    let label = ui
        .horizontal(|ui| {
            let label = ui.weak(egui::RichText::new(spec.label).size(10.0));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(format_orbital_element_value(elements, field, units))
                        .size(10.0),
                );
            });
            label
        })
        .inner;
    let mut value = slider_value_from_elements(elements, field).clamp(spec.min, spec.max);
    ui.spacing_mut().slider_width = ui.available_width();
    // No `max_decimals` on the slider: it rounds on every frame and reports a
    // change, which would drop a catalog record with no input (F-27).
    let slider = egui::Slider::new(&mut value, spec.min..=spec.max).show_value(false);
    if ui.add(slider).labelled_by(label.id).changed() {
        let value = egui::emath::round_to_decimals(value as f64, spec.precision) as f32;
        out.push(UiEvent::SetOrbitalElement { role, field, value });
    }
}

fn role_card(
    ui: &mut egui::Ui,
    role: OrbitalSelectionRole,
    selected: Option<&SelectedOrbitalObject>,
    units: UnitSystem,
    out: &mut Vec<UiEvent>,
) {
    egui::Frame::new()
        .fill(ui.visuals().faint_bg_color)
        .inner_margin(10.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let name = role_label(role);
            let accent = ui.visuals().hyperlink_color;
            ui.label(
                egui::RichText::new(format!("{name} Orbit"))
                    .size(14.0)
                    .color(accent),
            );
            let summary = selected.map_or("No orbit selected".to_string(), source_label);
            ui.label(egui::RichText::new(format!("{name}: {summary}")).size(11.0));
            ui.horizontal(|ui| {
                let soft = ui.visuals().panel_fill;
                let text =
                    |text| egui::Button::new(egui::RichText::new(text).size(11.0)).fill(soft);
                if ui.add(text("Use Selected")).clicked() {
                    out.push(UiEvent::UseCatalogSelection(role));
                }
                if ui.add(text("Create Custom")).clicked() {
                    out.push(UiEvent::CreateCustomOrbit(role));
                }
            });
            if let Some(selected) = selected {
                for field in ORBITAL_ELEMENT_FIELDS {
                    element_slider(ui, role, field, &selected.elements, units, out);
                }
            }
        });
}

pub(super) fn orbital_selection_section(
    ui: &mut egui::Ui,
    view: &OrbitalView,
    out: &mut Vec<UiEvent>,
) {
    ui.weak(
        egui::RichText::new(
            "Pick a catalog object on the map, assign it to target or chaser, or create a custom orbit.",
        )
        .size(12.0),
    );
    let selected = view.catalog_selection.as_deref().unwrap_or("none");
    ui.label(egui::RichText::new(format!("Selected catalog object: {selected}")).size(11.0));
    for (role, selected) in [
        (OrbitalSelectionRole::Target, view.target.as_ref()),
        (OrbitalSelectionRole::Chaser, view.chaser.as_ref()),
    ] {
        role_card(ui, role, selected, view.unit_system, out);
    }
}
