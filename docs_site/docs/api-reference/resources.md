---
sidebar_position: 2
---

# Resources

Resources are single values stored in the Bevy `World`, not attached to an entity. They are grouped below by crate, then module. Paths in `ssg_app` are given for orientation: the app's modules are private to its crate.

**Lifetimes.** Most resources live for the whole run (inserted by their plugin). The *session resources* exist only while `SimSession::Active` (the sim screen): `OrbitalCache`, `WorldTime` and `CaptureSphereRadius` are inserted on entering it and removed on leaving it; `RecordingBodies` exists from the body spawn to the end of the session; `Celestials` exists while the app is on `UiScreen::Sim`.

---

## `ssg-sim`

### `time`

#### `WorldTime`

`ssg_sim::time::WorldTime` · The sim clock (session resource). See [Time Warp](../concepts/time-warp).

| Field | Type | Meaning |
|---|---|---|
| `multiplier` | `u32` | The requested warp: fixed steps per `FixedUpdate` run (1, 2, 4, … up to `MAX_TIME_WARP` = 8192) |
| `effective_multiplier` | `u32` | The steps the last run executed (at most `StepCap`); 0 before the first run |
| `epoch` | `Epoch` | The current sim epoch; each fixed step adds `PHYS_DT` |
| `start_epoch` | `Epoch` | The epoch the session started at |

`change_warp` steps the warp ladder; a step down at 1x pauses (`PauseState::Paused`).

#### `SimConfig`

`ssg_sim::time::SimConfig` · The simulation's explicit inputs: `start_epoch`. The app, the CLI or the test harness inserts it before entering the sim; the sim never reads the wall clock. `setup_time` resets `WorldTime` from it.

#### `StepCap`

`ssg_sim::time::StepCap` · The most fixed steps one `FixedUpdate` run may execute; default `MAX_STEPS_PER_UPDATE` (64). It changes how many steps run per update, never which steps. The app adapts it to the machine between updates (`adapt_step_cap`); the CLI keeps the default.

### `project`

#### `SelectedProject`

`ssg_sim::project::SelectedProject` · `project_id: Option<String>`, the plan id of the open project.

#### `WorkingDirectory`

`ssg_sim::project::WorkingDirectory` · `path` (the working directory in use) and `pending_path` (the path being typed on the setup screen). The app registers it (`UiPlugin`); `set_working_directory` saves the path to the config file.

#### `AssetsDir`

`ssg_sim::project::AssetsDir` · The `assets/` directory datasets are read from. The app inserts the directory it resolved at start-up (inside the `.app` bundle on macOS), the CLI and the test harness theirs.

### `orbit`

#### `OrbitalCache`

`ssg_sim::orbit::cache::OrbitalCache` · Session resource.

| Field | Type | Meaning |
|---|---|---|
| `tethers` | `HashMap<String, Vec<Entity>>` | Tether name → its root and its tail (the interior nodes are reached through the joints) |
| `debris` | `HashMap<String, Entity>` | Debris name → entity (the capture target is `"Satellite1"`) |
| `eci_states` | `HashMap<Entity, Vector6<f64>>` | Every body's ECI state at the current epoch, written by `cache_eci_states` each fixed step |
| `com_rv` | `HashMap<Entity, (DVec3, DVec3)>` | Tether root → the tether's centre of mass position and velocity, written by `calculate_com_rv` |

#### `SpaceObjectCatalog`

`ssg_sim::orbit::catalog::SpaceObjectCatalog` · `entries`: one `SpaceCatalogEntry` per object of the datasets in `assets/datasets/`, loaded by `load_dataset_entities` on entering the sim. The Space Catalog panel searches it.

#### `GpuElements`

`ssg_sim::orbit::catalog::GpuElements` · The catalog's elements in the layout the GPU propagates (`GpuOrbitalElements`), filled by `load_dataset_entities`; the app's `GpuComputePlugin` extracts it to the render world.

#### `GpuComputeEpochOrigin`

`ssg_sim::orbit::catalog::GpuComputeEpochOrigin` · The epoch the GPU buffer's elements are taken at (`None` until the catalog is loaded).

#### `OrbitalSelectionState`

`ssg_sim::orbit::selection::OrbitalSelectionState` · The bodies chosen before Start: `target` and `chaser` (each an optional `SelectedOrbitalObject`), and `chaser_relative` when the chaser is placed relative to the target. `spawn_selected_bodies` reads it.

### `capture`

#### `CaptureSphereRadius`

`ssg_sim::capture::CaptureSphereRadius` · Session resource. `radius`, in metres: the capture sphere's current radius. Set to the plan's initial radius at each capture start (25 m by default) and shrunk by the controller.

#### `CapturePlanLibrary`

`ssg_sim::capture::plan::CapturePlanLibrary` · Every loaded capture plan.

| Field | Meaning |
|---|---|
| `plans` | The union of the user and example plans; what the simulation reads |
| `user_plans` | Plans from the working directory |
| `example_plans` | Plans from `assets/example_capture_plans/` |
| `compiled_plans` | Each plan compiled for the controller (`CompiledCapturePlan`) |

