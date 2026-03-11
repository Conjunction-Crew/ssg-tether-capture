---
sidebar_position: 4
---

# Time Warp

The `TimeWarp` resource controls how quickly simulation time advances relative to real time.

## TimeWarp resource

```rust
pub struct TimeWarp {
    pub multiplier: f64,
}
```

The default multiplier is `1.0` (real time). A multiplier of `60.0` makes one second of real time equal to one minute of simulation time.

## Changing time warp

The `change_time_warp` system (registered in `lib.rs` `Update`) responds to key input:

| Key | Effect |
|---|---|
| `,` (comma) | Decrease multiplier |
| `.` (period) | Increase multiplier |

## Effect on propagation

The `ssg_propagate_keplerian` system reads `TimeWarp::multiplier` each frame and scales the propagation delta time accordingly:

```
propagation_dt = real_dt * time_warp.multiplier
```

All Keplerian positions are advanced by `propagation_dt` each frame, so increasing the multiplier makes orbits visibly faster.

## Effect on physics

Avian3D physics runs on `FixedPostUpdate` and uses the engine's fixed timestep. Time warp does **not** directly scale the Avian3D simulation — only the orbital propagation (Keplerian) step is affected. This is intentional: tether dynamics are a local physics simulation, while orbital propagation is an analytical calculation that can be freely time-scaled.
