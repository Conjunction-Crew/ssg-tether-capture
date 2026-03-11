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
    Home,          // default
    ProjectDetail,
}
```

State transitions happen via `UiEvent`. Each state change triggers:
- **On enter** — the corresponding `spawn_*_screen` system runs.
- **On exit** — the corresponding `cleanup_*_screen` system despawns the screen's root node and all children.

---

## Events (`events.rs`)

```rust
pub enum UiEvent {
    OpenProject(String),  // transition Home → ProjectDetail
    BackToHome,           // transition ProjectDetail → Home
}
```

Events are sent by interaction systems and consumed by `UiPlugin`'s state-change observers.

---

## Screens

### Home screen (`screens/home.rs`)

Shown on `UiScreen::Home`. Displays:
- A header bar with the app title.
- A workspace path and project count summary.
- A list of project buttons, one per `ProjectCatalog` entry.

Clicking a project button sends `UiEvent::OpenProject(id)`, transitioning to `ProjectDetail`.

Key components:
- `HomeScreen` — marker for the root node (used for cleanup).
- `HomeProjectButton { project_id }` — marks each clickable project entry.

### Project detail screen (`screens/project_detail.rs`)

Shown on `UiScreen::ProjectDetail`. Displays details of the `SelectedProject` and lists its associated orbital entities from `OrbitalEntities`.

Includes a back button that sends `UiEvent::BackToHome`.

Also spawns `TrackObject` and `OrbitLabel` components to connect UI labels to orbital entities.

Key components:
- `ProjectDetailScreen` — marker for the root node (used for cleanup).
- `BackButton` — marks the back navigation button.

---

## Widgets (`widgets/`)

Shared widget primitives used across screens. Currently exports:

- `ScreenRoot` — marker component applied to the outermost node of every screen. Used by cleanup systems to despawn the full subtree in one query.

---

## Theme (`theme.rs`)

`UiTheme` is a `Resource` holding the colour palette:

| Property | Description |
|---|---|
| `background` | Main screen background colour |
| `header_background` | Header bar background colour |

Additional colour fields are defined in `UiTheme` for text, buttons, and accents. Query `Res<UiTheme>` in any system that spawns UI nodes.
