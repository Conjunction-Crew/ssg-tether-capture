---
sidebar_position: 3
---

# Systems

Systems are functions that run on matching sets of entities every frame (or on a schedule). Source: `src/systems/`.

---

## Setup systems (`setup.rs`)

These run once in the `Startup` schedule, chained in order.

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

### `init_orbitals`

**Schedule:** `PreUpdate` (via `OrbitalMechanicsPlugin`)

Processes newly-spawned `Orbit` components. Converts TLE/COE/params into a fully-populated `Orbital` + `TrueParams`. Removes the `Orbit` component after initialisation.

### `ssg_propagate_keplerian`

**Schedule:** `Update`

Propagates all `Orbital` entities with an active `RigidBody` by `dt * TimeWarp::multiplier`. For each entity, calls `state_eci(epoch)` on its `brahe::KeplerianPropagator` to obtain a `nalgebra::Vector6<f64>` ECI state, then writes the result into `TrueParams` and syncs Avian3D `Position` / `LinearVelocity`.

### `floating_origin`

**Schedule:** `PostUpdate`

Recentres the Bevy world around the `CameraTarget` entity when drift exceeds `MAX_ORIGIN_OFFSET`. Adjusts all entity `Transform` and Avian3D `Position` components uniformly.

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

**Schedule:** `PostUpdate` (after `floating_origin`)

Computes and applies the final camera `Transform` from `OrbitCamera` yaw/pitch/distance/focus.

---

## User input systems (`user_input.rs`)

### `toggle_map_view`

Toggles the active camera between scene view (`SCENE_LAYER`) and map view (`MAP_LAYER`) on `M` key press.

### `toggle_origin`

Toggles the floating origin debug marker visibility on `O` key press.

### `change_time_warp`

Adjusts `TimeWarp::multiplier` on `,` / `.` key press.

### `track_objects`

Updates `TrackObject` UI node positions to follow their tracked entities on screen.

### `map_orbitals`

Repositions map-view entity transforms based on current `TrueParams`, scaled by `MAP_UNITS_TO_M`.

---

## Gizmo systems (`gizmos.rs`)

### `orbital_gizmos`

**Schedule:** `Last`

Draws orbital path gizmos for all active `Orbital` entities in the map view layer.
