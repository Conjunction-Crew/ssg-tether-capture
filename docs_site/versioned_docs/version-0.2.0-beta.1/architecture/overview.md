---
sidebar_position: 1
---

# Architecture Overview

SSG Tether Capture is a [Bevy](https://bevyengine.org/) application that follows the **Entity Component System (ECS)** pattern. All simulation state lives in components attached to entities; logic runs as systems; shared mutable state is held in resources.

## Entry point

```
main.rs  →  lib.rs::run()  →  App::new()
```

`main.rs` is a thin wrapper. All shared logic — including everything needed by the test harness — lives in `lib.rs`.

`lib.rs` exposes two functions:

| Function | Purpose |
|---|---|
| `run()` | Full application entry point. Adds `DefaultPlugins`, `UiPlugin`, rendering setup, and startup systems, then calls `app.run()`. |
| `create_app()` | Returns a partially-configured `App` with only shared plugins (physics, orbital mechanics, camera). Used by integration tests. |

## Module structure

```
src/
  constants.rs        — shared numeric constants and camera layer indices
  lib.rs              — run() and create_app() entry points
  main.rs             — binary entry point
  components/         — ECS components (data attached to entities)
  plugins/            — Bevy Plugin impls that register systems
  resources/          — global shared state (ECS Resources)
  systems/            — all system functions
  ui/                 — Bevy-native UI layer (plugin, screens, state, theme)
  tests/              — integration and unit tests
```

## Plugin graph

The following plugins are added to the app:

```
PhysicsPlugins          (Avian3D rigid-body physics — on custom ManualPhysics schedule)
OrbitalMechanicsPlugin  (orbital init, physics stepping, capture algorithms)
OrbitCameraPlugin       (camera input + tracking — Update/PostUpdate)
GpuComputePlugin        (GPU compute pipeline for large orbital datasets)
UiPlugin                (UI camera, state machine, screen lifecycle)
DefaultPlugins          (Bevy standard — only added in run(), not in tests)
```

See [Plugins](./plugins) for what each plugin registers.

## System schedule

| Schedule | Systems registered |
|---|---|
| `OnEnter(UiScreen::Sim)` | `setup_lighting`, `setup_celestial`, `setup_tether`, `setup_entities`, `load_dataset_entities` (chained) |
| `First` | `init_orbitals` |
| `FixedUpdate` | `fixed_physics_step` (drives the `ManualPhysics` schedule at `FIXED_HZ`) |
| `Update` | `toggle_map_view`, `toggle_origin`, `toggle_capture_gizmos`, `change_time_warp`, `map_orbitals`, `floating_origin_update_visuals`, `dev_gizmos`, `capture_gizmos`, camera input systems, UI update systems |
| `PostUpdate` | `orbit_camera_track`, GPU compute sync systems |
| `ManualPhysics` | `cache_eci_states` (First) → `physics_bubble_add_remove`, `target_entity_reset_origin` (Prepare) → Avian3D physics → `capture_state_machine_update` (Last) |
| `Last` | `orbital_gizmos` |

## External dependencies

| Crate | Role |
|---|---|
| `bevy` | Game engine / ECS framework |
| `avian3d` | Rigid-body physics |
| `brahe` | Orbital mechanics (propagation, COE↔RV conversion) |
| `nalgebra` | 6-element state vectors (`Vector6`) for ECI position/velocity |
