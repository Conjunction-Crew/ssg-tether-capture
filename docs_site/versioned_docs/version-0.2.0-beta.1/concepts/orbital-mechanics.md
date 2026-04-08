---
sidebar_position: 1
---

# Orbital Mechanics

Orbital mechanics is the simulation's foundation. All object positions and velocities are computed from orbital state, then translated into Bevy world-space coordinates.

## Orbit initialisation

Each orbital body is spawned with an `Orbit` enum that declares how its initial state should be computed:

```rust
pub enum Orbit {
    FromTle(String),          // Two-Line Element set string
    FromElements(OrbitalElements), // Classical orbital elements
    FromParams(TrueParams),   // Raw position/velocity vectors
}
```

The `init_orbitals` system (run in `PreUpdate` by `OrbitalMechanicsPlugin`) reads newly-spawned `Orbit` components and populates the corresponding `Orbital` and `TrueParams` components.

## TrueParams

`TrueParams` stores the canonical position and velocity of an entity in an Earth-Centered Inertial (ECI) reference frame, in metres and metres per second:

```rust
pub struct TrueParams {
    pub r: [f64; 3],   // position (m)
    pub v: [f64; 3],   // velocity (m/s)
}
```

All other positional representations (Bevy `Transform`, map view position, approach metrics) are derived from `TrueParams`.

## Keplerian propagation

Each simulation step, `ssg_propagate_keplerian` advances every active `Orbital` entity forward in time:

1. Iterates over all active orbitals, each of which owns a `brahe::KeplerianPropagator` instance stored in the `OrbitalCache` resource.
2. Calls `propagator.state_eci(epoch)` — where `epoch` is derived from `WorldTime` scaled by `TimeWarp::multiplier` — to obtain a `nalgebra::Vector6<f64>` containing the current ECI position and velocity.
3. Writes the updated position/velocity back into each entity's `TrueParams`.

Keplerian propagation assumes two-body dynamics (no perturbations). This is accurate for short time spans and illustrative for longer ones.

## COE ↔ RV conversion

`brahe` handles orbital element conversion through the `KeplerianPropagator` API:

- **Elements → state**: `KeplerianPropagator::from_keplerian(epoch, elements, AngleFormat::Radians, gm)` initialises a propagator directly from classical orbital elements (semi-major axis, eccentricity, inclination, RAAN, argument of perigee, true anomaly).
- **State → elements**: `propagator.state_koe_osc(epoch, AngleFormat::Radians)` returns the osculating Keplerian elements as a fixed-size array `[f64; 6]`.

These are used during initialisation when an entity is spawned `FromElements`.

## Approach metrics

The `Orbital` component carries an `ApproachMetrics` struct that is updated each frame:

| Field | Description |
|---|---|
| `relative_position_km` | Position of this object relative to the primary target |
| `relative_velocity_km_s` | Velocity relative to the primary target |
| `range_km` | Current separation distance |
| `closing_speed_km_s` | Rate of change of range (negative = approaching) |
| `time_to_closest_approach_s` | Predicted time until closest approach (if converging) |
| `closest_approach_distance_km` | Predicted closest approach distance |

## brahe

[`brahe`](https://github.com/duncaneddy/brahe) is the underlying orbital mechanics library. It provides:

- `KeplerianPropagator` — per-entity two-body propagator; initialised from Keplerian elements (`from_keplerian`) or an ECI state vector (`from_eci`)
- `Epoch` — time representation used for propagation queries
- `DOrbitStateProvider` — trait implemented by propagators, providing `state_eci()` and `state_koe_osc()`
- `AngleFormat` — enum controlling whether angles are expressed in radians or degrees

State vectors are represented as `nalgebra::Vector6<f64>` (3 position components followed by 3 velocity components).
