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

The `init_orbitals` system (run in `First` by `OrbitPlugin`) reads newly-spawned `Orbit` components, builds a two-body propagator (`TwoBody`, in `orbit::kepler`) that starts at the simulation start epoch (`WorldTime::start_epoch`), stores it in the entity's `Orbital` component, and removes the `Orbit` component:

- **Catalog objects** (`FromGp`) start from their **SGP4** state at the start epoch, computed by SGP4 (`orbit::sgp4_start::teme_state`) from the full-precision (f64) GP record and the record's own `EPOCH`. A target or chaser picked from the catalog carries that record; editing any of its elements with the sliders drops it, and the body then starts from the edited elements.
- **Custom or edited elements** (`FromElementsAt`) are osculating Keplerian elements valid at their own epoch; they are propagated (two-body) from that epoch to the start epoch. A custom orbit takes the start epoch as its epoch.
- From then on every body is propagated with the **two-body** (Kepler) model. SGP4 only supplies the initial state: it cannot be re-seeded from an ECI state at a floating-origin reset or a bubble transition, and handing over from SGP4 to Kepler mid-run would put a model jump into the trajectory.

SGP4 produces TEME states; `orbit::frames::teme_to_gcrf` rotates them to GCRF (the sim's ECI frame): a GMST rotation to the pseudo-Earth-fixed frame, polar motion, then the Earth rotation angle and the IAU 2006/2000A precession-nutation (translated from ERFA), with zero Earth-orientation parameters. The times it needs come from brahe's `Epoch`, so `OrbitPlugin` installs brahe's static **zero** EOP provider (`StaticEOPProvider::from_zero()`): it is offline and deterministic, and both bodies of a pair get the same rotation, so their relative state is unaffected (the absolute error is at the polar-motion level, about 10 m). Never call `brahe::initialize_eop()`: it reads EOP files or the network and makes runs non-repeatable.

The catalog map points are a separate, lightweight model: the GPU propagates them with Kepler from f32 mean elements, so they sit tens of kilometres from the SGP4-initialised sim bodies (about 47 km for the Hubble Space Telescope at the dataset epoch). This mismatch is known and documented; a follow-up issue proposes numerical propagation (J2, drag) for the sim and a matching model for the map. A catalog target or chaser you pick is shown at the sim's start epoch: its card, orbit line and marker give the osculating elements of its SGP4 state there, so they sit where the body starts, and moving a slider starts the body from the elements shown. The orbit line and box of the catalog row you are browsing stay on the map points' model.

## JSON orbital datasets

Large collections of debris objects are loaded from JSON files in `assets/datasets/` using the `JsonOrbitalData` / `JsonOrbital` deserialization types. The JSON format matches the GP element-set schema used by Space-Track.org (fields like `OBJECT_NAME`, `NORAD_CAT_ID`, `MEAN_MOTION`, `ECCENTRICITY`, etc.).

The `load_dataset_entities` system runs on entering the sim session (`OnEnter(SimSession::Active)`, in `SimSet::SessionInit`), parses all `.json` files in `assets/datasets/`, and registers each object in `SpaceObjectCatalog`, keeping the record's mean elements at full precision with its own epoch (`SpaceCatalogEntry::gp`). The f32 element buffer derived from them is uploaded to the GPU via `GpuComputePlugin` for efficient rendering as instanced map-view points — no individual ECS entity is spawned per debris object.

## ECI state caching

Rather than each system querying the propagator directly, the `cache_eci_states` system runs once per `ManualPhysics` tick and writes each entity's current ECI `Vector6<f64>` into `OrbitalCache::eci_states`. All other systems — physics bubble management, capture algorithms, map view positioning — read from this cache.

The simulation's own systems run **after** Avian3D's step (`PhysicsSystems::Last`), chained in this order: `SimStep::CenterOfMass` → `OriginReset` → `CacheEci` → `Bubble` → `RelativeDynamics` → `Capture` → `Record`. The bookkeeping describes the state after the step; the two force producers (relative gravity and the capture controller) then act during the next step from that consistent snapshot; the recorder only reads.

## Keplerian propagation

Each `ManualPhysics` tick, `cache_eci_states` advances every active `Orbital` entity forward in time:

1. Iterates over all entities with an `Orbital` component that has a propagator.
2. Calls `propagator.state_eci(epoch)` — where `epoch` is the current `WorldTime::epoch` — to obtain a `nalgebra::Vector6<f64>` containing the current ECI position and velocity.
3. Writes the updated state into `OrbitalCache::eci_states`.

`fixed_physics_step` advances `WorldTime::epoch` by `PHYS_DT` (1/64 s) before each fixed step; a `FixedUpdate` run executes `WorldTime::multiplier` steps, at most `StepCap` ([Time Warp](./time-warp)).

## Elements and states

`TwoBody` keeps the ECI state it is given and advances it with the universal-variable form of Kepler's equation; it holds no orbital elements, so a near-circular orbit keeps its small eccentricity, and the state at the propagator's own epoch is the stored state, bit for bit. `orbit::kepler` converts between orbital elements and ECI states:

- **Elements → propagator:** `init_orbitals` builds a `TwoBody` with `from_elements` from the elements `[a, e, i, Ω, ω, M]` (metres, radians, mean anomaly) at their epoch, takes its ECI state at the start epoch, and builds the body's propagator from that state with `from_eci`. A catalog record's SGP4 start state goes through `from_eci` the same way.
- **Re-seeding:** the physics bubble and the floating-origin reset rebuild propagators from ECI states with `from_eci`.
- **State → elements:** `state_to_elements` (from the eccentricity vector, without rounding) gives the osculating elements of the chaser placed relative to the target and of a catalog pick at the start epoch; the map's orbit lines use `elements_at` on a body's propagator at the current epoch.

Every transcendental function on these paths, and in the rest of the simulation's own code, goes through `ssg_sim::math`, which calls the `libm` crate, so the same input gives the same bits on Linux, macOS and Windows.

## Approach metrics

Each physics step, `capture_state_machine_update` measures the tether root against the debris target from the bodies' bubble-frame states and the cached ECI states in `OrbitalCache::eci_states`: the range, the relative speed, the closing rate, the tether's straightness and the time in the current phase. The relative velocity is measured in the debris target's LVLH frame (`orbit::lvlh`): a body at rest relative to the co-rotating target frame reads zero, not the few m/s that the plain ECI difference shows at kilometre range; the HUD, the plots, the CSV samples and `ssg-sim` use the same measure. These values drive the transition conditions in a `CompiledCapturePlan`: each physics step, the current state's transitions are checked in plan order, a transition fires only when all of its conditions hold, and at most one transition fires per step (the first satisfied one).

## Libraries

- [`brahe`](https://github.com/duncaneddy/brahe) provides `Epoch` (the time representation, with its UTC, UT1 and TT conversions), the static Earth-orientation provider and constants such as `GM_EARTH`. Its propagators are used only in tests, as references.
- `sgp4p` (the workspace crate `crates/sgp4-portable`) is a copy of the `sgp4` crate 2.3.0 whose math calls go through `libm`; `orbit::sgp4_start` builds its elements from a GP record.

State vectors are represented as `nalgebra::Vector6<f64>` (3 position components followed by 3 velocity components).
