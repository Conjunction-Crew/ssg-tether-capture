---
sidebar_position: 4
---

# UI

The app's UI lives in `crates/ssg-app/src/ui/` and is registered by `UiPlugin` ([Plugins → UiPlugin](../architecture/plugins#uiplugin)). Screens and windows are egui (`bevy_egui`), drawn in `EguiPrimaryContextPass`. bevy_ui is used only for the overlays that float in the 3D view. Paths are given for orientation: the app's modules are private to its crate.

---

## Screens and the sim session

`UiScreen` (`ssg_app::ui::screen::UiScreen`) is the app's screen state: `WorkingDirectorySetup` (default, the launch screen), `Home`, `Sim`.

`ScreenPlugin` ties it to the simulation: `UiScreen::Sim` ⇔ `SimSession::Active`. A screen change only sets `NextState<UiScreen>`; `drive_sim_session` (in `StateTransition`) mirrors it into `NextState<SimSession>`, so both change in the same transition, and a re-entry of the sim screen restarts the session. The order is fixed so the screen's systems see the session resources: entering, `OnEnter(SimSession::Active)` runs before `OnEnter(UiScreen::Sim)`; leaving, `OnExit(UiScreen::Sim)` runs before `OnExit(SimSession::Active)`. `seed_capture_sphere_radius` then sets the capture sphere shown before a capture to the open plan's initial radius.

```
WorkingDirectorySetup  ──WorkingDirectorySelected──►  Home
Home                   ──OpenProject (known plan)──►  Sim  (SimState::Setup)
Sim (Setup)            ──StartSim─────────────────►  Sim  (SimState::Running)
Sim                    ──BackToHome───────────────►  Home
Home                   ──ChangeWorkingDirectory────►  WorkingDirectorySetup
Sim                    ──RestartSim───────────────►  Home ──► Sim (Running)
```

`RestartSim` goes through Home so everything is despawned and set up again; `poll_sim_restart` then enters the sim and starts it. Exit replay does the same with `ReturnToSim`.

## Events

`UiEvent` (`ssg_app::ui::events::UiEvent`) is the message every egui screen writes. `handle_ui_events` handles them in `Last`, before `SimSet::ApplyCommands`; `sim_screen_events` handles the sidebar's own events.

| Group | Variants | Effect |
|---|---|---|
| Navigation | `OpenProject(id)`, `StartSim`, `BackToHome`, `ShowExitConfirm`, `CancelExitConfirm` | Open a project (restores its scenario from `project.json`), start the sim, leave it, the exit-confirm modal |
| Working directory | `WorkingDirectorySelected(path)`, `BrowseForWorkingDirectory`, `ChangeWorkingDirectory` | Use a directory, open the folder picker, go back to the setup screen |
| Plan form | `OpenNewCapturePlanForm`, `CloseNewCapturePlanForm`, `EditCapturePlan(id)`, `AddTransition(phase)`, `RemoveTransition { phase, index }`, `SaveCapturePlan`, `ConfirmOverwriteCapturePlan`, `CancelOverwriteCapturePlan` | The capture plan form and its save |
| Sim controls | `CaptureDebris { entity, plan_id }`, `ToggleMapView`, `ToggleCaptureGizmos`, `ChangeTimeWarp { increase }`, `CycleCameraTarget`, `SetUnitSystem(unit)` | `CaptureDebris` becomes a `SimCommand::StartCapture`; the others match the keys `M`, `C`, `,`/`.`, `Tab` |
| Plan sync | `RestartSim`, `ApplyPlanLive`, `DismissRestartPrompt` | After a plan save while the sim runs: restart, apply the plan live, or continue out of sync |
| Orbit selection | `SelectFromCatalog`, `UseCatalogSelection(role)`, `ClearOrbitalSelection(role)`, `CreateCustomOrbit(role)`, `PlaceChaserRelative { preset, offset_m }`, `SetOrbitalElement { role, field, value }`, `ToggleSatelliteIndicator` | Choose the target and the chaser before Start (`role` is `Target` or `Chaser`) |

## Screens and windows

Each screen is a function that draws from a view struct and pushes `UiEvent`s, called from a thin system; the tests in `crates/ssg-app/src/tests/` drive these functions with `egui_kittest`.

