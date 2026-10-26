---
sidebar_position: 2
---

# Plugins

Plugins bundle related systems and resources into a reusable unit. Each implements Bevy's `Plugin` trait and is registered in `create_app()` or `run()`.

## OrbitalMechanicsPlugin

**Source:** `src/plugins/orbital_mechanics.rs`

The central simulation plugin. Registers Avian3D physics on a custom `ManualPhysics` schedule (decoupled from the render frame rate), and orchestrates all orbital mechanics and capture systems.

Registers the following on `OnEnter(UiScreen::Sim)`:
- `init_sim_resources` — inserts `Celestials`, `OrbitalCache`, `WorldTime`, and `CaptureSphereRadius`
- `load_dataset_entities` — loads JSON orbital datasets from `assets/datasets/`

Registers the following on `OnExit(UiScreen::Sim)`:
- `remove_sim_resources` — removes the above resources

`init_orbitals` runs in `First` to ensure newly-spawned `Orbit` components are initialised before any `Update` system runs.

`fixed_physics_step` runs in `FixedUpdate` at `FIXED_HZ` and manually drives the `ManualPhysics` schedule each tick, scaled by `WorldTime::multiplier`.

The `ManualPhysics` schedule runs in this order:

| Set | Systems |
|---|---|
| `PhysicsSystems::First` | `cache_eci_states` |
| `PhysicsSystems::Prepare` | `physics_bubble_add_remove`, `target_entity_reset_origin` |
| *(Avian3D default sets)* | Broad-phase, narrow-phase, constraint solving, integration |
| `PhysicsSystems::Last` | `capture_state_machine_update` |

Also registers `dev_gizmos`, `capture_gizmos`, and `floating_origin_update_visuals` in `Update`.

```rust
pub struct OrbitalMechanicsPlugin;

impl Plugin for OrbitalMechanicsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(PhysicsPlugins::new(ManualPhysics))
            .add_systems(OnEnter(UiScreen::Sim), (init_sim_resources, load_dataset_entities).chain())
            .add_systems(OnExit(UiScreen::Sim), remove_sim_resources)
            .add_systems(First, init_orbitals.run_if(in_state(UiScreen::Sim)))
            .add_systems(FixedUpdate, fixed_physics_step.run_if(in_state(UiScreen::Sim)))
            // ... gizmo + visual update systems in Update
            .add_systems(ManualPhysics, (
                cache_eci_states.in_set(PhysicsSystems::First),
                (physics_bubble_add_remove, target_entity_reset_origin).in_set(PhysicsSystems::Prepare),
                capture_state_machine_update.in_set(PhysicsSystems::Last),
            ).run_if(in_state(UiScreen::Sim)));
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
| `orbit_camera_track` | Runs after `floating_origin_update_visuals` to keep the camera positioned correctly |

`orbit_camera_track` is ordered with `.after(floating_origin_update_visuals)` to ensure the floating origin translation is applied before the camera is repositioned.

## GpuComputePlugin

**Source:** `src/plugins/gpu_compute.rs`

Manages a GPU compute pipeline for efficiently rendering large orbital debris datasets (thousands of objects) as instanced points on the map view.

Registers:
- `GpuElements`, `GpuComputeUniforms`, `GpuComputeEpochOrigin` resources on the main world
- `GpuEciStateBuffer` — a GPU-side `ShaderStorageBuffer` of ECI state vectors, extracted to the render world each frame
- `MapOrbitPointMaterial` — a custom `Material` that reads from the ECI buffer in its vertex shader to position instanced debris points on the map
- A compute pass (`GpuComputeLabel`) that propagates ECI states on the GPU using `orbital_eci.wgsl`
- `setup_map_orbit_points`, `update_gpu_uniforms`, `sync_map_orbit_point_visibility` in `PostUpdate`
- `reset_gpu_compute_resources` on `OnExit(UiScreen::Sim)`

The compute pipeline takes the initial ECI state buffer and a time offset uniform, and writes updated positions into the buffer read by the map material. This avoids uploading per-object transforms every frame for large catalogs.

## UiPlugin

**Source:** `src/ui/plugin.rs`

Responsible for the entire Bevy-native UI layer. Registers:

- A dedicated `UiCamera` entity on its own render layer (`UI_LAYER = 2`)
- `UiScreen` state (`WorkingDirectorySetup` → `Home` → `Sim` state machine)
- `SelectedProject`, `UiTheme`, `WorkingDirectory`, `NewCapturePlanForm` resources
- `UiEvent` message type
- Screen lifecycle systems: spawn/cleanup/interaction handlers for `WorkingDirectorySetupScreen`, `HomeScreen`, `SimScreen`, and `NewCapturePlanModal`
- Input field systems: `input_field_interaction`, `input_field_keyboard`, `input_field_display`
- Form sync system: `sync_form_fields`
- Polling systems: `poll_file_dialog_task`, `poll_new_plan_modal`, `poll_home_plan_refresh`

State transitions are driven by `UiEvent` messages processed by `handle_ui_events`:

| Event | Transition |
|---|---|
| `WorkingDirectorySelected(path)` | `WorkingDirectorySetup` → `Home` |
| `ChangeWorkingDirectory` | `Home` → `WorkingDirectorySetup` |
| `OpenProject(id)` | `Home` → `Sim` (only if `id` exists in `CapturePlanLibrary`) |
| `BackToHome` | `Sim` → `Home` |

The `poll_home_plan_refresh` system watches the `UserPlansDirty` flag. When set, it rebuilds the home screen plan list in-place without triggering a full screen transition.
