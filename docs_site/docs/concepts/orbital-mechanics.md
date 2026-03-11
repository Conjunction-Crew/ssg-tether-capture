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

1. Collects all active orbitals into a batch buffer (`BatchScratch`) to minimise allocation.
2. Calls `astrora_core::propagators::keplerian::batch_propagate_states` with the current time step (scaled by `TimeWarp::multiplier`).
3. Writes updated position/velocity back into each entity's `TrueParams`.

Keplerian propagation assumes two-body dynamics (no perturbations). This is accurate for short time spans and illustrative for longer ones.

## COE ↔ RV conversion

`astrora_core` provides `coe_to_rv` and `rv_to_coe` to convert between classical orbital elements and Cartesian state vectors. These are used during initialisation when an entity is spawned `FromElements`.

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

## astrora_core

The `astrora_core` crate is the underlying orbital mechanics library. It provides:

- Orbital element types (`OrbitalElements`)
- Gravitational constants (`GM_EARTH`)
- Keplerian batch propagator
- Linear algebra primitives (`Vector3`)
