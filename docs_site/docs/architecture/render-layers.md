---
sidebar_position: 3
---

# Render Layers

Bevy's `RenderLayers` component decides which cameras see which entities. The app uses three layers, defined in `crates/ssg-app/src/render/layers.rs`.

## Layers and cameras

| Constant | Value | Seen by | What is on it |
|---|---|---|---|
| `SCENE_LAYER` | `0` | The 3D camera in the scene view | The Earth at full scale, the tether bodies' meshes, the debris model, the sun and moon lights, the capture gizmos, the dev gizmos |
| `MAP_LAYER` | `1` | The 3D camera in the map view | The Earth scaled by `MAP_UNITS_TO_M`, the catalog points, the orbit gizmos, the sun and moon lights |
| `UI_LAYER` | `2` | The 2D UI camera | The bevy_ui overlays of the sim screen (the tether's map label, the selected-catalog box, the satellite indicator) |

There is **one 3D camera** (`OrbitCamera`, spawned by `setup_camera` on entering the sim screen). It sees one layer at a time: the scene or the map. It starts on `MAP_LAYER`. A replay switches it to the scene view when it opens.

The 2D UI camera (`order: 1`, no clear) draws the overlays on top of the 3D camera's image. The egui screens and windows are drawn by `bevy_egui`, not through these layers.

The default gizmo group is configured for both `SCENE_LAYER` and `MAP_LAYER`; the systems that draw into it check which view is active. `CaptureGizmoConfigGroup` is on `SCENE_LAYER` only.

## The map view toggle

`M` (or the map/scene button) runs `set_map_view` (`render/camera.rs`) on the 3D camera:

| From | To | Also |
|---|---|---|
| Scene view | `MAP_LAYER` | The atmosphere moves to the origin and its scale becomes `MAP_UNITS_TO_M` metres per unit |
| Map view | `SCENE_LAYER` | The atmosphere's scale becomes 1 metre per unit |

`OrbitCamera` keeps separate parameters for the two views (`scene_params`, `map_params`), so each view keeps its own zoom and angle. Systems that draw for one view only read the camera's `RenderLayers` (for example, `orbital_gizmos` returns in the scene view and `capture_gizmos` in the map view).

## Adding entities to a layer

Attach `RenderLayers` when spawning:

```rust
// The scene view only
RenderLayers::layer(SCENE_LAYER)

// Both views (the lights do this)
RenderLayers::from_layers(&[SCENE_LAYER, MAP_LAYER])

// The map view only
RenderLayers::layer(MAP_LAYER)
```

An entity without a `RenderLayers` component is on layer `0` (the scene view).
