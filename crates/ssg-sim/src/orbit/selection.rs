//! The target and chaser the user selects before a run.

use bevy::prelude::*;
use brahe::Epoch;
use nalgebra::Vector6;

use crate::orbit::catalog::GpElements;

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

#[derive(Resource, Debug, Clone, Default)]
pub struct OrbitalSelectionState {
    pub target: Option<SelectedOrbitalObject>,
    pub chaser: Option<SelectedOrbitalObject>,
}
