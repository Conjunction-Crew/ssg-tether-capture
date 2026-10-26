---
sidebar_position: 1
---

# Components

Components are data attached to ECS entities. Source: `src/components/`.

---

## Orbital components (`orbit.rs`)

### `Orbit`

Init-only component. Controls how an entity's orbital state is initialised.

```rust
pub enum Orbit {
    FromTle(String),
    FromElements(Vector6<f64>),
}
```

`FromTle` accepts a raw Two-Line Element set string. `FromElements` accepts a `nalgebra::Vector6<f64>` of classical orbital elements `[a, e, i, Ω, ω, M]` in SI units and radians.

Once `init_orbitals` processes this component it can be removed. The `#[require(Orbital)]` attribute ensures an `Orbital` is always present on the same entity.

---

### `Orbital`

The primary runtime state for an orbital body.

| Field | Type | Description |
|---|---|---|
| `object_id` | `String` | Unique identifier string |
| `parent_entity` | `Option<Entity>` | Parent body (e.g. Earth for a satellite) |
| `propagator` | `Option<KeplerianPropagator>` | Brahe propagator, populated by `init_orbitals` |

ECI position and velocity are computed by calling `propagator.state_eci(epoch)`, which returns a `nalgebra::Vector6<f64>`. The result is cached in `OrbitalCache::eci_states` each physics tick by `cache_eci_states`.

---

### `TetherNode`

Marks an entity as a joint in a tether chain and holds a reference to the root joint.

```rust
pub struct TetherNode {
    pub root: Entity,
}
```

---

### `Earth`

Zero-data marker component used to query the Earth entity.

---

### `JsonOrbital` / `JsonOrbitalData`

Deserialization types for orbital data loaded from JSON files in `assets/datasets/`. These map the space-data JSON format (GP element sets) to Rust structs.

`JsonOrbital` holds optional fields matching GP element-set keys (`OBJECT_NAME`, `OBJECT_ID`, `EPOCH`, `MEAN_MOTION`, `ECCENTRICITY`, `INCLINATION`, `RA_OF_ASC_NODE`, `ARG_OF_PERICENTER`, `MEAN_ANOMALY`, `NORAD_CAT_ID`, `BSTAR`, etc.).

`JsonOrbitalData` wraps a `Vec<JsonOrbital>` with an optional dataset `name`. It accepts both a bare JSON array and an object with `"name"` and `"data"` keys.

Loaded by `load_dataset_entities` on `OnEnter(UiScreen::Sim)` and used to spawn GPU-rendered debris points via `GpuComputePlugin`.

---

## Capture components (`capture_components.rs`)

### `CaptureComponent`

Attached to the tether entity when a capture sequence is in progress. There should only ever be 0 or 1 entity with this component at a time.

```rust
pub struct CaptureComponent {
    pub plan_id: String,
    pub current_state: String,
    pub state_enter_time_s: f64,
    pub state_elapsed_time_s: f64,
}
```

The `capture_state_machine_update` system reads this component each physics tick to decide which forces to apply to the tether, and advances `current_state` when transition conditions are met.

### `CapturePlan`

Deserialised structure of a capture plan JSON file.

```rust
pub struct CapturePlan {
    pub name: String,
    pub tether: String,
    pub states: Vec<State>,
    pub device: Option<CapturePlanDevice>,
}
```

`CapturePlanDevice` carries `device_type` and `tether_length` (metres). `State` carries `id`, optional `next`, `parameters` (JSON object), and `transitions` (JSON array). Pre-compiled into a `CompiledCapturePlan` by `CapturePlanLibrary::insert_plan` for fast runtime access.

---

## Camera components (`orbit_camera.rs`)

### `OrbitCamera`

Controls camera behaviour; holds separate param sets for scene and map views.

```rust
pub struct OrbitCamera {
    pub scene_params: OrbitCameraParams,
    pub map_params: OrbitCameraParams,
}
```

`OrbitCameraParams` fields: `focus`, `distance`, `yaw`, `pitch`, `min_distance`, `max_distance`, `sensitivity`, `max_pitch`, `up`.

---

### `CameraTarget`

Zero-data marker. The entity bearing this component is the camera's current tracking target.

---

## UI components (`user_interface.rs`)

### `TimeWarpReadout`

Marker component on the UI text node that displays the current `WorldTime::multiplier`.

### `CaptureTelemetryReadout`

Drives the live telemetry text node in the sim sidebar. Holds the `target_entity` and `reference_entity` being tracked and a `target_label` string.

### `CaptureGuidanceReadout`

Similar to `CaptureTelemetryReadout` but drives the capture guidance panel. Also carries `plan_id` so it can look up the active capture plan.

### `OrbitLabel`

Marks a UI text node as an orbital information label for a specific entity.

```rust
pub struct OrbitLabel {
    pub entity: Option<Entity>,
}
```
