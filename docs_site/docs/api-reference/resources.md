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

## `OrbitalCache`

**Source:** `src/resources/orbital_entities.rs`

Tracks tether and debris entities by their `object_id` string.

```rust
pub struct OrbitalCache {
    pub tethers: HashMap<String, Entity>,
    pub debris: HashMap<String, Entity>,
}
```

Populated during `setup_tether` and `setup_entities` (Startup). Used by the project detail screen to resolve the tether entity from a capture plan's `tether` field.

---

## `CapturePlanLibrary`

**Source:** `src/resources/capture_plans.rs`

Holds all loaded capture plans, split into user and example sets.

```rust
pub struct CapturePlanLibrary {
    /// Union of user_plans and example_plans — used by simulation systems.
    pub plans: HashMap<String, CapturePlan>,
    /// Plans loaded from the user's working directory.
    pub user_plans: HashMap<String, CapturePlan>,
    /// Plans loaded from assets/capture_plans.
    pub example_plans: HashMap<String, CapturePlan>,
}
```

Plan keys are file stems (e.g. `"example_plan"` for `example_plan.json`). Loaded on home screen entry and reloaded whenever the user saves or edits a plan. The free function `load_plans_from_dir(path)` is used by both the home screen spawn system and the save event handler.

---

## `WorkingDirectory`

**Source:** `src/resources/working_directory.rs`

Tracks the user-selected directory where capture plan JSON files are read from and saved to.

```rust
pub struct WorkingDirectory {
    pub path: String,
    pub pending_path: String,
}
```

`path` is the active directory. `pending_path` is a staging field used while the user is browsing for a new directory on the setup screen.

---

## `NewCapturePlanForm`

**Source:** `src/resources/new_capture_plan_form.rs`

Drives the new/edit capture plan modal. The form is visible when `open == true`; the `poll_new_plan_modal` system in `UiPlugin` watches this resource for changes and spawns or despawns the modal UI accordingly.

Key fields:

| Field | Type | Description |
|---|---|---|
| `open` | `bool` | Whether the modal is visible |
| `plan_name` | `String` | Plan name (used to derive the save filename) |
| `tether_name` | `String` | Tether identifier string |
| `tether_type` | `String` | Device type (e.g. `"tether"`) |
| `num_joints` | `String` | Number of tether joints |
| `approach_transitions` | `Vec<TransitionForm>` | Approach-state transition conditions |
| `terminal_transitions` | `Vec<TransitionForm>` | Terminal-state transition conditions |
| `overwrite_conflict_path` | `Option<String>` | Set when a filename conflict is detected on save |
| `validation_errors` | `Vec<String>` | Populated by `validate_form` before display |
| `unit_system` | `UnitSystem` | `Metric` (default) or `Imperial`; values are converted to metric on save |
| `editing_plan_id` | `Option<String>` | `Some(id)` when editing an existing plan; `None` for a new plan |

`reset()` returns the form to its default state (including clearing `editing_plan_id`).

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
| `SelectedProject` | The currently-selected capture plan's ID (`Option<String>`) |
| `UiTheme` | Colour palette used by all UI screens |
