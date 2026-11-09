---
sidebar_position: 1
---

# Architecture Overview

SSG Tether Capture is a [Bevy](https://bevyengine.org/) application built on the **Entity Component System (ECS)** pattern: simulation state lives in components on entities and in resources, and systems hold the logic. The simulation runs headless in its own crate; the desktop app adds rendering and UI on top of it.

## Workspace layout

The repository is a Cargo workspace of three crates. They share one version through `[workspace.package]`.

```
crates/
  ssg-sim/          — the simulation core: no render, window or UI types
    src/
      lib.rs              — SimPlugins, the SimStep and SimSet system sets
      session.rs          — SimSession, SimState, PauseState; per-session resources
      time.rs             — WorldTime, SimConfig, StepCap, the ManualPhysics schedule
      project.rs, project/ — projects, the working directory, plan files, manifests, exports
      telemetry.rs        — LogEvent, CaptureLog, DataCollection
      orbit/              — Orbital/Orbit, OrbitalCache, the catalog, LVLH, the orbit selection
      anchor.rs           — SimAnchor, the body the floating origin follows
      floating_origin.rs  — centre of mass and the floating-origin reset
      physics_bubble.rs   — the physics bubble and relative gravity
      tether.rs           — spawning the tether and the debris
      capture/            — capture plans, controller gains, the capture state machine
      recording/          — the input log, the recorder, recording files, the player
      test_support/       — the shared test harness (feature "test-support")
  ssg-sim-cli/      — the `ssg-sim` binary: run, validate, replay
  ssg-app/          — the desktop application, the only crate that renders
    src/
      main.rs             — the `ssg_tether_capture` binary
      lib.rs              — run(): builds the App
      render/             — cameras, celestial bodies, body visuals, gizmos, the GPU catalog
      ui/                 — screens, windows, overlays, input, the screen state
```

**Dependency rule.** `ssg-sim` depends on neither of the other crates and uses no render, window or UI type. `ssg-sim-cli` and `ssg-app` each depend on `ssg-sim`, never on each other.

## Entry points

| Entry point | Crate | What it builds |
|---|---|---|
| `ssg_tether_capture` binary | `ssg-app` | `main()` calls `ssg_app::run()`: Bevy's `DefaultPlugins` (with the resolved `assets/` path), `EguiPlugin`, `AutoExposurePlugin`, the `AssetsDir` and `SimConfig` resources, then `SimPlugins`, `RenderPlugins` and `UiPlugin` |
| `ssg-sim` binary | `ssg-sim-cli` | `run <project-dir>`, `validate <project-dir>`, `replay <recording-dir>` (see [Headless CLI](../getting-started/headless-cli)). It builds `MinimalPlugins`, `StatesPlugin`, `AssetPlugin` and `SimPlugins`, with no window |
| `SimPlugins` | `ssg-sim` | The simulation as a plugin group, used by the app, the CLI and the test harness alike. Add it after `StatesPlugin` |

`run()` inserts `SimConfig` with the current time as the start epoch until a project is opened; opening a project sets it again. The simulation itself never reads the wall clock.

## Plugin groups

| Group | Crate | Contents |
|---|---|---|
| `SimPlugins` | `ssg-sim` | `SessionPlugin`, `TimePlugin`, `ProjectPlugin`, `TelemetryPlugin`, `OrbitPlugin`, `FloatingOriginPlugin`, `PhysicsBubblePlugin`, `TetherPlugin`, `CapturePlugin`, `RecordingPlugin` |
| `RenderPlugins` | `ssg-app` | `CelestialPlugin`, `OrbitCameraPlugin`, `AttachPlugin`, `GizmoPlugin`, `GpuComputePlugin`. Add after `SimPlugins` |
| `UiPlugin` | `ssg-app` | `ScreenPlugin`, the UI resources, the egui screens and windows, the overlays, keyboard input |

See [Plugins](./plugins) for what each plugin registers.

## The fixed step

`TimePlugin` runs `fixed_physics_step` in `FixedUpdate` at `FIXED_HZ` (64 Hz) while the session is active, the sim is running and it is not paused. Each run executes `min(WorldTime::multiplier, StepCap)` fixed steps; each step advances `WorldTime::epoch` by `PHYS_DT` (1/64 s) and runs the `ManualPhysics` schedule once. Steps are never carried over to the next run.

`ManualPhysics` holds Avian3D's `PhysicsPlugins` (gravity zero, 8 substeps) and then the `SimStep` sets, chained in this order and all inside `PhysicsSystems::Last`, after the physics step:

| # | `SimStep` | System | Plugin |
|---|---|---|---|
| 1 | `CenterOfMass` | `calculate_com_rv` | `FloatingOriginPlugin` |
| 2 | `OriginReset` | `target_entity_reset_origin` | `FloatingOriginPlugin` |
| 3 | `CacheEci` | `cache_eci_states` | `OrbitPlugin` |
| 4 | `Bubble` | `physics_bubble_add_remove` | `PhysicsBubblePlugin` |
| 5 | `RelativeDynamics` | `apply_relative_gravity` | `PhysicsBubblePlugin` |
| 6 | `Capture` | `capture_state_machine_update` (then `assert_finite_state` in debug builds) | `CapturePlugin` |
| 7 | `Record` | `record_step` | `RecordingPlugin` |

The bookkeeping steps (1–4) describe the state after the physics step. The two force producers (5, 6) then act on the next physics step from that consistent snapshot. The recorder (7) only reads the step's end state.

## Schedules

| Schedule | What runs there |
|---|---|
| `Startup` | `configure_catch_up_window` (Bevy's catch-up window lowered to 3 fixed steps); the UI camera |
| `StateTransition` | `drive_sim_session`, which mirrors the app's `UiScreen` into `SimSession` |
| `OnEnter(SimSession::Active)` | `SimSet::SessionInit`: `init_sim_resources`, `load_dataset_entities`, `setup_time` (chained); the recorder and input-log resets before it |
| `OnEnter(UiScreen::Sim)` | Celestials, lights and the camera (`CelestialPlugin`); the HUD readouts and the overlays; UI state resets |
| `OnEnter(SimState::Running)` | `SimSet::SpawnBodies`: `spawn_selected_bodies`; after it, the body visuals, the camera target, the recording body table and the sim screen's overlays. Also the recording sink (`install_recording_sink`) |
| `OnEnter(SimState::Replay)` | The replay bodies and their visuals |
| `First` | `init_orbitals` (while the session is active) |
| `PreUpdate` | `keep_tab_from_egui` |
| `FixedUpdate` | `fixed_physics_step`, which runs `ManualPhysics` |
| `Update` | Keyboard toggles, the time-warp keys, HUD readouts, camera input, gizmos, visuals, the replay player |
| `PostUpdate` | `orbit_camera_track`, `map_orbitals`, the GPU catalog systems |
| `Last` | `collect_log_events`; `SimSet::ApplyCommands` (`apply_sim_commands`), then `apply_recorder_commands`; `handle_ui_events` (before `ApplyCommands`); `orbital_gizmos` |
| `EguiPrimaryContextPass` | Every egui screen, panel and window |
| `OnExit(SimSession::Active)` | `remove_sim_resources`; the recording ends and its body table is removed |

The app orders its own systems against `SimSet::SessionInit`, `SimSet::SpawnBodies` and `SimSet::ApplyCommands`. Entering the sim, `OnEnter(SimSession::Active)` runs before `OnEnter(UiScreen::Sim)`; leaving it, `OnExit(UiScreen::Sim)` runs before `OnExit(SimSession::Active)`.

## External dependencies

Versions from `Cargo.lock`; toolchain from `rust-toolchain.toml` (Rust 1.99.1).

| Crate | Version | Role |
|---|---|---|
| `bevy` | 0.18.0, from the project's fork at rev `f93dee9` | Engine and ECS |
| `avian3d` | 0.6.1 (f64) | Rigid-body physics, joints, collisions |
| `brahe` | 1.3.4 | Orbital mechanics: Kepler and SGP4 propagation, element conversions, epochs |
| `nalgebra` | 0.34.2 | 6-element state vectors (`Vector6`) |
| `sgp4` | 2.3.0 | SGP4 itself, used through `brahe` (not a direct dependency) |
| `bevy_egui` | 0.39.1 | The egui UI (`ssg-app`) |
| `egui_plot` | 0.34.1 | The data collection plots (`ssg-app`) |
| `parquet` | 60.0.0 | The frame files of a recording (`ssg-sim`) |
| `schemars` | 1.2.3 | Test only: generates the published JSON Schema files from the Rust types |
