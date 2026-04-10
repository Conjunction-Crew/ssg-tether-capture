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

**Source:** `src/resources/orbital_cache.rs`

Tracks tether and debris entities by their `object_id` string, and caches current ECI state vectors.

```rust
pub struct OrbitalCache {
    pub tethers: HashMap<String, Vec<Entity>>,
    pub debris: HashMap<String, Entity>,
    pub eci_states: HashMap<Entity, Vector6<f64>>,
}
```

`tethers` maps each tether ID to the list of all joint entities in the chain. `eci_states` is populated each physics tick by `cache_eci_states` and consumed by the GPU compute pipeline and capture algorithms. Populated during `setup_tether` and `setup_entities`. Used by the project detail screen to resolve the tether entity from a capture plan's `tether` field.

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
    /// Plans loaded from assets/example_capture_plans.
    pub example_plans: HashMap<String, CapturePlan>,
    /// Plans pre-compiled into a structure optimised for runtime algorithms.
    pub compiled_plans: HashMap<String, CompiledCapturePlan>,
}
```

Plan keys are file stems (e.g. `"example_plan"` for `example_plan.json`). Loaded on home screen entry and reloaded whenever the user saves or edits a plan.

Use `insert_plan(plan_id, plan)` to add a plan — this both inserts into `plans` and compiles it into `compiled_plans`.

`CompiledCapturePlan` is a pre-processed representation optimised for the capture state machine. It contains:

| Type | Description |
|---|---|
| `CompiledCapturePlan` | Owns a `Vec<CompiledCaptureState>` and a `state_indices` map for O(1) lookup by ID |
| `CompiledCaptureState` | Holds pre-parsed `parameters` and `transitions` |
| `CompiledCaptureStateParameters` | Typed `max_velocity`, `max_force`, and optional `shrink_rate` (f64) |
| `CompiledCaptureTransition` | Typed `to`, and optional `distance_less_than` / `distance_greater_than` / `relative_velocity_*` bounds |

The free functions `load_plans_from_dir(path)` and `load_plans_from_dir_with_errors(path)` are used by the home screen and save event handler. `load_plans_from_dir_with_errors` returns both the successfully loaded plans and a `HashMap<String, Vec<String>>` of per-file validation errors. `validate_capture_plan(plan_id, plan)` and `build_capture_component(plan_id, plan, physics_time_secs)` are also available as helpers.

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

**Source:** `src/resources/capture_plan_form.rs`

Drives the new/edit/view capture plan modal. The form is visible when `open == true`; the `poll_new_plan_modal` system in `UiPlugin` watches this resource for changes and spawns or despawns the modal UI accordingly.

Key fields:

| Field | Type | Description |
|---|---|---|
| `open` | `bool` | Whether the modal is visible |
| `plan_name` | `String` | Plan name (used to derive the save filename) |
| `tether_name` | `String` | Tether identifier string |
| `tether_type` | `String` | Device type (e.g. `"tether"`) |
| `tether_length` | `String` | Physical tether length in metres (stored as string for the input field) |
| `approach_max_velocity` / `approach_max_force` | `String` | Approach phase parameters |
| `approach_transitions` | `Vec<TransitionForm>` | Approach-state transition conditions |
| `terminal_max_velocity` / `terminal_max_force` / `terminal_shrink_rate` | `String` | Terminal phase parameters |
| `terminal_transitions` | `Vec<TransitionForm>` | Terminal-state transition conditions |
| `capture_max_velocity` / `capture_max_force` / `capture_shrink_rate` | `String` | Capture phase parameters |
| `overwrite_conflict_path` | `Option<String>` | Set when a filename conflict is detected on save (create mode only) |
| `validation_errors` | `Vec<String>` | Populated by `validate_form` before display |
| `unit_system` | `UnitSystem` | `Metric` (default) or `Imperial`; values are converted to metric on save |
| `editing_plan_id` | `Option<String>` | `Some(id)` when editing an existing plan; `None` for a new plan |
| `show_restart_prompt` | `bool` | Set after saving an edited plan in the sim screen to prompt user to reset |
| `read_only` | `bool` | When `true` the form is view-only (e.g. viewing an example plan) |
| `original_json` | `Option<Value>` | Snapshot of the plan JSON taken when the form is opened for editing; used to detect actual changes |

`reset()` returns the form to its default state (including clearing `editing_plan_id` and `read_only`).

`TransitionForm` carries `to`, `distance_kind` (`"less_than"` or `"greater_than"`), and `distance_value`.

---

## `SimPlanSyncState`

**Source:** `src/resources/capture_plan_form.rs`

Tracks whether the currently-loaded simulation plan is in sync with the saved version on disk.

```rust
pub struct SimPlanSyncState {
    pub in_sync: bool,
    pub restart_requested: bool,
}
```

When the user edits and saves a plan while the sim screen is open, `in_sync` is set to `false`. The polling system then shows a restart prompt in the UI. Setting `restart_requested` triggers a sim reset.

---

## `WorldTime`

**Source:** `src/resources/world_time.rs`

Controls simulation time speed and tracks the current simulation epoch. See [Time Warp](../concepts/time-warp) for usage.

```rust
pub struct WorldTime {
    pub multiplier: u32, // default: 1
    pub epoch: Epoch,    // default: Epoch::now()
}
```

The epoch is advanced each physics tick by the `fixed_physics_step` system, scaled by `multiplier`. All propagator queries use `WorldTime::epoch`.

---

## `SpaceObjectCatalog`

**Source:** `src/resources/space_catalog.rs`

Holds the in-memory catalogue of all loaded orbital objects.

```rust
pub struct SpaceObjectCatalog {
    pub entries: Vec<SpaceCatalogEntry>,
}
```

Each `SpaceCatalogEntry` carries:

| Field | Description |
|---|---|
| `gpu_index` | Index into the GPU ECI state buffer for this object |
| `norad_id` | NORAD catalogue number |
| `object_name` | Object name from the dataset |
| `object_id` | International designator |
| `search_blob` | Pre-built lowercase search string for fast filtering |

`display_name()` and `display_label()` are convenience methods that fall back gracefully when name/ID fields are empty.

---

## `SpaceCatalogUiState`

**Source:** `src/resources/space_catalog.rs`

Drives the Space Catalog panel in the sim sidebar.

| Field | Description |
|---|---|
| `show_catalog` | Whether the catalog panel is expanded |
| `show_points` | Whether GPU debris points are visible on the map |
| `search_text` | Current text in the search input |
| `search_focused` | Whether the search field is keyboard-focused |
| `selected_index` | Currently highlighted entry in the filtered results |
| `page` | Current results page |

---

## `FilteredSpaceCatalogResults`

**Source:** `src/resources/space_catalog.rs`

A `Vec<usize>` of indices into `SpaceObjectCatalog::entries` that match the current search text. Updated each frame by the catalog search system.

---

## `CaptureSphereRadius`

**Source:** `src/resources/capture_plans.rs`

The radius (metres) of the virtual capture sphere around the tether root. Objects entering this radius are considered candidates for capture. Initialised to `25.0` m when the sim starts.

---

## `CapturePlanLoadErrors`

**Source:** `src/resources/capture_plans.rs`

Accumulates per-file validation errors for capture plans that failed to parse or failed `validate_capture_plan`. Used to surface load errors in the UI.

```rust
pub struct CapturePlanLoadErrors {
    pub errors: HashMap<String, Vec<String>>,
}
```

---

## `Settings`

**Source:** `src/resources/settings.rs`

Holds debug visualisation toggles.

| Field | Default | Description |
|---|---|---|
| `dev_gizmos` | `false` | Show development debug gizmos (floating origin marker, physics bubble) |
| `capture_gizmos` | `false` | Show capture force vector gizmos |

Toggled at runtime by `toggle_capture_gizmos` (keyboard input) and read by `dev_gizmos` / `capture_gizmos` systems.

---

## UI resources

The following resources are initialised by `UiPlugin` (source: `src/ui/`):

| Resource | Description |
|---|---|
| `SelectedProject` | The currently-selected capture plan's ID (`Option<String>`) |
| `UiTheme` | Colour palette used by all UI screens |
