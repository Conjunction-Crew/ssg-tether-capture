//! Orbits: the orbital state of sim bodies and its propagation.

pub mod cache;
pub mod catalog;
pub mod selection;

use std::sync::PoisonError;

use bevy::prelude::*;
use brahe::utils::DOrbitStateProvider;
use brahe::{Epoch, KeplerianPropagator};
use nalgebra::Vector6;

use crate::SimStep;
use crate::orbit::cache::OrbitalCache;
use crate::session::UiScreen;
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
    // F-10: constructed when P4 implements SGP4
    #[allow(dead_code)]
    FromTle(String),
    FromElements(Vector6<f64>),
}

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

pub fn init_orbitals(
    mut commands: Commands,
    mut q: Query<(Entity, &Orbit, &mut Orbital), Added<Orbit>>,
) {
    for (entity, init, mut orbital) in &mut q {
        match init {
            Orbit::FromElements(elements) => {
                let epoch = Epoch::now();
                let propagator = KeplerianPropagator::from_keplerian(
                    epoch,
                    *elements,
                    brahe::AngleFormat::Radians,
                    1.0,
                );
                if let Ok(eci) = propagator.state_eci(epoch) {
                    orbital.propagator = Some(KeplerianPropagator::from_eci(epoch, eci, 1.0));
                    // println!("ECI Initialized to: {}", eci);
                    eci
                } else {
                    Vector6::new(0.0, 0.0, 0.0, 0.0, 0.0, 0.0)
                }
            }
            Orbit::FromTle(_tle) => {
                // TODO: init logic from TLE data (sgp4)
                Vector6::new(0.0, 0.0, 0.0, 0.0, 0.0, 0.0)
            }
        };

        commands.entity(entity).remove::<Orbit>();
    }
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
        app.add_systems(First, init_orbitals.run_if(in_state(UiScreen::Sim)))
            .add_systems(ManualPhysics, cache_eci_states.in_set(SimStep::CacheEci));
    }
}
