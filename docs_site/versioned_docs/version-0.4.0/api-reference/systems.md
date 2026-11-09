---
sidebar_position: 3
---

# Systems

Systems, the messages they exchange, the states they run in and the sets that order them. Grouped by crate, then module. The UI systems are on the [UI](./ui) page; the plugins that register everything are on [Plugins](../architecture/plugins).

---

## States

| State | Path | Values | Driven by |
|---|---|---|---|
| `SimSession` | `ssg_sim::session::SimSession` | `Idle` (default), `Active` | The app mirrors its `UiScreen` into it (`Active` ⇔ the sim screen); the CLI and the tests set it directly. `Active` holds the session resources, the bodies and the clock |
| `SimState` | `ssg_sim::session::SimState` | `Setup` (default), `Running`, `Replay` | `Setup` before Start; `Running` once the bodies spawn; `Replay` while a recording plays (visual-only bodies, no physics step, no sim commands, no recorder) |
| `PauseState` | `ssg_sim::session::PauseState` | `Running` (default), `Paused` | The time-warp ladder: a step down at 1x pauses. Pause is a state, never a 0x warp |

The app's own screen state, `UiScreen`, is on the [UI](./ui#screens-and-the-sim-session) page.

## Schedule and system sets

| Item | Path | Meaning |
|---|---|---|
| `ManualPhysics` | `ssg_sim::time::ManualPhysics` | The schedule of one fixed step: Avian3D's physics, then the `SimStep` chain. Run by `fixed_physics_step` |
| `SimStep` | `ssg_sim::SimStep` | `CenterOfMass`, `OriginReset`, `CacheEci`, `Bubble`, `RelativeDynamics`, `Capture`, `Record`: chained in this order inside `PhysicsSystems::Last` (see [Overview](../architecture/overview#the-fixed-step)) |
| `SimSet` | `ssg_sim::SimSet` | Sets in other schedules the app orders against: `SessionInit` (`OnEnter(SimSession::Active)`: session resources, dataset load, sim clock), `SpawnBodies` (`OnEnter(SimState::Running)`: the tether and the debris), `ApplyCommands` (`Last`: queued `SimCommand`s applied) |

## Messages

| Message | Path | Written by | Read by |
|---|---|---|---|
| `LogEvent` | `ssg_sim::telemetry::LogEvent` | Any system: `level`, `source`, `message` | `collect_log_events` → `CaptureLog` |
| `SimCommand` | `ssg_sim::capture::SimCommand` | The app (`StartCapture { target, plan_id }`, the Capture button), the CLI | `apply_sim_commands` |
| `CaptureTransition` | `ssg_sim::capture::CaptureTransition` | `capture_state_machine_update`, at each phase change: `entity`, `from`, `to`, `epoch` (the sub-step it fired in), `reason` | The recorder; the CLI |
| `CaptureEnded` | `ssg_sim::capture::CaptureEnded` | Once, when a capture enters a phase with `end`: `entity`, `phase`, `outcome`, `epoch`. The controller keeps running in that phase | The recorder (automatic stop) |
| `OriginReset` | `ssg_sim::floating_origin::OriginReset` | `target_entity_reset_origin`: `step`, `com_r`, `com_v` (the shift applied to every enabled body) | The recorder |
| `RecorderCommand` | `ssg_sim::recording::recorder::RecorderCommand` | The app's Recording window, the CLI: `Start { name }`, `Stop` | `apply_recorder_commands` |
| `RecorderEvent` | `ssg_sim::recording::recorder::RecorderEvent` | The recorder: `Started`, `Stopped`, `Warning` (80 % of a cap), `Refused`; each is also a log line | The app's Recording window |
| `UiEvent` | `ssg_app::ui::events::UiEvent` | The egui screens | `handle_ui_events`, `sim_screen_events` (see [UI](./ui#events)) |

---

## `ssg-sim` systems

### `session`

| System | Schedule | Purpose |
|---|---|---|
| `init_sim_resources` | `OnEnter(SimSession::Active)`, `SimSet::SessionInit` (first) | Inserts `OrbitalCache`, `WorldTime` at `SimConfig::start_epoch`, `CaptureSphereRadius` (25 m); unpauses |
| `remove_sim_resources` | `OnExit(SimSession::Active)` | Removes those three resources |

### `time`

| System | Schedule | Purpose |
|---|---|---|
| `setup_time` | `OnEnter(SimSession::Active)`, `SimSet::SessionInit` (last) | Resets the clock to `SimConfig::start_epoch` at 1x and logs the start epoch |
| `fixed_physics_step` | `FixedUpdate` at `FIXED_HZ` (64 Hz), while the session is active, the sim running and not paused | Runs `min(multiplier, StepCap)` fixed steps; each adds `PHYS_DT` to the epoch and runs `ManualPhysics`. Records the count in `effective_multiplier`. No step debt is carried over |
| `configure_catch_up_window` | `Startup` | Lowers Bevy's catch-up window to 3 fixed steps, so one slow frame cannot multiply the work of a run |

### `orbit`

| System | Schedule | Purpose |
|---|---|---|
| `load_dataset_entities` | `OnEnter(SimSession::Active)`, `SimSet::SessionInit` | Loads the GP datasets in `assets/datasets/` into `SpaceObjectCatalog` and `GpuElements` |
| `init_orbitals` | `First`, while the session is active | Builds the propagator of each new `Orbit`: catalog records from SGP4 at the start epoch, elements at their own epoch; then two-body propagation. The first time in a session it records the session-start entry of `SimInputLog` |
| `cache_eci_states` | `ManualPhysics`, `SimStep::CacheEci` | Writes every body's ECI state at the current epoch to `OrbitalCache::eci_states` |

### `floating_origin`

| System | Schedule | Purpose |
|---|---|---|
| `calculate_com_rv` | `ManualPhysics`, `SimStep::CenterOfMass` | Mass-weighted centre of mass position and velocity of each tether's enabled bodies → `OrbitalCache::com_rv` |
| `target_entity_reset_origin` | `ManualPhysics`, `SimStep::OriginReset` | When the anchor tether's centre of mass is farther than `MAX_ORIGIN_OFFSET` (50 m) from the origin: shifts every enabled body's position and velocity by minus that centre, rebuilds their propagators by plus it, and writes `OriginReset` |

See [Floating Origin](../concepts/floating-origin).

### `physics_bubble`

| System | Schedule | Purpose |
|---|---|---|
| `physics_bubble_add_remove` | `ManualPhysics`, `SimStep::Bubble` | Disables a body's physics (`RigidBodyDisabled`) beyond `PHYSICS_DISABLE_RADIUS` (110 m) from the anchor and enables it again within `PHYSICS_ENABLE_RADIUS` (100 m); rebuilds the body's propagator at each change and mirrors disabled bodies' bubble-frame state each step |
| `apply_relative_gravity` | `ManualPhysics`, `SimStep::RelativeDynamics` | Applies the differential two-body acceleration about the bubble frame's origin to every enabled dynamic body |

### `tether`

| System | Schedule | Purpose |
|---|---|---|
| `spawn_selected_bodies` | `OnEnter(SimState::Running)`, `SimSet::SpawnBodies` | Spawns the tether (root at the origin, laid along the plan's spawn orientation) for the chaser and the debris for the target of `OrbitalSelectionState`; registers them in `OrbitalCache` |

### `capture`

| System | Schedule | Purpose |
|---|---|---|
| `apply_sim_commands` | `Last`, `SimSet::ApplyCommands` | Applies queued `SimCommand`s: `StartCapture` puts `CaptureComponent` on the target and `CaptureForce` on the tether root and tail, sets the capture sphere to the plan's initial radius, and appends the input to `SimInputLog`. A refused command changes nothing |
| `capture_state_machine_update` | `ManualPhysics`, `SimStep::Capture` | One controller step per capture: evaluates the current phase's transitions, applies the phase's forces to the tether (ramped up to `max_force_n`), shrinks the capture sphere, records the plot and ephemeris samples, and writes `CaptureTransition`, `CaptureEnded` and log lines |
| `assert_finite_state` | `ManualPhysics`, after `capture_state_machine_update`, debug builds only | Panics on a non-finite position or velocity of an enabled body, naming the entity and the step |

See [Tether System](../concepts/tether-system).

### `telemetry`

| System | Schedule | Purpose |
|---|---|---|
| `collect_log_events` | `Last` | Appends the frame's `LogEvent`s to `CaptureLog`, stamped with the sim time (or none outside a session), on every screen |

### `recording`

| System | Schedule | Purpose |
|---|---|---|
| `reset_sim_input_log`, `reset_recorder` | `OnEnter(SimSession::Active)`, before `SimSet::SessionInit` | A new session starts with an empty input log and an idle recorder |
| `build_recording_bodies` | `OnEnter(SimState::Running)`, after `SimSet::SpawnBodies` | Builds `RecordingBodies`: the target, then the tether from the root along the joints to the tail |
| `record_step` | `ManualPhysics`, `SimStep::Record` | The frame tap: after every fixed step, writes a frame every `keyframe_interval_steps` steps (and at the first and last step), with the step's resets, transitions and end events. Reads only |
| `apply_recorder_commands` | `Last`, after `SimSet::ApplyCommands`, while the session is active | Applies `RecorderCommand`s and the automatic start when a capture's first phase is `auto_start_phase` |
| `end_recording_on_exit`, `remove_recording_bodies` | `OnExit(SimSession::Active)`, chained | Ends a running recording when the session ends, then removes `RecordingBodies` |

A run with a recording is bit-identical to the run without. See [Recordings](../concepts/recordings).

---

## `ssg-app` render systems

| System | Module | Schedule | Purpose |
|---|---|---|---|
| `setup_lighting` | `render::celestial` | `OnEnter(UiScreen::Sim)` | The sun and the moon as directional lights on both views |
| `setup_celestial` | `render::celestial` | `OnEnter(UiScreen::Sim)` | The scene Earth (`Earth`, `SCENE_LAYER`) and the map Earth (`MAP_LAYER`), registered in `Celestials` |
| `setup_camera` | `render::camera` | `OnEnter(UiScreen::Sim)` | The 3D camera with `OrbitCamera`, bloom, auto-exposure, the skybox and the atmosphere |
| `place_replay_earth` | `render::celestial` | `Update` in a replay | Places the Earth and its atmosphere from the player's frame origin (scene view) |
| `orbit_camera_input`, `orbit_camera_switch_target`, `orbit_camera_track` | `render::camera` | `Update`, `Update`, `PostUpdate` | See [Plugins → OrbitCameraPlugin](../architecture/plugins#orbitcameraplugin) |
| `attach_tether_visuals` | `render::attach` | `OnEnter(SimState::Running)` after `SimSet::SpawnBodies`; `OnEnter(SimState::Replay)` | A mesh per new `TetherShape` |
| `attach_debris_scene` | `render::attach` | The same | The glTF scene of each new `DebrisBody` |
| `attach_camera_target` | `render::attach` | `OnEnter(SimState::Running)` after `SimSet::SpawnBodies` | Puts `CameraTarget` on the new `SimAnchor` body unless a body already has it |
| `floating_origin_update_visuals` | `render::floating_origin_visuals` (registered by `GizmoPlugin`) | `Update` while the sim runs | Moves the Earth and the atmosphere so the camera target stays in place in the scene view |
| `dev_gizmos` | `render::gizmos` | `Update` while the sim runs | When `Settings::dev_gizmos` (`O`): the origin axes and the physics-bubble sphere, scene view only |
| `capture_gizmos` | `render::gizmos` | `Update` while the sim runs | When `Settings::capture_gizmos` (`C`): the capture spheres, the applied force and the relative velocity as rays, scene view only |
| `tether_segments` | `render::gizmos` | `Update` on the sim screen | A line between consecutive tether bodies (the rope), scene view, also in a replay |
| `orbital_gizmos` | `render::gizmos` | `Last` on the sim screen | In the map view: the orbits of the bodies, of the catalog object selected in the panel and of the chosen target and chaser |

The GPU catalog systems are described under [Plugins → GpuComputePlugin](../architecture/plugins#gpucomputeplugin).
