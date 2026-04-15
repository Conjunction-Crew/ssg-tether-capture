---
sidebar_position: 4
---

# UI

The UI is implemented entirely with Bevy's native node-based UI system (no external UI crates). Source: `src/ui/`.

---

## UiPlugin

**Source:** `src/ui/plugin.rs`

Registers everything UI-related into the app. See [Plugins — UiPlugin](../architecture/plugins#uiplugin) for a full breakdown.

---

## State machine (`state.rs`)

The UI screen is controlled by a Bevy `States` enum:

```rust
pub enum UiScreen {
    WorkingDirectorySetup, // initial screen on first launch
    Home,
    Sim,
}
```

State transitions happen via `UiEvent`. Each state change triggers:
- **On enter** — the corresponding `spawn_*_screen` system runs.
- **On exit** — the corresponding `cleanup_*_screen` system despawns the screen's root node and all children.

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
    BackToHome,                       // Sim → Home
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
    EditCapturePlan(String),          // opens form pre-filled with existing user plan

    // Unit preference
    SetUnitSystem(UnitSystem),        // Metric or Imperial

    // Simulation controls
    CaptureDebris { entity: Option<Entity>, plan_id: String },
    ToggleMapView,
    ToggleOrigin,
    ChangeTimeWarp { increase: bool },
    CycleCameraTarget,
}
```

All events are sent by interaction systems and consumed by `handle_ui_events` inside `UiPlugin`.

---

## Screens

### Working directory setup screen (`screens/working_directory_setup.rs`)

The first screen shown on launch (`UiScreen::WorkingDirectorySetup`). Prompts the user to provide a working directory path where their capture plan JSON files will be stored.

- The path field updates `WorkingDirectory::pending_path` as the user types.
- "Browse" opens a native OS folder picker (via `rfd::AsyncFileDialog`); the result is polled each frame by `poll_file_dialog_task`.
- "Confirm" sends `UiEvent::WorkingDirectorySelected(path)`, sets `WorkingDirectory::path`, and transitions to `Home`.

Key components: `DirectoryPathText` — marks the label that displays the current pending path.

---

### Home screen (`screens/home.rs`)

Shown on `UiScreen::Home`. Displays:
- A header bar with the app title.
- A **Working Directory** panel showing the current path and a "Change Directory" button.
- A **My Capture Plans** section — one card per plan in `CapturePlanLibrary::user_plans`.
- An **Example Capture Plans** section — one card per plan in `CapturePlanLibrary::example_plans`.
- A "+ New Plan" button that opens the new capture plan modal.

Clicking a plan card sends `UiEvent::OpenProject(plan_id)`, transitioning to `Sim`.

Each **user** plan card also shows a small **"edit"** button. Clicking it sends `UiEvent::EditCapturePlan(plan_id)`, opening the capture plan modal pre-filled with the existing plan data. Example plan cards do not have an edit button.

When `UserPlansDirty` is set (e.g. after saving a new or edited plan), the home screen is rebuilt in-place by `poll_home_plan_refresh` without a full screen transition.

Key components:
- `HomeScreen` — marker for the root node (used for cleanup).
- `HomeProjectButton { project_id }` — marks each clickable plan card.
- `EditCapturePlanButton { plan_id }` — marks the edit button on each user plan card.
- `HomeWorkingDirectoryLabel` — marks the label showing the current working directory path.
- `NewPlanButton` — marks the "+ New Plan" button.
- `ChangeDirectoryButton` — marks the "Change Directory" button.

---

### Project detail screen (`screens/project_detail.rs`)

Shown on `UiScreen::Sim`. Loaded from `SelectedProject::project_id`, which holds the capture plan file stem set when the user clicked a plan card.

The screen looks up the plan in `CapturePlanLibrary` and the tether entity in `OrbitalCache` using the plan's `tether` field.

The right sidebar contains collapsible sections:

| Section | Contents |
|---|---|
| Project Information | Plan name, working directory path, filename, edit button |
| Space Catalog | Search field, show/hide catalog toggle, show/hide GPU debris points toggle, paginated results list; clicking an entry targets it in the camera |
| Time Warp | Decrease / increase `WorldTime::multiplier` buttons |
| Simulation Controls | Map View, Toggle Origin, Cycle Camera Target, Apply/Reset plan buttons, Capture button |
| Simulation HUD | Live telemetry (`CaptureTelemetryReadout`) and capture guidance (`CaptureGuidanceReadout`) readouts |
| Reference | Usage hints |

When a capture plan is edited and saved while the sim is running, a **restart prompt** banner is shown if `SimPlanSyncState::in_sync` is `false`. The user can apply the new plan by triggering a sim reset.

Key components:
- `SimScreen` — marker for the root node (used for cleanup).
- `BackButton` — sends `UiEvent::BackToHome`.
- `CaptureButton { entity, plan_id }` — triggers `UiEvent::CaptureDebris`.
- `CollapsibleToggle / CollapsibleContent` — pair controlling show/hide for each sidebar section.

---

### New / Edit / View capture plan modal (`screens/capture_plan.rs`)

An overlay modal driven by the `NewCapturePlanForm` resource. Shown when `NewCapturePlanForm::open == true`.

The title reads **“New Capture Plan”** when creating, **“Edit Capture Plan”** when `editing_plan_id` is set, and **“View Capture Plan”** when `read_only` is `true` (e.g. for example plans).

The form contains:
- General fields: Plan Name, Tether Name, Tether Type, Tether Length.
- Unit system radio buttons: **m** (metric, default) or **ft** (imperial). Velocity and force values are stored in metric; if imperial is selected, values are converted on save (1 ft = 0.3048 m, 1 lbf = 4.44822 N).
- Per-phase sections (Approach, Terminal, Capture) with max velocity, max force, and shrink rate fields.
- “+ Add Transition” buttons to add distance-based transition conditions to Approach and Terminal phases.
- Save and Cancel buttons in the header bar (hidden in read-only mode).

On save, `validate_form` checks all required fields. If a filename conflict exists and the form is in **create** mode, a confirmation dialog appears; in **edit** mode the file is always overwritten without a dialog. After a successful save, `CapturePlanLibrary::user_plans` is reloaded and the home screen plan list refreshes. If the save occurred while the sim screen was open, `SimPlanSyncState::in_sync` is set to `false` to trigger the restart prompt.

Key components: `NewCapturePlanModal`, `NewPlanSaveButton`, `NewPlanCancelButton`, `AddApproachTransitionButton`, `AddTerminalTransitionButton`, `NewCapturePlanScrollBody`.

---

## Widgets (`widgets/`)

Shared widget primitives used across screens.

### `ScreenRoot`

Marker component applied to the outermost node of every screen. Used by cleanup systems to despawn the full subtree in one query.

### `InputField`

A keyboard-driven text input widget. Fields are focused on click and accept character input. Numeric-only fields reject non-digit characters. A `|` cursor is appended to the displayed text when the field is focused.

```rust
pub struct InputField {
    pub value: String,
    pub focused: bool,
    pub placeholder: String,
    pub is_numeric: bool,
    pub error: Option<String>,
}
```

Three systems manage input fields: `input_field_interaction` (focus on click), `input_field_keyboard` (routes key events to the focused field), `input_field_display` (syncs the displayed text node).

### `TerminalLog`

**Source:** `src/ui/widgets/terminal_log.rs`

A collapsible terminal panel displayed at the bottom of the sim screen. Renders entries from the `CaptureLog` resource in a scrollable, filterable viewport using a monospace font.

**Marker components:**

| Component | Description |
|---|---|
| `TerminalPanel` | Root container node (column layout, full width) |
| `TerminalLogViewport` | Scrollable content area; holds `ScrollPosition` |
| `TerminalLogWrapper` | Inner column that contains the dynamically-spawned row entities |
| `TerminalLogRow` | Marks each log row; carries `entry_index` (position in the filtered list) |
| `TerminalToggleButton` | The open/close chevron button in the header |
| `LogLevelFilterButton` | Per-level toggle button; carries `level: LogLevel` |
| `TerminalClearButton` | Clears `CaptureLog` entries when clicked |

**Spawn function:**

```rust
pub fn spawn_terminal_panel(
    parent: &mut ChildSpawnerCommands,
    font: &Handle<Font>,
    theme: &UiTheme,
)
```

Called at the end of `spawn_project_detail_screen` to append the panel as the last child of the sim screen root.

**Systems:**

| System | Description |
|---|---|
| `terminal_toggle_interaction` | Expands/collapses the viewport and updates the button label |
| `log_level_filter_interaction` | Toggles per-level visibility; forces a display rebuild |
| `terminal_clear_interaction` | Clears `CaptureLog` and resets the rendered row count |
| `terminal_row_selection_interaction` | Handles click and shift-click row selection |
| `terminal_keyboard_input` | Ctrl+A (select all), Ctrl+C (copy selection to clipboard) |
| `sync_terminal_log_display` | Rebuilds row entities when entry count or active filters change; auto-scrolls when `is_user_scrolled` is false |

All terminal systems run inside the Sim-only `.chain().run_if(in_state(UiScreen::Sim))` group registered in `UiPlugin`.

---

## Theme (`theme.rs`)

`UiTheme` is a `Resource` holding the colour palette:

| Property | Description |
|---|---|
| `background` | Main screen background colour |
| `header_background` | Header bar background colour |

Additional colour fields are defined in `UiTheme` for text, buttons, and accents. Query `Res<UiTheme>` in any system that spawns UI nodes.