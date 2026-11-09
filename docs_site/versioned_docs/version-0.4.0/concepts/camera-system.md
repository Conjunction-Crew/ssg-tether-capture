---
sidebar_position: 3
---

# Camera System

One 3D camera shows the sim, in either the **scene view** (the bodies at full scale, around the camera target) or the **map view** (the Earth scaled down, with orbits and the catalog). It orbits a target: drag to turn it, scroll to zoom.

## OrbitCamera

The camera entity carries an `OrbitCamera` component with one set of parameters per view:

```rust
pub struct OrbitCamera {
    pub scene_params: OrbitCameraParams,
    pub map_params: OrbitCameraParams,
}
```

`OrbitCameraParams` holds the focus point, distance, yaw, pitch, the distance limits (0.5 to 10000), the input sensitivity, the pitch limit (1.55 rad) and the up vector. Because each view has its own set, switching views keeps each view's zoom and angle.

`orbit_camera_input` (`Update`) changes the active view's parameters: holding the right mouse button and dragging turns the camera (yaw and pitch), the scroll wheel changes the distance. Neither acts while the pointer is over a UI panel.

## CameraTarget

The `CameraTarget` marker is on the body the camera looks at; one body carries it at a time. When the bodies spawn, `attach_camera_target` puts it on the tether root (the `SimAnchor` body). `Tab`, or the sidebar's target button, moves it to the next body (`orbit_camera_switch_target`, `cycle_camera_target`).

`CameraTarget` is a viewing marker only: the floating origin and the physics bubble follow `SimAnchor`, so cycling the target never changes the simulation.

`orbit_camera_track` (`PostUpdate`, before transforms propagate) places the camera each frame:

- **Scene view:** the focus is the target's position; "up" points away from the Earth.
- **Map view:** the focus is the target's ECI position scaled to map units.

In the scene view, `floating_origin_update_visuals` places the Earth and its atmosphere each frame at the target's position minus its ECI position, so the Earth sits where it is relative to the target. The scene uses ECI axes; positions are never rotated.

## Scene and map view

`M`, or the view button in the sidebar, switches the camera between the two render layers (`set_map_view`):

- **Scene view** (`SCENE_LAYER`): the full-scale Earth with its atmosphere, the tether and the debris, the capture and dev gizmos.
- **Map view** (`MAP_LAYER`): the Earth scaled by `MAP_UNITS_TO_M`, the orbit paths, the catalog points and the Space Catalog panel.

The camera starts in the map view; a replay opens in the scene view. See [Render Layers](../architecture/render-layers).

## Overlays

Small bevy_ui overlays follow bodies on screen: the tether's label in the map view (`OrbitLabel`, placed by `map_orbitals`), the box around the catalog object selected in the panel (`SelectedCatalogOverlay`) and the satellite indicator on the capture target (`SatelliteIndicatorOverlay`). See [UI → Overlays](../api-reference/ui#overlays-bevy_ui).
