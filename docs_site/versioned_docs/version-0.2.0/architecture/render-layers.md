---
sidebar_position: 3
---

# Render Layers

Bevy's `RenderLayers` component controls which cameras can see which entities. The app uses three non-overlapping layers so that the 3D scene, the orbital map, and the UI are rendered independently.

## Layer assignments

| Constant | Value | Camera | What renders on it |
|---|---|---|---|
| `SCENE_LAYER` | `0` | Main 3D scene camera | Earth mesh, tether/debris entities, sun/moon lights, atmosphere, skybox |
| `MAP_LAYER` | `1` | Map camera | Scaled-down Earth, orbital gizmos, map-view positions of tracked objects |
| `UI_LAYER` | `2` | `UiCamera` (UiPlugin) | All Bevy UI nodes (home screen, project detail, labels) |

Constants are defined in `src/constants.rs`.

## Why separate layers?

- **Post-processing isolation** — bloom and auto-exposure only apply to the scene camera; the UI stays crisp.
- **Independent clear colour** — the map view can use a different background without affecting the 3D scene.
- **Z-fighting prevention** — UI nodes rendered at UI depth do not interfere with 3D mesh depth buffers.

## Map view toggle

The `toggle_map_view` system (registered in `lib.rs`, `Update`) switches the active camera between `SCENE_LAYER` and `MAP_LAYER` when the user presses `M`. Only one camera is active at a time outside the UI layer.

## Adding entities to a layer

Attach `RenderLayers` at spawn time:

```rust
// Visible in scene view only
RenderLayers::layer(SCENE_LAYER)

// Visible in both scene and map views
RenderLayers::from_layers(&[SCENE_LAYER, MAP_LAYER])

// UI layer
RenderLayers::layer(UI_LAYER)
```

Entities without a `RenderLayers` component default to layer `0` (scene only).
