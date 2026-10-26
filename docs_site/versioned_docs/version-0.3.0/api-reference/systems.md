---
sidebar_position: 3
---

# Systems

Systems are functions that run on matching sets of entities every frame (or on a schedule). Source: `src/systems/`.

---

## Setup systems (`setup.rs`)

These run once on `OnEnter(UiScreen::Sim)`, chained in order.

### `setup_lighting`

Spawns a `DirectionalLight` for the sun and a second for the moon, both visible on `SCENE_LAYER` and `MAP_LAYER`. Sun uses `AMBIENT_DAYLIGHT` illuminance; moon uses `CIVIL_TWILIGHT`.

### `setup_celestial`

Spawns the Earth entity and registers it in `Celestials`. Also spawns a scaled-down `Map_Earth` for the map view. Both use the 8K KTX2 Earth texture.

Registers both with keys `"Earth"` and `"Map_Earth"` in the `Celestials` resource.

### `setup_tether`

Spawns the tether joint chain. Creates `NUM_TETHER_JOINTS` rigid body entities each separated by `DIST_BETWEEN_JOINTS` metres, links them with `TetherNode`, and registers the root in `OrbitalCache::tethers`.

### `setup_entities`

Spawns the atmosphere, skybox, orbit camera, and initial debris/capture target entities. Also configures auto-exposure compensation curves and HDR bloom.

---

## Propagation systems (`propagation.rs`)

### `load_dataset_entities`

**Schedule:** `OnEnter(UiScreen::Sim)` (chained after setup systems)

Loads all JSON orbital datasets from `assets/datasets/`. For each `JsonOrbital` entry, registers the object in `SpaceObjectCatalog` and uploads ECI state data to the GPU buffer used by `GpuComputePlugin`.

### `init_orbitals`

**Schedule:** `First` (via `OrbitalMechanicsPlugin`)

Processes newly-spawned `Orbit` components. Converts TLE or classical elements into a `brahe::KeplerianPropagator` and stores it in the entity's `Orbital` component. Removes the `Orbit` component after initialisation.

### `cache_eci_states`

**Schedule:** `ManualPhysics::First` (via `OrbitalMechanicsPlugin`)

Iterates all entities with an `Orbital` component and calls `propagator.state_eci(epoch)` using the current `WorldTime::epoch`. Writes the resulting `Vector6<f64>` into `OrbitalCache::eci_states` so that downstream physics and algorithm systems can read ECI state without querying the propagator directly.

### `physics_bubble_add_remove`

**Schedule:** `ManualPhysics::Prepare`

Enables or disables the Avian3D `RigidBody` on each orbital entity based on its distance from the tether root. Bodies within `PHYSICS_ENABLE_RADIUS` get an active `RigidBody`; bodies outside `PHYSICS_DISABLE_RADIUS` have it removed. This prevents the physics solver from integrating objects that are too far away to interact with the tether.

### `target_entity_reset_origin`

**Schedule:** `ManualPhysics::Prepare`

Shifts the `CameraTarget` entity back toward the world origin when its position exceeds `MAX_ORIGIN_OFFSET`. Applies the same offset uniformly to all other entity transforms to preserve relative positions (floating origin step for physics).

### `floating_origin_update_visuals`

**Schedule:** `Update`

Syncs Bevy `Transform` positions for all orbital entities from their Avian3D physics `Position` (or from `OrbitalCache::eci_states` for inactive bodies), accounting for the current floating origin offset.

---

## Camera systems (`orbit_camera.rs`)

### `orbit_camera_input`

**Schedule:** `Update`

Reads mouse delta (right-click drag) and scroll wheel input. Updates `OrbitCamera` yaw, pitch, and distance for the active view.

### `orbit_camera_switch_target`

**Schedule:** `Update`

On left-click, raycasts into the scene. If an `Orbital` entity is hit, transfers `CameraTarget` to that entity.

### `orbit_camera_control_target`

**Schedule:** `Update`

Updates `OrbitCamera::focus` to match the current `CameraTarget`'s world position.

### `orbit_camera_track`

**Schedule:** `PostUpdate` (after `floating_origin_update_visuals`)

Computes and applies the final camera `Transform` from `OrbitCamera` yaw/pitch/distance/focus.

---

## User input systems (`user_input.rs`)

### `toggle_map_view`

Toggles the active camera between scene view (`SCENE_LAYER`) and map view (`MAP_LAYER`) on `M` key press.

### `toggle_origin`

Toggles the floating origin debug marker visibility on `O` key press.

### `toggle_capture_gizmos`

Toggles `Settings::capture_gizmos` on the assigned key press, enabling or disabling capture force vector visualisation.

### `change_time_warp`

Adjusts `WorldTime::multiplier` on `,` / `.` key press.

### `map_orbitals`

Repositions map-view entity transforms based on current `OrbitalCache::eci_states`, scaled by `MAP_UNITS_TO_M`.

---

## Physics systems (`physics.rs`)

### `fixed_physics_step`

**Schedule:** `FixedUpdate`

Manually runs the `ManualPhysics` schedule at a fixed frequency (`FIXED_HZ`). This decouples all physics and orbital capture calculations from the frame rate, ensuring deterministic timesteps regardless of rendering load.

---

## Capture algorithm systems (`capture_algorithms.rs`)

### `capture_state_machine_update`

**Schedule:** `ManualPhysics::Last`

Advances the capture state machine for any entity carrying `CaptureComponent`. Each tick it:

1. Looks up the `CompiledCapturePlan` from `CapturePlanLibrary::compiled_plans`.
2. Evaluates all `CompiledCaptureTransition` conditions for the current state (distance and relative velocity bounds against values in `OrbitalCache::eci_states`).
3. On a matching transition, switches `CaptureComponent::current_state` and records `state_enter_time_s`.
4. Applies `max_velocity` and `max_force` constraints from `CompiledCaptureStateParameters` as Avian3D forces on the tether root joint.
5. If the state carries a `shrink_rate`, decrements the tether joint separations each tick.

---

## Gizmo systems (`gizmos.rs`)

### `dev_gizmos`

**Schedule:** `Update`

Draws developer debug gizmos when `Settings::dev_gizmos` is true: floating origin sphere and physics-bubble radius around the camera target.

### `capture_gizmos`

**Schedule:** `Update`

Draws force vector gizmos for the active capture sequence when `Settings::capture_gizmos` is true.

### `orbital_gizmos`

**Schedule:** `Last`

Draws orbital path gizmos for all active `Orbital` entities in the map view layer.

---

## Capture Log systems (`ui/plugin.rs`)

### `collect_log_events`

**Schedule:** `Update`

Collects all `LogEvent` messages emitted during the frame and appends them to `CaptureLog` (evicting the oldest entry when the ring buffer is full). Stamps each entry with the current `WorldTime::epoch` in seconds. Runs unconditionally so events are never dropped regardless of which screen is active.

### `sync_terminal_log_display`

**Schedule:** `Update` (runs only in `UiScreen::Sim`)

Rebuilds the `TerminalLogWrapper` child list when the number of entries in `CaptureLog` changes or when `CaptureLogUiState::active_filters` is modified. Applies level-filter visibility, per-row selection highlighting, and auto-scrolls the viewport to the bottom unless the user has manually scrolled up (`CaptureLogUiState::is_user_scrolled`).
