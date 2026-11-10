---
sidebar_position: 1
---

# Components

Components are data attached to entities. They are grouped below by crate, then module. Paths in `ssg_app` are given for orientation: the app's modules are private to its crate.

---

## `ssg-sim`

### `orbit` — orbital state

#### `Orbit`

`ssg_sim::orbit::Orbit` · How a body's orbit starts. Requires `Orbital`. `init_orbitals` builds the body's propagator from it.

| Variant | Meaning |
|---|---|
| `FromElements(Vector6<f64>)` | Osculating Keplerian elements `[a, e, i, Ω, ω, M]` (metres, radians) at the start epoch |
| `FromElementsAt { elements, epoch }` | The same elements, taken at `epoch` |
| `FromGp(GpElements)` | A catalog GP record: the start state comes from SGP4 at the start epoch |

Every body starts at `WorldTime::start_epoch`; from then on it is propagated as a two-body (Kepler) orbit.

#### `Orbital`

`ssg_sim::orbit::Orbital` · The runtime orbital state of a body.

| Field | Type | Meaning |
|---|---|---|
| `object_id` | `String` | Not read today |
| `parent_entity` | `Option<Entity>` | Not read today |
| `propagator` | `Option<TwoBody>` | the two-body propagator (`orbit::kepler`), set by `init_orbitals` |

For a body inside the physics bubble, the propagator describes the bubble frame's origin and the body's ECI state is that origin plus its `Position` and `LinearVelocity`. For a body outside it, the propagator is the body's own orbit. See [Floating Origin](../concepts/floating-origin).

### `anchor`

#### `SimAnchor`

`ssg_sim::anchor::SimAnchor` · Marker on the body the floating origin and the physics bubble are centred on: the tether root, inserted when the tether spawns. The camera target is separate and never moves it.

### `tether` — the spawned bodies

#### `TetherRoot`

`ssg_sim::tether::TetherRoot` · Marker on the first body of the tether (the chaser). It also carries `SimAnchor`.

#### `TetherNode`

`ssg_sim::tether::TetherNode` · On every other tether body (interior nodes and the tail). `root: Entity` is the tether's root. Consecutive bodies are linked by distance joints.

#### `TetherShape`

`ssg_sim::tether::TetherShape` · The visual shape of a tether body, for the renderer: `Sphere { radius }` or `Cylinder { radius, length }` (metres). `attach_tether_visuals` gives each body a mesh from it.

#### `DebrisBody`

`ssg_sim::tether::DebrisBody` · On the capture target. `model_path` is the glTF scene `attach_debris_scene` loads.

### `capture` — the capture controller

#### `CaptureComponent`

`ssg_sim::capture::CaptureComponent` · On the body being captured while a capture runs (at most one). Inserted by `apply_sim_commands` from a `SimCommand::StartCapture`.

| Field | Type | Meaning |
|---|---|---|
| `plan_id` | `String` | The capture plan |
| `current_state` | `String` | The current phase's id |
| `state_enter_time_s` | `f64` | Sim seconds since the start epoch when the phase was entered |
| `state_elapsed_time_s` | `f64` | Seconds in the current phase |
| `sphere_radius_at_entry_m` | `f64` | The capture-sphere radius when the phase was entered |

`capture_state_machine_update` reads and advances it every fixed step.

#### `CaptureForce`

`ssg_sim::capture::CaptureForce` · The controller's state and output on the tether root and the tail while a capture runs; inserted and removed with `CaptureComponent`.

| Field | Type | Meaning |
|---|---|---|
| `applied` | `DVec3` | The force handed to the physics this step, in N (zero for a body the phase does not push); the capture gizmo draws it |
| `ramp_n` | `f64` | The ramped force bound in N: 0 at the capture start and at each phase entry, rising to the phase's `max_force_n` |
| `direction` | `DVec3` | Unit direction of the last non-zero force |
| `inner`, `outer`, `kill` | `bool` | The branch states (inside the inner edge, outside the outer edge, above the speed limit), with the dead band |

---

## `ssg-app`

### `render::camera`

#### `OrbitCamera`

`ssg_app::render::camera::OrbitCamera` · On the one 3D camera. `scene_params` and `map_params` are two `OrbitCameraParams`, one per view.

| `OrbitCameraParams` field | Default |
|---|---|
| `focus` | origin |
| `distance` | 30.0 |
| `yaw`, `pitch` | 0.0 |
| `min_distance`, `max_distance` | 0.5, 10000.0 |
| `sensitivity` | 0.005 (0.002 on Windows) |
| `max_pitch` | 1.55 rad |
| `up` | +Y |

#### `CameraTarget`

`ssg_app::render::camera::CameraTarget` · Marker on the body the camera looks at. Put on the `SimAnchor` body when it spawns (`attach_camera_target`); `Tab` moves it. A viewing marker only: it does not move the floating origin or the bubble.

### `render::celestial`

#### `Earth`

`ssg_app::render::celestial::Earth` · Marker on the scene Earth.

### `render::gpu_compute`

#### `MapOrbitPointMarker`

`ssg_app::render::gpu_compute::MapOrbitPointMarker` · Marker on each catalog point of the map view; its visibility follows the catalog panel's points toggle.

### `ui::overlays` — the bevy_ui overlays of the 3D view

| Component | Purpose |
|---|---|
| `OrbitLabel` | The label node for `entity: Option<Entity>` (the tether), placed over it in the map view |
| `SelectedCatalogOverlay` | The box drawn around the catalog object selected in the panel |
| `SelectedCatalogOverlayLabel` | Its text |
| `SatelliteIndicatorOverlay` | The marker of the capture target; `entity` and `label` |
| `SatelliteIndicatorOverlayLabel` | Its text |

### `ui::replay`

#### `ReplayBody`

`ssg_app::ui::replay::ReplayBody` · On each visual-only body of a replay; `index` is the body's index in the recording's body table.

### `ui::screens::project_detail`

#### `SimScreen`

`ssg_app::ui::screens::project_detail::SimScreen` · Marker on the root node of the sim screen's bevy_ui overlays; despawned when the screen is rebuilt or left.
