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
PhysicsPlugins          (Avian3D rigid-body physics)
OrbitalMechanicsPlugin  (orbital initialisation — PreUpdate)
OrbitCameraPlugin       (camera input + tracking — Update/PostUpdate)
UiPlugin                (UI camera, state machine, screen lifecycle)
DefaultPlugins          (Bevy standard — only added in run(), not in tests)
```

See [Plugins](./plugins) for what each plugin registers.

## System schedule

| Schedule | Systems registered |
|---|---|
| `Startup` | `setup_lighting`, `setup_celestial`, `setup_tether`, `setup_entities` (chained) |
| `PreUpdate` | `init_orbitals` |
| `Update` | `ssg_propagate_keplerian`, `toggle_map_view`, `toggle_origin`, `change_time_warp`, `track_objects`, `map_orbitals`, camera input systems |
| `PostUpdate` | `floating_origin`, `orbit_camera_track` |
| `FixedPostUpdate` | Avian3D physics systems |
| `Last` | `orbital_gizmos` |

## External dependencies

| Crate | Role |
|---|---|
| `bevy` | Game engine / ECS framework |
| `avian3d` | Rigid-body physics |
| `brahe` | Orbital mechanics (propagation, COE↔RV conversion) |
| `nalgebra` | 6-element state vectors (`Vector6`) for ECI position/velocity |
