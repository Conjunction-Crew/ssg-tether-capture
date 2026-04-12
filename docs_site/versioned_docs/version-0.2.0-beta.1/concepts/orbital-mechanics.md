---
sidebar_position: 1
---

# Orbital Mechanics

Orbital mechanics is the simulation's foundation. All object positions and velocities are computed from orbital state, then translated into Bevy world-space coordinates.

## Orbit initialisation

Each orbital body is spawned with an `Orbit` enum that declares how its initial state should be computed:

```rust
pub enum Orbit {
    FromTle(String),              // Two-Line Element set string
    FromElements(Vector6<f64>),   // Classical orbital elements [a, e, i, Ω, ω, M]
}
```

The `init_orbitals` system (run in `First` by `OrbitalMechanicsPlugin`) reads newly-spawned `Orbit` components, constructs a `brahe::KeplerianPropagator`, stores it in the entity's `Orbital` component, and removes the `Orbit` component.

## JSON orbital datasets

Large collections of debris objects are loaded from JSON files in `assets/datasets/` using the `JsonOrbitalData` / `JsonOrbital` deserialization types. The JSON format matches the GP element-set schema used by Space-Track.org (fields like `OBJECT_NAME`, `NORAD_CAT_ID`, `MEAN_MOTION`, `ECCENTRICITY`, etc.).

The `load_dataset_entities` system runs on `OnEnter(UiScreen::Sim)`, parses all `.json` files in `assets/datasets/`, and registers each object in `SpaceObjectCatalog`. The orbital state for each object is uploaded to the GPU via `GpuComputePlugin` for efficient rendering as instanced map-view points — no individual ECS entity is spawned per debris object.

## ECI state caching

Rather than each system querying the `KeplerianPropagator` directly, the `cache_eci_states` system runs once per `ManualPhysics` tick (in `PhysicsSystems::First`) and writes each entity's current ECI `Vector6<f64>` into `OrbitalCache::eci_states`. All other systems — physics bubble management, capture algorithms, map view positioning — read from this cache.

## Keplerian propagation

Each `ManualPhysics` tick, `cache_eci_states` advances every active `Orbital` entity forward in time:

1. Iterates over all entities with an `Orbital` component that has a propagator.
2. Calls `propagator.state_eci(epoch)` — where `epoch` is the current `WorldTime::epoch` — to obtain a `nalgebra::Vector6<f64>` containing the current ECI position and velocity.
3. Writes the updated state into `OrbitalCache::eci_states`.

The `WorldTime::epoch` is advanced by `fixed_physics_step` each `FixedUpdate` tick, scaled by `WorldTime::multiplier`.

## COE ↔ RV conversion

`brahe` handles orbital element conversion through the `KeplerianPropagator` API:

- **Elements → state**: `KeplerianPropagator::from_keplerian(epoch, elements, AngleFormat::Radians, gm)` initialises a propagator directly from classical orbital elements (semi-major axis, eccentricity, inclination, RAAN, argument of perigee, true anomaly).
- **State → elements**: `propagator.state_koe_osc(epoch, AngleFormat::Radians)` returns the osculating Keplerian elements as a fixed-size array `[f64; 6]`.

These are used during initialisation when an entity is spawned `FromElements`.

## Approach metrics

Relative position, velocity, range, closing speed, and predicted closest-approach distance between the tether and a debris target are computed each physics tick by `capture_state_machine_update` directly from the cached ECI states in `OrbitalCache::eci_states`. These values drive the transition conditions in a `CompiledCapturePlan`.

## brahe

[`brahe`](https://github.com/duncaneddy/brahe) is the underlying orbital mechanics library. It provides:

- `KeplerianPropagator` — per-entity two-body propagator; initialised from Keplerian elements (`from_keplerian`) or an ECI state vector (`from_eci`)
- `Epoch` — time representation used for propagation queries
- `DOrbitStateProvider` — trait implemented by propagators, providing `state_eci()` and `state_koe_osc()`
- `AngleFormat` — enum controlling whether angles are expressed in radians or degrees

State vectors are represented as `nalgebra::Vector6<f64>` (3 position components followed by 3 velocity components).
