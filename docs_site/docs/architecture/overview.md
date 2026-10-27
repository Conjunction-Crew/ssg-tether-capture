---
sidebar_position: 1
---

# Architecture Overview

SSG Tether Capture is a [Bevy](https://bevyengine.org/) application that follows the **Entity Component System (ECS)** pattern. All simulation state lives in components attached to entities; logic runs as systems; shared mutable state is held in resources.

## Workspace layout

The repository is a Cargo workspace of three crates (the version is shared through `[workspace.package]`):

```
crates/
  ssg-sim/        — headless simulation core (no render, window or UI types)
    src/
      session.rs          — UiScreen / SimState, session resources
      time.rs             — WorldTime, the fixed-step driver
      project.rs          — projects, capture-plan library, catalogs
      telemetry.rs        — LogEvent, capture log, data collection
      orbit/              — orbital propagation, cache, catalog, selection
      anchor.rs           — the camera/anchor target
      floating_origin.rs  — floating-origin reset
      physics_bubble.rs   — the physics bubble
      tether.rs           — tether and debris bodies
      capture/            — capture plans and the capture state machine
      test_support/       — shared test harness (feature "test-support")
  ssg-sim-cli/    — `ssg-sim run`, the headless runner (see Getting Started → Headless CLI)
  ssg-app/        — the desktop application (the only crate that renders)
    src/
      render/             — meshes, materials, cameras, gizmos, GPU catalog
      ui/                 — Bevy-native UI layer (plugin, screens, state, theme)
      lib.rs              — run(): the entry point
      main.rs             — binary entry point
```

`ssg-app` depends on `ssg-sim`; `ssg-sim` depends on neither of the other two. `ssg-sim-cli` depends on `ssg-sim`, not on `ssg-app`.

## Entry point

```
main.rs  →  lib.rs::run()  →  App::new()
```

`run()` adds Bevy's `DefaultPlugins` and the UI-widget and egui plugins, inserts the `AssetsDir` resource, and then adds the three plugin groups below. The headless runner and the tests build the same simulation with `SimPlugins` alone.

## Plugin graph

| Group | Crate | Contents |
|---|---|---|
| `SimPlugins` | `ssg-sim` | One plugin per feature module: session, time, project, telemetry, orbit, anchor, floating origin, physics bubble, tether, capture. Add after `StatesPlugin`. |
| `RenderPlugins` | `ssg-app` | `CelestialPlugin`, `OrbitCameraPlugin`, `AttachPlugin`, `GizmoPlugin`, `GpuComputePlugin`. Add after `SimPlugins`. |
| `UiPlugin` | `ssg-app` | UI camera, screen lifecycle, forms, HUD. |

See [Plugins](./plugins) for what each group registers.

### Order of the `ManualPhysics` step

`TimePlugin` configures the `SimStep` sets in `ManualPhysics`. They are chained in this order, and all of them run after Avian3D's step (`PhysicsSystems::Last`). This is the order the systems have today, now named; it is stated here as it is, not as a design claim.

| # | `SimStep` | System |
|---|---|---|
| 1 | `SyncAnchor` | `sync_anchor_from_camera_target` |
| 2 | `CenterOfMass` | `calculate_com_rv` |
| 3 | `OriginReset` | `target_entity_reset_origin` |
| 4 | `CacheEci` | `cache_eci_states` |
| 5 | `Bubble` | `physics_bubble_add_remove` |
| 6 | `Capture` | `capture_state_machine_update` |

## System schedule

| Schedule | Systems registered |
|---|---|
| `OnEnter(UiScreen::Sim)` | `setup_lighting`, `setup_celestial`, `setup_tether`, `setup_entities`, `load_dataset_entities` (chained) |
| `First` | `init_orbitals` |
| `FixedUpdate` | `fixed_physics_step` (drives the `ManualPhysics` schedule at `FIXED_HZ`) |
| `Update` | `toggle_map_view`, `toggle_origin`, `toggle_capture_gizmos`, `change_time_warp`, `map_orbitals`, `floating_origin_update_visuals`, `dev_gizmos`, `capture_gizmos`, camera input systems, UI update systems |
| `PostUpdate` | `orbit_camera_track`, GPU compute sync systems |
| `ManualPhysics` | Avian3D physics, then the `SimStep` chain (see above) |
| `Last` | `orbital_gizmos` |

## External dependencies

| Crate | Role |
|---|---|
| `bevy` | Game engine / ECS framework |
| `avian3d` | Rigid-body physics |
| `brahe` | Orbital mechanics (propagation, COE↔RV conversion) |
| `nalgebra` | 6-element state vectors (`Vector6`) for ECI position/velocity |
