//! The target and chaser the user selects before a run.

use bevy::prelude::*;
use nalgebra::Vector6;

use crate::orbit::catalog::GpuOrbitalElements;

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
    pub epoch_offset_seconds: f64,
}

impl EditableOrbitalElements {
    /// The f64 elements of a catalog buffer entry, as the app selects them.
    // F-27: current behaviour, expected to change in P4 (the values went through f32)
    pub fn from_gpu(elements: &GpuOrbitalElements) -> Self {
        Self {
            semi_major_axis_m: elements.a as f64,
            eccentricity: elements.e as f64,
            inclination_rad: elements.i as f64,
            raan_rad: elements.raan as f64,
            arg_perigee_rad: elements.argp as f64,
            mean_anomaly_rad: elements.mean_anomaly as f64,
            epoch_offset_seconds: elements.epoch_offset_seconds as f64,
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
}

#[derive(Resource, Debug, Clone, Default)]
pub struct OrbitalSelectionState {
    pub target: Option<SelectedOrbitalObject>,
    pub chaser: Option<SelectedOrbitalObject>,
}
