---
sidebar_position: 2
---

# Resources

Resources are singleton values stored globally in the Bevy `World`. Unlike components they are not attached to entities. Source: `src/resources/`.

---

## `Celestials`

**Source:** `src/resources/celestials.rs`

Tracks all planetary/celestial body entities by name.

```rust
pub struct Celestials {
    pub planets: HashMap<String, Entity>,
}
```

Populated during `setup_celestial` (Startup). Known keys after startup:

| Key | Description |
|---|---|
| `"Earth"` | 3D scene Earth entity (on `SCENE_LAYER`) |
| `"Map_Earth"` | Scaled-down map view Earth entity (on `MAP_LAYER`) |

Systems that need to find Earth by name query this resource rather than running a component query.

---

## `OrbitalEntities`

**Source:** `src/resources/orbital_entities.rs`

Tracks tether and debris entities by their `object_id` string.

```rust
pub struct OrbitalEntities {
    pub tethers: HashMap<String, Entity>,
    pub debris: HashMap<String, Entity>,
}
```

Populated during `setup_tether` and `setup_entities` (Startup). Used by UI screens to resolve entities from project IDs.

---

## `TimeWarp`

**Source:** `src/resources/time_warp.rs`

Controls simulation time speed. See [Time Warp](../concepts/time-warp) for usage.

```rust
pub struct TimeWarp {
    pub multiplier: f64, // default: 1.0
}
```

---

## UI resources

The following resources are initialised by `UiPlugin` (source: `src/ui/`):

| Resource | Description |
|---|---|
| `ProjectCatalog` | List of `MockProject` instances available in the workspace |
| `SelectedProject` | The currently-selected project's ID (`Option<String>`) |
| `UiTheme` | Colour palette used by all UI screens |