| System | Source | Shown |
|---|---|---|
| `working_directory_setup_screen` | `screens/working_directory_setup.rs` | `UiScreen::WorkingDirectorySetup`: a typed path, **Browse**, **Select**, and the last error under the path |
| `home_screen` | `screens/home.rs` | `UiScreen::Home`: the Working Directory panel and the user and example plan cards |
| `capture_plan_form_window` | `screens/capture_plan/` | A modal window over Home or the sim while `NewCapturePlanForm::open`; read-only for example plans |
| `sim_screen_panels` | `screens/project_detail/sidebar.rs` | The header and the sidebar (see [Running the App](../getting-started/running-the-app#the-simulation-screen)); not in a replay |
| `space_catalog_panel` | `screens/project_detail/catalog.rs` | The catalog toggle (map view only) and the panel: search, 48 results per page, paging; not in a replay |
| `sim_screen_modals` | `screens/project_detail/sidebar.rs` | The exit-confirm modal and the restart prompt |
| `egui_terminal_panel` | `egui_terminal.rs` | The capture log as a bottom panel on the sim screen, drawn before the plots so they fit above it |
| `egui_plots` | `egui.rs` | The data collection window: range and speed plots, CSV export |
| `recording_window` | `recording.rs` | `SimState::Running`: start and stop a recording, its settings |
| `recordings_window` | `replay.rs` | `SimState::Setup`: the project's recordings, opened for replay |
| `transport_window` | `replay.rs` | `SimState::Replay`: play/pause, frame steps, jump to start or end, speed, a scrub slider, the map/detail view button, Exit replay |

`apply_egui_theme` runs first: it applies `visuals_from_theme(UiTheme)` (`egui_theme.rs`) once to each new egui context, so every panel and window takes the same colours.

## Overlays (bevy_ui)

`spawn_project_detail_screen` spawns a transparent full-screen node (`SimScreen`, on `UI_LAYER`) holding the overlays of `ui/overlays.rs`: the tether's map label (`OrbitLabel`), the selected-catalog box (`SelectedCatalogOverlay`) and the satellite indicator (`SatelliteIndicatorOverlay`). It runs on entering the sim screen and again when the bodies spawn; `cleanup_project_detail_screen` despawns it. `map_orbitals` (`PostUpdate`), `update_selected_catalog_overlay` and `update_satellite_indicator_overlay` (`Last`) place them each frame.

## Keyboard

On the sim screen, unless egui is taking the keyboard (`egui_wants_keyboard`):

| System | Key | Effect |
|---|---|---|
| `toggle_map_view` | `M` | Scene ⇄ map view ([Render Layers](../architecture/render-layers#the-map-view-toggle)) |
| `toggle_origin` | `O` | `Settings::dev_gizmos`: the origin axes and the physics-bubble sphere |
| `toggle_capture_gizmos` | `C` | `Settings::capture_gizmos` |
| `change_time_warp` | `.` / `,` | One step up or down the warp ladder (`step_time_warp`); down at 1x pauses. Not in a replay |
| `orbit_camera_switch_target` | `Tab` | Next camera target (`OrbitCameraPlugin`) |

`keep_tab_from_egui` (`PreUpdate`) removes `Tab` from egui's input on the sim screen, so `Tab` only cycles the camera target.

## Other systems

| System | Schedule | Purpose |
|---|---|---|
| `load_home_plans` | `OnEnter(UiScreen::Home)` | Loads the example and user plans |
| `poll_home_plan_refresh` | `Last` | On Home, after the plans changed: re-scans the working directory for plan load errors (`CapturePlanLoadErrors`) |
| `poll_file_dialog_task` | `Last` | Finishes the folder picker |
| `clear_working_directory_setup_error` | `OnEnter(UiScreen::WorkingDirectorySetup)` | Clears the last error |
| `init_hud_readouts` | `OnEnter(UiScreen::Sim)`, `OnEnter(SimState::Running)` | Points `HudReadouts` at the session's bodies and plan |
| `update_time_warp_readout`, `update_capture_telemetry`, `update_capture_guidance` | `Update`, not in a replay | Write the sidebar's readouts |
| `restore_catalog_sources` | `OnEnter(UiScreen::Sim)` | Links a target or chaser restored from `project.json` back to its catalog entry (by NORAD id), once the catalog is loaded |
| `refresh_space_catalog_results` | `Last` | Filters the catalog by the search text |
| `adapt_step_cap` | `Update` while running | Adapts `StepCap` to the machine; lowers the requested warp with it |
| `install_recording_sink` | `OnEnter(SimState::Running)` | The project's recording settings and a sink writing `<project>/recordings/` |
| `collect_recorder_events` | `Update` | Keeps the recorder's messages for the Recording window |
| `scan_recordings_system`, `poll_open_recording` | `Update` in `SimState::Setup` | List the project's recordings; finish opening one |
| `spawn_replay_bodies`, `show_replay_in_scene_view` | `OnEnter(SimState::Replay)` | One visual-only `ReplayBody` per recorded body; switch to the scene view |
| `advance_replay`, `show_replay_frame` | `Update` in a replay | Move the player by the wall time and place the bodies |
| `remove_replay_session` | `OnExit(SimState::Replay)` | Removes `ReplaySession` |
| `return_to_sim` | `Update` on Home | Re-enters the sim after Exit replay |
| `poll_sim_restart` | `Last` | Carries out a requested restart |
