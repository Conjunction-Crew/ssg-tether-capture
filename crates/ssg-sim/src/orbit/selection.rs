//! The target and chaser the user selects before a run.

use bevy::math::DVec3;
use bevy::prelude::*;
use brahe::coordinates::state_eci_to_koe;
use brahe::{AngleFormat, Epoch};
use nalgebra::Vector6;

use crate::orbit::catalog::GpElements;
use crate::orbit::lvlh::{LvlhFrame, relative_offset_lvlh, relative_start};
use crate::orbit::{Orbit, propagated_start_state, use_zero_eop};
use crate::project::manifest::{RelativePreset, RelativeToTarget};

#[derive(Debug, Clone)]
pub enum OrbitalSelectionSource {
    Catalog {
        // unused today; the P3 component/resource audit decides
        #[allow(dead_code)]
        catalog_index: usize,
        gpu_index: usize,
        label: String,
    },
    Custom {
        label: String,
    },
}

#[derive(Debug, Clone, Copy)]
pub struct EditableOrbitalElements {
    pub semi_major_axis_m: f64,
    pub eccentricity: f64,
    pub inclination_rad: f64,
    pub raan_rad: f64,
    pub arg_perigee_rad: f64,
    pub mean_anomaly_rad: f64,
    /// The epoch the six elements are valid at (F-27).
    pub epoch: Epoch,
}

impl EditableOrbitalElements {
    /// The elements of a catalog record, f64 throughout, at the record's epoch.
    // F-27: fixed in P4 (was `from_gpu`, through the f32 catalog buffer)
    pub fn from_gp(gp: &GpElements) -> Self {
        let elements = gp.kepler_elements_rad();
        Self {
            semi_major_axis_m: elements[0],
            eccentricity: elements[1],
            inclination_rad: elements[2],
            raan_rad: elements[3],
            arg_perigee_rad: elements[4],
            mean_anomaly_rad: elements[5],
            epoch: gp.epoch,
        }
    }

    pub fn to_vec6(self) -> Vector6<f64> {
        Vector6::new(
            self.semi_major_axis_m,
            self.eccentricity,
            self.inclination_rad,
            self.raan_rad,
            self.arg_perigee_rad,
            self.mean_anomaly_rad,
        )
    }
}

#[derive(Debug, Clone)]
pub struct SelectedOrbitalObject {
    pub source: OrbitalSelectionSource,
    pub elements: EditableOrbitalElements,
    /// The catalog record a pick came from; `None` for a custom orbit or once
    /// the elements are edited. `Some` starts the body from SGP4 (F-10).
    pub gp: Option<GpElements>,
    /// Mass in kg from `project.json`; `None` takes the body's default (the
    /// target 2,500 kg; the chaser, which is the tether root, the plan's
    /// `end_mass_kg`).
    pub mass_kg: Option<f64>,
    /// Inertia tensor in kg m² from `project.json`, in the sim's ECI-aligned axes
    /// at spawn; `None` (always from the catalog) takes the collider's.
    pub inertia_kg_m2: Option<[[f64; 3]; 3]>,
}

impl SelectedOrbitalObject {
    /// How the body starts, as the spawner's `initial_orbit` decides it: SGP4 for an
    /// unedited catalog pick, otherwise its elements at their epoch (F-10, F-27).
    pub(crate) fn start_orbit(&self) -> Orbit {
        match &self.gp {
            Some(gp) => Orbit::FromGp(gp.clone()),
            None => Orbit::FromElementsAt {
                elements: self.elements.to_vec6(),
                epoch: self.elements.epoch,
            },
        }
    }
}

#[derive(Resource, Debug, Clone, Default)]
pub struct OrbitalSelectionState {
    pub target: Option<SelectedOrbitalObject>,
    pub chaser: Option<SelectedOrbitalObject>,
    /// `Some` when the chaser is placed relative to the target (P9 S5): `chaser`
    /// holds the converted elements, and `project.json` gets `relative_to_target`.
    pub chaser_relative: Option<RelativeToTarget>,
}

/// The farthest the chaser's start may land from its V-bar/R-bar point, in metres
/// (P9 M1, operator decision M1-a). brahe 1.3.4 rounds an orbit with e ≲ 1e-4 to
/// e = 0, which on a near-circular target moves the start by up to `a·e`.
pub const RELATIVE_START_TOLERANCE_M: f64 = 1e-3;

/// The chaser's osculating elements at `start_epoch` for a start `offset_m` from
/// `target` on `preset`'s line, at rest in the target's LVLH frame (P9 S5, A-3):
/// [`relative_start`] from the target's start state as the sim propagates it, then
/// brahe's `state_eci_to_koe`. `Err` with the reason when the target has no start
/// state or LVLH frame, or when the elements, propagated as the sim does, start
/// farther than [`RELATIVE_START_TOLERANCE_M`] from that point.
pub fn relative_chaser_elements(
    target: &SelectedOrbitalObject,
    start_epoch: Epoch,
    preset: RelativePreset,
    offset_m: f64,
) -> Result<EditableOrbitalElements, String> {
    use_zero_eop();
    let target_state = propagated_start_state(&target.start_orbit(), start_epoch)
        .ok_or("the target has no start state (SGP4 failed)")?;
    let chaser = relative_start(&target_state, preset, offset_m)
        .ok_or("the target's orbit has no LVLH frame")?;
    let e = state_eci_to_koe(chaser, AngleFormat::Radians);
    let started = propagated_start_state(
        &Orbit::FromElementsAt {
            elements: e,
            epoch: start_epoch,
        },
        start_epoch,
    )
    .ok_or("the chaser's elements do not propagate")?;
    let frame = LvlhFrame::from_eci(&target_state).ok_or("the target's orbit has no LVLH frame")?;
    let rel = DVec3::new(
        started[0] - target_state[0],
        started[1] - target_state[1],
        started[2] - target_state[2],
    );
    let miss = (frame.to_lvlh(rel) - relative_offset_lvlh(preset, offset_m)).length();
    if miss.is_nan() || miss > RELATIVE_START_TOLERANCE_M {
        return Err(format!(
            "the chaser would start {miss:.3} m from its point: the propagator treats an orbit \
             with eccentricity below about 1e-4 as circular (brahe 1.3.4); use V-bar, a smaller \
             offset, or a target whose orbit is less circular"
        ));
    }
    Ok(EditableOrbitalElements {
        semi_major_axis_m: e[0],
        eccentricity: e[1],
        inclination_rad: e[2],
        raan_rad: e[3],
        arg_perigee_rad: e[4],
        mean_anomaly_rad: e[5],
        epoch: start_epoch,
    })
}
