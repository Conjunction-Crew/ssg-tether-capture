---
sidebar_position: 1
---

# Orbital Mechanics

Orbital mechanics is the simulation's foundation. All object positions and velocities are computed from orbital state, then translated into Bevy world-space coordinates.

## Orbit initialisation

Each orbital body is spawned with an `Orbit` enum that declares how its initial state should be computed:

```rust
pub enum Orbit {
    FromElements(Vector6<f64>),                                // [a, e, i, Ω, ω, M] at the start epoch
    FromElementsAt { elements: Vector6<f64>, epoch: Epoch },   // the same, valid at `epoch`
    FromGp(GpElements),                                        // a catalog GP record (full precision)
}
```

The `init_orbitals` system (run in `First` by `OrbitPlugin`) reads newly-spawned `Orbit` components, builds a `brahe::KeplerianPropagator` that starts at the simulation start epoch (`WorldTime::start_epoch`), stores it in the entity's `Orbital` component, and removes the `Orbit` component:

- **Catalog objects** (`FromGp`) start from their **SGP4** state at the start epoch, computed by brahe's `SGPPropagator` from the full-precision (f64) GP record and the record's own `EPOCH`. A target or chaser picked from the catalog carries that record; editing any of its elements with the sliders drops it, and the body then starts from the edited elements.
- **Custom or edited elements** (`FromElementsAt`) are osculating Keplerian elements valid at their own epoch; they are propagated (two-body) from that epoch to the start epoch. A custom orbit takes the start epoch as its epoch.
- From then on every body is propagated with the **two-body** (Kepler) model. SGP4 only supplies the initial state: it cannot be re-seeded from an ECI state at a floating-origin reset or a bubble transition, and handing over from SGP4 to Kepler mid-run would put a model jump into the trajectory.

SGP4 produces TEME states; brahe rotates them to GCRF (the sim's ECI frame) with Earth-orientation parameters. `OrbitPlugin` installs brahe's static **zero** EOP provider (`StaticEOPProvider::from_zero()`): it is offline and deterministic, and both bodies of a pair get the same rotation, so their relative state is unaffected (the absolute error is at the polar-motion level, about 10 m). Never call `brahe::initialize_eop()`: it reads EOP files or the network and makes runs non-repeatable.

The catalog map points are a separate, lightweight model: the GPU propagates them with Kepler from f32 mean elements, so they sit tens of kilometres from the SGP4-initialised sim bodies (about 47 km for the Hubble Space Telescope at the dataset epoch). This mismatch is known and documented; a follow-up issue proposes numerical propagation (J2, drag) for the sim and a matching model for the map.

## JSON orbital datasets

Large collections of debris objects are loaded from JSON files in `assets/datasets/` using the `JsonOrbitalData` / `JsonOrbital` deserialization types. The JSON format matches the GP element-set schema used by Space-Track.org (fields like `OBJECT_NAME`, `NORAD_CAT_ID`, `MEAN_MOTION`, `ECCENTRICITY`, etc.).

The `load_dataset_entities` system runs on `OnEnter(UiScreen::Sim)`, parses all `.json` files in `assets/datasets/`, and registers each object in `SpaceObjectCatalog`, keeping the record's mean elements at full precision with its own epoch (`SpaceCatalogEntry::gp`). The f32 element buffer derived from them is uploaded to the GPU via `GpuComputePlugin` for efficient rendering as instanced map-view points — no individual ECS entity is spawned per debris object.

## ECI state caching

Rather than each system querying the `KeplerianPropagator` directly, the `cache_eci_states` system runs once per `ManualPhysics` tick and writes each entity's current ECI `Vector6<f64>` into `OrbitalCache::eci_states`. All other systems — physics bubble management, capture algorithms, map view positioning — read from this cache.

The simulation's own systems run **after** Avian3D's step (`PhysicsSystems::Last`), chained in this order: `SimStep::CenterOfMass` → `OriginReset` → `CacheEci` → `Bubble` → `RelativeDynamics` → `Capture`. The bookkeeping describes the state after the step; the two force producers (relative gravity and the capture controller) then act during the next step from that consistent snapshot.

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

These are used during initialisation when an entity is spawned `FromElements` or `FromElementsAt`, and to re-seed a propagator from the SGP4 state (`from_eci`).

## Approach metrics

Relative position, velocity, range, closing speed, and predicted closest-approach distance between the tether and a debris target are computed each physics tick by `capture_state_machine_update` from the bodies' bubble-frame states and the cached ECI states in `OrbitalCache::eci_states`. The relative velocity is measured in the debris target's LVLH frame (`orbit::lvlh`): a body at rest relative to the co-rotating target frame reads zero, not the few m/s that the plain ECI difference shows at kilometre range; the HUD, the plots, the CSV samples and `ssg-sim` use the same measure. These values drive the transition conditions in a `CompiledCapturePlan`: each physics step, the current state's transitions are checked in plan order, a transition fires only when all of its conditions hold, and at most one transition fires per step (the first satisfied one).

## brahe

[`brahe`](https://github.com/duncaneddy/brahe) is the underlying orbital mechanics library. It provides:

- `KeplerianPropagator` — per-entity two-body propagator; initialised from Keplerian elements (`from_keplerian`) or an ECI state vector (`from_eci`)
- `SGPPropagator` — SGP4/SDP4 from GP (OMM) mean elements; used once per catalog body for its initial state
- `Epoch` — time representation used for propagation queries
- `DOrbitStateProvider` — trait implemented by propagators, providing `state_eci()` and `state_koe_osc()`
- `AngleFormat` — enum controlling whether angles are expressed in radians or degrees

State vectors are represented as `nalgebra::Vector6<f64>` (3 position components followed by 3 velocity components).
