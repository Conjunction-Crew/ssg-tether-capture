---
sidebar_position: 3
---

# Camera System

The orbit camera allows the user to inspect orbital objects from any angle in both the 3D scene view and the orbital map view.

## OrbitCamera component

Entities that act as cameras carry an `OrbitCamera` component, which stores separate parameters for the scene view and the map view:

```rust
pub struct OrbitCamera {
    pub scene_params: OrbitCameraParams,
    pub map_params: OrbitCameraParams,
}
```

`OrbitCameraParams` controls focus point, distance, yaw, pitch, zoom limits, input sensitivity, and the up-vector. The two parameter sets allow the scene camera and map camera to maintain independent zoom/orientation state when switching between views.

## CameraTarget

The `CameraTarget` marker component identifies which entity the orbit camera is currently tracking. Only one entity should carry `CameraTarget` at a time.

The `orbit_camera_switch_target` system updates which entity holds `CameraTarget` when the user clicks an orbital object. The `orbit_camera_control_target` system reads the target's current `TrueParams` position and updates the camera focus accordingly.

`orbit_camera_track` then runs in `PostUpdate` (after `floating_origin`) to apply the final camera transform.

## Scene/map view toggle

The `toggle_map_view` system (registered in `lib.rs` `Update`) switches the active render layer when the user presses `M`:

- **Scene view** (`SCENE_LAYER = 0`) — full 3D Earth rendering with atmosphere, bloom, and auto-exposure.
- **Map view** (`MAP_LAYER = 1`) — a top-down schematic view with a scaled-down Earth and orbital gizmos showing object paths.

Each view has its own camera entity; only the active camera has its output enabled.

## TrackObject and OrbitLabel

Two UI-helper components work alongside the camera:

- `TrackObject { entity: Option<Entity> }` — attached to UI elements that should follow a specific entity's screen position.
- `OrbitLabel { entity: Option<Entity> }` — marks a text label that displays information about a tracked orbital entity.

These are processed by the `track_objects` system in `Update`.
