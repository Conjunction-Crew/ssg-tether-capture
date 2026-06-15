use bevy::prelude::*;

use crate::components::capture_components::PropagationNodeMode;

/// Runtime state for an active propagation sim. Populated by
/// `setup_orbital_selection` when the selected plan is a propagation plan, and
/// read by the propagation physics systems. Disabled (`enabled = false`) for
/// capture sims so those systems early-return.
#[derive(Resource, Debug, Clone)]
pub struct ActivePropagation {
    pub enabled: bool,
    /// Name/key of the tether in [`crate::resources::orbital_cache::OrbitalCache::tethers`].
    pub tether_name: String,
    pub node_mode: PropagationNodeMode,
    /// Reference orbit semi-major axis (m), used for the Hill mean motion.
    pub reference_a_m: f64,
    /// Max allowable joint tension (N). Log-only — exceeding it emits a warning
    /// but does not break or clamp the joint.
    pub max_tension_n: Option<f64>,
}

impl Default for ActivePropagation {
    fn default() -> Self {
        Self {
            enabled: false,
            tether_name: String::new(),
            node_mode: PropagationNodeMode::default(),
            reference_a_m: 0.0,
            max_tension_n: None,
        }
    }
}