`insert_plan` stores a plan and its compiled form. See [Project Files](../concepts/project-files#capture-plans-schema-v1) for the file format.

### `telemetry`

#### `CaptureLog`

`ssg_sim::telemetry::CaptureLog` · The log shown in the terminal panel: a ring buffer of `LogEntry` values (`entries`), at most `max_entries` (256), the oldest dropped first. `collect_log_events` fills it from `LogEvent` messages on every screen. See [Capture Log](../concepts/capture-log).

A `LogEntry` has `timestamp` (sim time `HH:MM:SS`, or `--:--:--` outside a session), `epoch_utc`, `level` (`LogLevel`: `Error`, `Warn`, `Info`, `Debug`), `source` and `message`.

#### `DataCollection`

`ssg_sim::telemetry::DataCollection` · Samples per capture entity: `velocity` and `position` (relative speed and range against time, for the plots and the CSV export) and `ephemeris` (`EphemerisSample`, one per second of a running capture, for the OEM export). `forces` is not written today.

### `recording`

See [Recordings](../concepts/recordings).

#### `SimInputLog`

`ssg_sim::recording::inputs::SimInputLog` · Every sim-affecting input of the session from step 0 (`entries`, each a `RecordedInput`: the step it applies after and a `RecordedCommand`), whether a recording runs or not. Reset on entering the session.

#### `Recorder`

`ssg_sim::recording::recorder::Recorder` · The recorder's `state`: `RecorderState::Idle` or `Recording` with the running recording.

#### `RecordingSettings`

`ssg_sim::recording::recorder::RecordingSettings` · The project's `recording` block: `auto_start_phase`, `auto_stop_on_end` (default true), `stop_after_end_s` (60), `keyframe_interval_steps` (32), `max_duration_s` (1800), `max_size_mb` (100), `max_tether_bodies` (200).

#### `RecordingSink`

`ssg_sim::recording::RecordingSink` · Where frames go: a boxed `FrameSink`, by default the in-memory `MemorySink`. The app installs a sink writing `<project>/recordings/` when the sim starts; the CLI installs its own.

#### `RecordingBodies`

`ssg_sim::recording::RecordingBodies` · The recorded bodies in table order (`entities`, `roles`): the target first, then the tether root, the interior nodes and the tail. Built by `build_recording_bodies` after the spawn.

---

## `ssg-app`

### `render`

| Resource | Module | Purpose |
|---|---|---|
| `Celestials` | `render::celestial` | `planets`: name → entity, `"Earth"` (scene) and `"Map_Earth"` (map); filled by `setup_celestial` |
| `RenderCatalogElements` | `render::gpu_compute` | The render-world copy of `GpuElements`, in the shader's layout |

### `ui` — state of the screens and windows

| Resource | Module | Purpose |
|---|---|---|
| `UiTheme` | `ui::theme` | The colour palette; `visuals_from_theme` turns it into egui visuals |
| `Settings` | `ui::settings` | `dev_gizmos` (`O`), `capture_gizmos` (`C`), `unit_system` (Metric/Imperial, this launch only) |
| `NewCapturePlanForm` | `ui::form` | The plan form: `open`, the `draft` plan it edits, and one field per key it shows |
| `SimPlanSyncState` | `ui::form` | `in_sync` (false after **Continue** on the restart prompt: the sim runs without the saved plan change) and `restart_requested` (set by Reset/Restart Sim) |
| `HudReadouts` | `ui::hud` | The sidebar's texts (time warp, sim time, telemetry, guidance) and the bodies and plan they describe |
| `CaptureLogUiState` | `ui::log_ui` | The terminal panel: open or not, level filters, scroll and row selection |
| `CapturePlanLoadErrors` | `ui::log_ui` | Plan file → its validation errors, for plans that failed to load |
| `SpaceCatalogUiState` | `ui::catalog_state` | The catalog panel: shown or not, catalog points shown, satellite indicator shown, search text, selected row, page |
| `FilteredSpaceCatalogResults` | `ui::catalog_state` | Indices into `SpaceObjectCatalog::entries` that match the search |
| `CsvExportState` | `ui::egui` | The data collection window's CSV export (file name, completed exports, last error) and the plot session |
| `RecordingPanel` | `ui::recording` | The Recording window: the settings being edited, the recorder's last messages, the last Save result |
| `RecordingsList` | `ui::replay` | The Recordings window: the listed recordings and the load in flight |
| `ReplaySession` | `ui::replay` | The recording being played: its `Player`, body roles and shapes, frame epochs, and how its build differs from this one. Exists only in `SimState::Replay` |
| `ReturnToSim` | `ui::replay` | Set by Exit replay: Home re-enters the sim screen on the next frame |
| `ExitConfirmOpen` | `ui::screens::project_detail` | The exit-confirm modal is open |
| `WorkingDirectorySetupError` | `ui::screens::working_directory_setup` | The last failure to use the selected directory |
