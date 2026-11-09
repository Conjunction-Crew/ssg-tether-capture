---
sidebar_position: 2
---

# Plugins

Each feature module registers its systems, resources and messages through one Bevy `Plugin`. The simulation plugins form the `SimPlugins` group (crate `ssg-sim`); the app adds `RenderPlugins` and `UiPlugin` (crate `ssg-app`). The order of the `ManualPhysics` sets is on the [Overview](./overview#the-fixed-step).

## SimPlugins

**Source:** `crates/ssg-sim/src/lib.rs` (the group), one plugin per module.

The headless simulation. Add it after `StatesPlugin`: `init_state` needs the `StateTransition` schedule. It holds no render, window or UI type, so the `ssg-sim` CLI and the tests run it as it is.

| Plugin | Module | Registers |
|---|---|---|
| `SessionPlugin` | `ssg_sim::session` | The states `SimSession`, `SimState`, `PauseState`. On `OnEnter(SimSession::Active)`, in `SimSet::SessionInit`, chained: `init_sim_resources` (inserts `OrbitalCache`, `WorldTime` at `SimConfig::start_epoch`, `CaptureSphereRadius`; sets `PauseState::Running`), `load_dataset_entities`, `setup_time`. On `OnExit(SimSession::Active)`: `remove_sim_resources` |
| `TimePlugin` | `ssg_sim::time` | Avian3D's `PhysicsPlugins` on the `ManualPhysics` schedule, `Gravity` zero, `SubstepCount` 8, the `FixedUpdate` rate `FIXED_HZ`, the `StepCap` resource. `fixed_physics_step` in `FixedUpdate` (session active, sim running, not paused); `configure_catch_up_window` in `Startup`; the `SimStep` chain in `ManualPhysics`, inside `PhysicsSystems::Last`, while the session is active |
| `ProjectPlugin` | `ssg_sim::project` | The resources `SelectedProject`, `CapturePlanLibrary`, `SpaceObjectCatalog`, `OrbitalSelectionState` |
| `TelemetryPlugin` | `ssg_sim::telemetry` | The `LogEvent` message; `CaptureLog` and `DataCollection`; `collect_log_events` in `Last` |
| `OrbitPlugin` | `ssg_sim::orbit` | Sets brahe's global Earth-orientation provider to zero (offline and deterministic). `init_orbitals` in `First` while the session is active; `cache_eci_states` in `SimStep::CacheEci` |
| `FloatingOriginPlugin` | `ssg_sim::floating_origin` | The `OriginReset` message; `calculate_com_rv` in `SimStep::CenterOfMass`, `target_entity_reset_origin` in `SimStep::OriginReset` |
| `PhysicsBubblePlugin` | `ssg_sim::physics_bubble` | `physics_bubble_add_remove` in `SimStep::Bubble`, `apply_relative_gravity` in `SimStep::RelativeDynamics` |
| `TetherPlugin` | `ssg_sim::tether` | `spawn_selected_bodies` on `OnEnter(SimState::Running)`, in `SimSet::SpawnBodies` |
| `CapturePlugin` | `ssg_sim::capture` | The messages `SimCommand`, `CaptureTransition`, `CaptureEnded`. `capture_state_machine_update` in `SimStep::Capture` (debug builds add `assert_finite_state` after it); `apply_sim_commands` in `Last`, in `SimSet::ApplyCommands` |
| `RecordingPlugin` | `ssg_sim::recording` | The resources `SimInputLog`, `Recorder`, `RecordingSettings`, `RecordingSink`; the messages `RecorderCommand`, `RecorderEvent`. `reset_sim_input_log` and `reset_recorder` on `OnEnter(SimSession::Active)` before `SimSet::SessionInit`; `build_recording_bodies` on `OnEnter(SimState::Running)` after `SimSet::SpawnBodies`; `end_recording_on_exit` then `remove_recording_bodies` on `OnExit(SimSession::Active)`; `record_step` in `SimStep::Record`; `apply_recorder_commands` in `Last` after `SimSet::ApplyCommands` |

`init_orbitals` runs in `First`, so the bodies spawned on entering `SimState::Running` have their propagators before the next fixed step.

## RenderPlugins

**Source:** `crates/ssg-app/src/render/mod.rs` (the group). Add it after `SimPlugins`.

| Plugin | Module | Registers |
|---|---|---|
| `CelestialPlugin` | `render::celestial` | On `OnEnter(UiScreen::Sim)`, chained: `Celestials` inserted, `setup_lighting` (sun and moon lights), `setup_celestial` (the scene Earth and the map Earth), `setup_camera` (the 3D camera). On `OnExit(UiScreen::Sim)`: `Celestials` removed. In `Update` during a replay: `place_replay_earth` |
| `OrbitCameraPlugin` | `render::camera` | `orbit_camera_input` and `orbit_camera_switch_target` in `Update`; `orbit_camera_track` in `PostUpdate` before `TransformSystems::Propagate`; all on the sim screen (below) |
| `AttachPlugin` | `render::attach` | On `OnEnter(SimState::Running)` after `SimSet::SpawnBodies`: `attach_tether_visuals`, `attach_debris_scene`, `attach_camera_target`. On `OnEnter(SimState::Replay)` after `spawn_replay_bodies`: the first two again, for the replay bodies |
| `GizmoPlugin` | `render::gizmos` | `dev_gizmos`, `capture_gizmos`, `floating_origin_update_visuals` in `Update` while the sim runs; `tether_segments` in `Update` on the sim screen; `orbital_gizmos` in `Last` on the sim screen. Gizmo configs: the default group on the scene and map layers, `CaptureGizmoConfigGroup` on the scene layer only. A black `ClearColor` |
| `GpuComputePlugin` | `render::gpu_compute` | The GPU catalog (below) |

## OrbitCameraPlugin

**Source:** `crates/ssg-app/src/render/camera.rs`

The 3D camera is one entity carrying `OrbitCamera` with separate parameters for the scene and the map view. All three systems run only on `UiScreen::Sim`.

| System | Schedule | Purpose |
|---|---|---|
| `orbit_camera_input` | `Update` | Right-button drag changes yaw and pitch, the scroll wheel the distance, of the active view's parameters; both ignored while the pointer is over egui or a bevy_ui node |
| `orbit_camera_switch_target` | `Update` | `Tab` moves `CameraTarget` to the next body with a `RigidBody` and an `Orbital` (`cycle_camera_target`, also used by the UI button); ignored while egui takes the keyboard |
| `orbit_camera_track` | `PostUpdate`, before `TransformSystems::Propagate` | Scene view: the focus is the target's transform, "up" points away from the Earth. Map view: the focus is the target's ECI position in map units. Then places the camera from yaw, pitch and distance |

## GpuComputePlugin

**Source:** `crates/ssg-app/src/render/gpu_compute.rs`

Draws the loaded catalog (thousands of objects) as points on the map view, propagated on the GPU.

- Main world: the resources `GpuElements` and `GpuComputeEpochOrigin` (both defined in `ssg_sim::orbit::catalog`) and the compute uniforms. In `PostUpdate` on the sim screen it records the epoch the catalog's buffer starts from, spawns one point (`MapOrbitPointMarker`, on `MAP_LAYER`) per catalog object, and updates the time offset each frame; a second system shows or hides the points with the catalog panel's points toggle. On `OnExit(UiScreen::Sim)` everything is reset.
- Render world: `RenderCatalogElements` (the extracted copy of `GpuElements`), a compute pipeline running `shaders/orbital_eci.wgsl` that writes each object's ECI state into a storage buffer, and a map material (`shaders/map_eci_points.wgsl`) that positions every point from that buffer. The compute node runs before the camera driver.

No per-object transform is uploaded each frame.

## UiPlugin

**Source:** `crates/ssg-app/src/ui/plugin.rs`; the event handler in `ui/events_handler.rs`.

The app's UI. Screens and windows are egui, drawn in `EguiPrimaryContextPass`; bevy_ui is used only for the overlays that float in the 3D view (`ui/overlays.rs`). It registers:

- `ScreenPlugin` (`ui/screen.rs`): the `UiScreen` state and `drive_sim_session` (see [UI](../api-reference/ui#screens-and-the-sim-session)), and `seed_capture_sphere_radius` on `OnEnter(UiScreen::Sim)`.
- The resources `UiTheme`, `WorkingDirectory`, `NewCapturePlanForm`, `ExitConfirmOpen`, `HudReadouts`, `WorkingDirectorySetupError`, `SimPlanSyncState`, `CapturePlanLoadErrors`, `CaptureLogUiState`, `CsvExportState`, `RecordingPanel`, `RecordingsList`, `ReturnToSim`, `SpaceCatalogUiState`, `FilteredSpaceCatalogResults`, `Settings`, and two internal ones (the file dialog task and the plans-changed flag).
- The `UiEvent` message, handled by `handle_ui_events` in `Last`, before `SimSet::ApplyCommands`, so a capture started from the UI applies in the same frame.
- A 2D camera on `UI_LAYER` for the overlays (`Startup`).
- Keyboard: `toggle_map_view`, `toggle_origin`, `toggle_capture_gizmos`, `change_time_warp` in `Update` on the sim screen, unless egui takes the keyboard (the warp keys also do nothing in a replay); `keep_tab_from_egui` in `PreUpdate`.
- The HUD readouts (`update_time_warp_readout`, `update_capture_telemetry`, `update_capture_guidance`) in `Update`, outside a replay.
- Recording and replay: `install_recording_sink`, `adapt_step_cap`, `collect_recorder_events`, the Recordings list and loader, the replay player (`advance_replay`, `show_replay_frame`), the replay bodies.
- The egui systems: `working_directory_setup_screen`, `home_screen`, `capture_plan_form_window`, `sim_screen_modals`, `egui_terminal_panel`, `sim_screen_panels`, `space_catalog_panel`, `egui_plots`, `recording_window`, `recordings_window`, `transport_window`, after `apply_egui_theme`.
- In `Last`: `poll_file_dialog_task`, `sim_screen_events`, `poll_home_plan_refresh`, `poll_sim_restart`, the catalog search and the overlay updates.

The systems are listed by schedule on the [UI](../api-reference/ui) page.
