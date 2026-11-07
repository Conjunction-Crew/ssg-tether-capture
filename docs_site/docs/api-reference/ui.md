---
sidebar_position: 4
---

# UI

:::note
Paths below predate the v0.3.1 workspace split; see [Architecture → Overview](../architecture/overview). Rewritten in P10.
:::

The screens and windows are egui (`bevy_egui`), drawn in `EguiPrimaryContextPass`. bevy_ui remains only for the overlays that float in the 3D view: the tether map label, the selected-catalog box and the satellite indicator (`ui/overlays.rs`). Source: `crates/ssg-app/src/ui/`.

---

## UiPlugin

**Source:** `crates/ssg-app/src/ui/plugin.rs`

Registers everything UI-related into the app. See [Plugins — UiPlugin](../architecture/plugins#uiplugin) for a full breakdown.

---

## State machine (`screen.rs`)

The UI screen is controlled by a Bevy `States` enum owned by the app:

```rust
pub enum UiScreen {
    WorkingDirectorySetup, // the screen shown on launch
    Home,
    Sim,
}
```

State transitions happen via `UiEvent`. `UiScreen::Sim` goes with the simulation's `SimSession::Active`: `ScreenPlugin` sets both in the same transition. Each egui screen runs with `run_if(in_state(...))`, so it has nothing to spawn or clean up.

```
WorkingDirectorySetup  ──WorkingDirectorySelected──►  Home
Home                   ──OpenProject (valid id)─────►  Sim
Sim                    ──BackToHome───────────────►  Home
Home                   ──ChangeWorkingDirectory────►  WorkingDirectorySetup
```

---

## Events (`events.rs`)

```rust
pub enum UiEvent {
    // Navigation
    OpenProject(String),              // Home → Sim (id = capture plan file stem)
    StartSim,
    BackToHome,                       // Sim → Home
    ShowExitConfirm,
    CancelExitConfirm,
    WorkingDirectorySelected(String), // WorkingDirectorySetup → Home
    BrowseForWorkingDirectory,        // opens native OS folder picker (rfd)
    ChangeWorkingDirectory,           // Home → WorkingDirectorySetup

    // Capture plan form
    OpenNewCapturePlanForm,
    CloseNewCapturePlanForm,
    AddApproachTransition,
    RemoveApproachTransition(usize),
    AddTerminalTransition,
    RemoveTerminalTransition(usize),
    SaveCapturePlan,
    ConfirmOverwriteCapturePlan,
    CancelOverwriteCapturePlan,
    EditCapturePlan(String),          // opens the form filled from the plan

    // Display
    SetUnitSystem(UnitSystem),        // Metric or Imperial; display only

    // Simulation controls
    CaptureDebris { entity: Option<Entity>, plan_id: String },
    ToggleMapView,
    ToggleCaptureGizmos,
    ChangeTimeWarp { increase: bool },
    CycleCameraTarget,
    RestartSim,
    ApplyPlanLive,
    DismissRestartPrompt,
    ToggleSatelliteIndicator,

    // Orbital selection
    UseCatalogSelection(OrbitalSelectionRole),
    CreateCustomOrbit(OrbitalSelectionRole),
    SetOrbitalElement { role: OrbitalSelectionRole, field: OrbitalElementField, value: f32 },
}
```

The screen functions push these events. `handle_ui_events` (`ui/events_handler.rs`) and `sim_screen_events` (`screens/project_detail/interactions.rs`) consume them.

---

## Screens

Each screen is a function that draws from a view struct and pushes `UiEvent`s, called from a thin system. The tests in `crates/ssg-app/src/tests/` drive these functions with `egui_kittest`.

| Screen | Source | Shown |
|---|---|---|
| Working directory setup | `screens/working_directory_setup.rs` | `UiScreen::WorkingDirectorySetup`: a typed path, **Browse**, **Select**, and any error under the path |
| Home | `screens/home.rs` | `UiScreen::Home`: the Working Directory panel and the user and example plan cards, in a scroll area |
| Capture plan form | `screens/capture_plan/` | An egui window over any screen while `NewCapturePlanForm::open`; read-only for example plans |
| Sim sidebar and header | `screens/project_detail/sidebar.rs` | `UiScreen::Sim`: see [Running the App](../getting-started/running-the-app#the-simulation-screen) |
| Space Catalog | `screens/project_detail/catalog.rs` | `UiScreen::Sim`: search, results (48 per page), paging |
| Orbital selection | `screens/project_detail/orbital_selection.rs` | Before the start: Target and Chaser cards, six element sliders |
| Exit and restart prompts | `screens/project_detail/modals.rs`, `sidebar.rs` | `egui::Modal` over the sim screen |

The readouts in the sidebar come from the `HudReadouts` resource, which the `hud.rs` systems fill.

---

## Theme (`theme.rs`)

`UiTheme` is a `Resource` holding the colour palette. `visuals_from_theme` (`egui_theme.rs`) maps it to one egui `Visuals`, applied to the primary context, so every panel and window takes the same colours.
