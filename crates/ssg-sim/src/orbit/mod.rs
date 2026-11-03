//! Orbits: the orbital state of sim bodies and its propagation.

pub mod cache;
pub mod catalog;
pub mod lvlh;
pub mod selection;

use std::sync::PoisonError;

use bevy::prelude::*;
use brahe::eop::{StaticEOPProvider, set_global_eop_provider};
use brahe::propagators::SGPPropagator;
use brahe::utils::DOrbitStateProvider;
use brahe::{Epoch, KeplerianPropagator};
use nalgebra::Vector6;

use crate::SimStep;
use crate::orbit::cache::OrbitalCache;
use crate::orbit::catalog::GpElements;
use crate::recording::inputs::SimInputLog;
use crate::session::UiScreen;
use crate::telemetry::{LogEvent, LogLevel};
use crate::time::{ManualPhysics, WorldTime};

// Orbital parameters and state for a body approaching another object.
#[derive(Component, Debug, Clone, Default)]
pub struct Orbital {
    // unused today; the P3 component/resource audit decides
    #[allow(dead_code)]
    pub object_id: String,
    // unused today; the P3 component/resource audit decides
    #[allow(dead_code)]
    pub parent_entity: Option<Entity>,
    pub propagator: Option<KeplerianPropagator>,
}

// Init methods for orbital objects
#[derive(Component, Debug, Clone)]
#[require(Orbital)]
pub enum Orbit {
    /// Osculating Keplerian elements at the start epoch.
    FromElements(Vector6<f64>),
    /// Osculating Keplerian elements at `epoch` (F-27).
    FromElementsAt {
        elements: Vector6<f64>,
        epoch: Epoch,
    },
    /// A catalog GP record: the start state comes from SGP4 (F-10).
    FromGp(GpElements),
}

/// The Earth's equatorial radius (WGS-84), for altitudes (F-28).
pub const EARTH_EQUATORIAL_RADIUS_M: f64 = brahe::WGS84_A;

// Other constants
pub const ISS_ORBIT: Vector6<f64> = Vector6::new(
    // Semi-major axis (meters)
    6_799_130.0,
    // Eccentricity (dimensionless)
    0.00112,
    // Inclination (radians)
    0.90114,
    // Right ascension of ascending node (radians)
    3.54993,
    // Argument of periapsis (radians)
    1.51296,
    // Mean anomaly (radians)
    4.77190,
);

/// Builds the propagators of newly spawned bodies. Every body starts at
/// `WorldTime.start_epoch`, not at the current epoch, which is already past it
/// when this runs (F-06). Catalog records start from their SGP4 state there (F-10);
/// elements are taken at their own epoch (F-27). From then on every body is
/// two-body (Kepler) propagated.
///
/// The first time it builds a propagator in a session it writes the
/// [`SimInputLog`]'s `SessionStart` entry: the fixed steps that ran before it,
/// the spawn-update step count k (P7, C1).
pub fn init_orbitals(
    mut commands: Commands,
    mut q: Query<(Entity, &Orbit, &mut Orbital), Added<Orbit>>,
    world_time: Res<WorldTime>,
    mut log_events: MessageWriter<LogEvent>,
    mut input_log: ResMut<SimInputLog>,
) {
    let start_epoch = world_time.start_epoch;
    if !q.is_empty() {
        let step = ((world_time.epoch - start_epoch) / crate::time::PHYS_DT).round() as u32;
        input_log.push_session_start(step);
    }
    for (entity, init, mut orbital) in &mut q {
        orbital.propagator = match init {
            Orbit::FromElements(elements) => kepler_at_start(*elements, start_epoch, start_epoch),
            Orbit::FromElementsAt { elements, epoch } => {
                kepler_at_start(*elements, *epoch, start_epoch)
            }
            Orbit::FromGp(gp) => match sgp4_state_eci(gp, start_epoch) {
                Ok(state) => Some(KeplerianPropagator::from_eci(start_epoch, state, 1.0)),
                Err(e) => {
                    log_events.write(LogEvent {
                        level: LogLevel::Error,
                        source: "propagation",
                        message: format!("SGP4 failed for NORAD {}: {e}", gp.norad_id),
                    });
                    None
                }
            },
        };

        commands.entity(entity).remove::<Orbit>();
    }
}

