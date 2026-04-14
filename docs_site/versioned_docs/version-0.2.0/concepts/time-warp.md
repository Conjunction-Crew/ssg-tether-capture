---
sidebar_position: 4
---

# Time Warp

The `WorldTime` resource controls both the current simulation epoch and how quickly simulation time advances relative to real time.

## WorldTime resource

```rust
pub struct WorldTime {
    pub multiplier: u32, // default: 1
    pub epoch: Epoch,    // default: Epoch::now()
}
```

The default multiplier is `1` (real time). A multiplier of `60` makes one fixed tick advance the simulation epoch by 60× the normal timestep.

## Changing time warp

The `change_time_warp` system (registered in `lib.rs` `Update`) responds to key input:

| Key | Effect |
|---|---|
| `,` (comma) | Decrease multiplier |
| `.` (period) | Increase multiplier |

The multiplier is a `u32` — it steps in whole-number increments.

## Effect on propagation

The `fixed_physics_step` system advances `WorldTime::epoch` each `FixedUpdate` tick. The time delta applied is `fixed_dt * multiplier`. The `cache_eci_states` system then calls `propagator.state_eci(epoch)` with the updated epoch, so all Keplerian positions advance at the scaled rate.

## Effect on physics

Avian3D physics runs inside the `ManualPhysics` schedule, which is driven by `fixed_physics_step`. The physics timestep itself is fixed at `FIXED_HZ` regardless of `multiplier`; only the orbital epoch advances faster. This is intentional: tether dynamics are a local physics simulation that must remain stable, while orbital propagation is an analytical calculation that can be freely time-scaled.
