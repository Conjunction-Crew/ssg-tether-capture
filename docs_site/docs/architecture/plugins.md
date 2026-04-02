---
sidebar_position: 2
---

# Plugins

Plugins bundle related systems and resources into a reusable unit. Each implements Bevy's `Plugin` trait and is registered in `create_app()` or `run()`.

## OrbitalMechanicsPlugin

**Source:** `src/plugins/orbital_mechanics.rs`

Registers the `init_orbitals` system in the `PreUpdate` schedule.

`init_orbitals` processes newly-spawned entities that carry an `Orbit` component and converts them into fully-initialised `Orbital` + `TrueParams` components. This runs before `Update` so that propagation systems always see fully-initialised orbitals on the same frame they are spawned.

```rust
pub struct OrbitalMechanicsPlugin;

impl Plugin for OrbitalMechanicsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreUpdate, init_orbitals);
    }
}
```

## OrbitCameraPlugin

**Source:** `src/plugins/orbit_camera.rs`

Registers three input systems in `Update` and one tracking system in `PostUpdate`.

| System | Purpose |
|---|---|
| `orbit_camera_input` | Handles mouse pan and scroll zoom |
| `orbit_camera_switch_target` | Switches tracked entity on click |
| `orbit_camera_control_target` | Syncs camera focus to the current target's position |
| `orbit_camera_track` | Runs after `floating_origin` to keep the camera positioned correctly |

`orbit_camera_track` is ordered with `.after(floating_origin)` to ensure the floating origin translation is applied before the camera is repositioned.

## UiPlugin

**Source:** `src/ui/plugin.rs`

Responsible for the entire Bevy-native UI layer. Registers:

- A dedicated `UiCamera` entity on its own render layer (`UI_LAYER = 2`)
- `UiScreen` state (an enum-based Bevy `States`)
- `ProjectCatalog` and `SelectedProject` resources
- `UiTheme` resource
- `UiEvent` message type
- Screen lifecycle systems: spawn/cleanup/interaction handlers for `HomeScreen` and `SimScreen`

State transitions are driven by `UiEvent` messages — `OpenProject(id)` transitions to `Sim`, `BackToHome` returns to `Home`.