/// The ECI state `orbit` starts from at `start_epoch`, as [`init_orbitals`]
/// computes it; `None` when SGP4 fails. The tether spawner reads it before any
/// propagator exists (P7.5 design D9).
pub(crate) fn start_state_eci(orbit: &Orbit, start_epoch: Epoch) -> Option<Vector6<f64>> {
    let kepler = |elements: Vector6<f64>, epoch: Epoch| {
        kepler_at_start(elements, epoch, start_epoch)?
            .state_eci(start_epoch)
            .ok()
    };
    match orbit {
        Orbit::FromElements(elements) => kepler(*elements, start_epoch),
        Orbit::FromElementsAt { elements, epoch } => kepler(*elements, *epoch),
        Orbit::FromGp(gp) => sgp4_state_eci(gp, start_epoch).ok(),
    }
}

/// A two-body propagator at `start_epoch` from osculating `elements` at `epoch`.
fn kepler_at_start(
    elements: Vector6<f64>,
    epoch: Epoch,
    start_epoch: Epoch,
) -> Option<KeplerianPropagator> {
    let propagator =
        KeplerianPropagator::from_keplerian(epoch, elements, brahe::AngleFormat::Radians, 1.0);
    let eci = propagator.state_eci(start_epoch).ok()?;
    Some(KeplerianPropagator::from_eci(start_epoch, eci, 1.0))
}

/// The SGP4 state of `gp` at `epoch`, in brahe's ECI (GCRF) frame. The object
/// name and id are not passed: synthetic ids (e.g. "P0-CHASER-1") fail brahe's
/// international-designator check, and SGP4 does not use them.
fn sgp4_state_eci(gp: &GpElements, epoch: Epoch) -> Result<Vector6<f64>, String> {
    let propagator = SGPPropagator::from_omm_elements(
        &gp.epoch_str,
        gp.mean_motion_rev_per_day,
        gp.eccentricity,
        gp.inclination_deg,
        gp.raan_deg,
        gp.arg_perigee_deg,
        gp.mean_anomaly_deg,
        gp.norad_id as u64,
        1.0,
        None,
        None,
        gp.classification,
        gp.bstar,
        gp.mean_motion_dot,
        gp.mean_motion_ddot,
        gp.ephemeris_type,
        None,
        None,
    )
    .map_err(|e| e.to_string())?;
    propagator.state_eci(epoch).map_err(|e| e.to_string())
}

pub fn cache_eci_states(
    orbitals: Query<(Entity, &Orbital)>,
    mut orbital_cache: ResMut<OrbitalCache>,
    world_time: Res<WorldTime>,
) {
    let epoch = world_time.epoch;
    let states = std::sync::Mutex::new(bevy::platform::collections::HashMap::default());

    orbitals.par_iter().for_each(|(entity, orbital)| {
        if let Some(state) = orbital
            .propagator
            .as_ref()
            .and_then(|prop| prop.state_eci(epoch).ok())
        {
            states
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(entity, state);
        }
    });

    orbital_cache.eci_states = states.into_inner().unwrap_or_else(PoisonError::into_inner);
}

pub struct OrbitPlugin;

impl Plugin for OrbitPlugin {
    fn build(&self, app: &mut App) {
        // SGP4's TEME -> GCRF rotation reads brahe's global EOP provider. Zero EOP
        // is offline and deterministic, and both bodies of a pair get the same
        // rotation, so their relative state is unaffected (A-3). Never call
        // `brahe::initialize_eop()`: it reads files or the network.
        set_global_eop_provider(StaticEOPProvider::from_zero());
        app.add_systems(First, init_orbitals.run_if(in_state(UiScreen::Sim)))
            .add_systems(ManualPhysics, cache_eci_states.in_set(SimStep::CacheEci));
    }
}
