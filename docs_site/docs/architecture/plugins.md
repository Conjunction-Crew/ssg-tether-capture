---
sidebar_position: 2
---

# Plugins

Plugins bundle related systems and resources into a reusable unit. Each implements Bevy's `Plugin` trait. The simulation plugins are collected in `SimPlugins` (crate `ssg-sim`); the rendering plugins in `RenderPlugins` and `UiPlugin` (crate `ssg-app`), which `run()` adds.

## SimPlugins

**Source:** `crates/ssg-sim/src/lib.rs` (the group); one plugin per feature module under `crates/ssg-sim/src/`.

The headless simulation, as a plugin group. Add it after `StatesPlugin`, because `init_state` needs the `StateTransition` schedule. It contains no render, window or UI type, so it also runs under the `ssg-sim run` command.

| Plugin | Source | Registers |
|---|---|---|
| `SessionPlugin` | `session.rs` | `UiScreen` and `SimState`; session resources on `OnEnter(UiScreen::Sim)` (set `SimSet::SessionInit`) and their removal on `OnExit` |
| `TimePlugin` | `time.rs` | Avian3D `PhysicsPlugins` on `ManualPhysics`, gravity, substeps, the `FixedUpdate` rate, `fixed_physics_step`, and the `SimStep` chain |
| `ProjectPlugin` | `project.rs` | `SelectedProject`, `CapturePlanLibrary`, `SpaceObjectCatalog`, `OrbitalSelectionState` |
| `TelemetryPlugin` | `telemetry.rs` | `LogEvent`, `CaptureLog`, `DataCollection`, `collect_log_events` in `Last` |
| `OrbitPlugin` | `orbit/mod.rs` | `init_orbitals` in `First`; `cache_eci_states` in `SimStep::CacheEci` |
| `AnchorPlugin` | `anchor.rs` | `sync_anchor_from_camera_target` in `SimStep::SyncAnchor` |
| `FloatingOriginPlugin` | `floating_origin.rs` | `calculate_com_rv` (`SimStep::CenterOfMass`), `target_entity_reset_origin` (`SimStep::OriginReset`) |
| `PhysicsBubblePlugin` | `physics_bubble.rs` | `physics_bubble_add_remove` in `SimStep::Bubble` |
| `TetherPlugin` | `tether.rs` | `spawn_selected_bodies` on `OnEnter(SimState::Running)` (set `SimSet::SpawnBodies`) |
| `CapturePlugin` | `capture/mod.rs` | `SimCommand`, `capture_state_machine_update` in `SimStep::Capture`, `apply_sim_commands` in `Last` (set `SimSet::ApplyCommands`) |

`init_orbitals` runs in `First` so that newly spawned `Orbit` components are initialised before any `Update` system runs.

`fixed_physics_step` runs in `FixedUpdate` at `FIXED_HZ` while the session is running, and drives the `ManualPhysics` schedule each tick, scaled by `WorldTime::multiplier`.

The `SimStep` sets run in `ManualPhysics`, chained in this order and all after Avian3D's step (`PhysicsSystems::Last`). This is today's order, named:

| Order | Set |
|---|---|
| 1 | `SimStep::SyncAnchor` |
| 2 | `SimStep::CenterOfMass` |
| 3 | `SimStep::OriginReset` |
| 4 | `SimStep::CacheEci` |
| 5 | `SimStep::Bubble` |
| 6 | `SimStep::Capture` |

The app orders its own systems against `SimSet::SessionInit`, `SimSet::SpawnBodies` and `SimSet::ApplyCommands`.

## RenderPlugins

**Source:** `crates/ssg-app/src/render/mod.rs` (the group)

The rendering side, in `ssg-app`. Add it after `SimPlugins`.

| Plugin | Source |
|---|---|
| `CelestialPlugin` | `render/celestial.rs` |
| `OrbitCameraPlugin` | `render/camera.rs` (below) |
| `AttachPlugin` | `render/attach.rs` — attaches meshes and scenes to the tether and debris bodies that `SimPlugins` spawns |
| `GizmoPlugin` | `render/gizmos.rs` |
| `GpuComputePlugin` | `render/gpu_compute.rs` (below) |

## OrbitCameraPlugin

**Source:** `crates/ssg-app/src/render/camera.rs`

Registers three input systems in `Update` and one tracking system in `PostUpdate`.

| System | Purpose |
|---|---|
| `orbit_camera_input` | Handles mouse pan and scroll zoom |
| `orbit_camera_switch_target` | Switches tracked entity on click |
| `orbit_camera_control_target` | Syncs camera focus to the current target's position |
| `orbit_camera_track` | Runs after `floating_origin_update_visuals` to keep the camera positioned correctly |

`orbit_camera_track` is ordered with `.after(floating_origin_update_visuals)` to ensure the floating origin translation is applied before the camera is repositioned.

## GpuComputePlugin

**Source:** `crates/ssg-app/src/render/gpu_compute.rs`

Manages a GPU compute pipeline for efficiently rendering large orbital debris datasets (thousands of objects) as instanced points on the map view.

Registers:
- `GpuElements`, `GpuComputeUniforms`, `GpuComputeEpochOrigin` resources on the main world
- `GpuEciStateBuffer` — a GPU-side `ShaderStorageBuffer` of ECI state vectors, extracted to the render world each frame
- `MapOrbitPointMaterial` — a custom `Material` that reads from the ECI buffer in its vertex shader to position instanced debris points on the map
- A compute pass (`GpuComputeLabel`) that propagates ECI states on the GPU using `orbital_eci.wgsl`
- `setup_map_orbit_points`, `update_gpu_uniforms`, `sync_map_orbit_point_visibility` in `PostUpdate`
- `reset_gpu_compute_resources` on `OnExit(UiScreen::Sim)`

The compute pipeline takes the initial ECI state buffer and a time offset uniform, and writes updated positions into the buffer read by the map material. This avoids uploading per-object transforms every frame for large catalogs.

## UiPlugin

**Source:** `crates/ssg-app/src/ui/plugin.rs`

Responsible for the entire Bevy-native UI layer. Registers:

- A dedicated `UiCamera` entity on its own render layer (`UI_LAYER = 2`)
- `UiScreen` state (`WorkingDirectorySetup` → `Home` → `Sim` state machine)
- `SelectedProject`, `UiTheme`, `WorkingDirectory`, `NewCapturePlanForm` resources
- `UiEvent` message type
- Screen lifecycle systems: spawn/cleanup/interaction handlers for `WorkingDirectorySetupScreen`, `HomeScreen`, `SimScreen`, and `NewCapturePlanModal`
- Input field systems: `input_field_interaction`, `input_field_keyboard`, `input_field_display`
- Form sync system: `sync_form_fields`
- Polling systems: `poll_file_dialog_task`, `poll_new_plan_modal`, `poll_home_plan_refresh`

State transitions are driven by `UiEvent` messages processed by `handle_ui_events`:

| Event | Transition |
|---|---|
| `WorkingDirectorySelected(path)` | `WorkingDirectorySetup` → `Home` |
| `ChangeWorkingDirectory` | `Home` → `WorkingDirectorySetup` |
| `OpenProject(id)` | `Home` → `Sim` (only if `id` exists in `CapturePlanLibrary`) |
| `BackToHome` | `Sim` → `Home` |

The `poll_home_plan_refresh` system watches the `UserPlansDirty` flag. When set, it rebuilds the home screen plan list in-place without triggering a full screen transition.
