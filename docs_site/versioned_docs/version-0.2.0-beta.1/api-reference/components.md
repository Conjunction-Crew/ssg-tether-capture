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
    FromElements(OrbitalElements),
    FromParams(TrueParams),
}
```

Once `init_orbitals` processes this component it can be removed. Requires `Orbital` and `TrueParams`.

---

### `Orbital`

The primary runtime state for an orbital body.

| Field | Type | Description |
|---|---|---|
| `object_id` | `String` | Unique identifier string |
| `parent_entity` | `Option<Entity>` | Parent body (e.g. Earth for a satellite) |
| `tle` | `Option<TleData>` | Source TLE data if initialised from TLE |
| `elements` | `Option<OrbitalElements>` | Classical orbital elements |
| `attitude` | `AttitudeState` | Orientation and angular velocity |
| `state` | `PhysicsState` | `ACTIVE` or `INACTIVE` |
| `approach` | `ApproachMetrics` | Real-time approach metrics relative to target |

Requires `TrueParams`.

---

### `TrueParams`

Ground-truth ECI position and velocity. All derived positions are computed from this.

```rust
pub struct TrueParams {
    pub r: [f64; 3],  // position (m), ECI frame
    pub v: [f64; 3],  // velocity (m/s), ECI frame
}
```

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

### Supporting types

| Type | Description |
|---|---|
| `TleData` | Raw TLE strings, epoch, B* drag term, mean motion |
| `AttitudeState` | Quaternion orientation, angular velocity, body axes |
| `BodyAxes` | X/Y/Z body-frame unit vectors |
| `ApproachMetrics` | Range, closing speed, TCA, closest approach distance |
| `PhysicsState` | `ACTIVE` / `INACTIVE` enum |

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

### `TrackObject`

Attaches to a UI node to make it follow a specific entity's screen position.

```rust
pub struct TrackObject {
    pub entity: Option<Entity>,
}
```

### `OrbitLabel`

Marks a UI text node as an orbital information label for a specific entity.

```rust
pub struct OrbitLabel {
    pub entity: Option<Entity>,
}
```
