---
sidebar_position: 6
---

# Running the App

## Development build

From the repository root:

```bash
cargo run
```

This builds and runs the desktop app (`ssg-app`, the workspace's default member) and opens its window. The first build takes longer due to shader compilation and asset preprocessing.

## Release build

For better runtime performance:

```bash
cargo run --release
```

## Working directory

Run from the **repository root** (the directory holding `Cargo.toml` and `assets/`): a development build reads `assets/` relative to the current directory. An installed macOS app reads the assets inside its bundle instead.

## First launch

On launch the app shows the **Select Working Directory** screen. Type a folder path or click **Browse** to choose the directory where your capture plan JSON files will be stored, then click **Select** or press Enter to go to the home screen. A folder that cannot be created keeps you on this screen, with the error under the path. The next launch opens here with the same folder filled in. You can change the directory at any time with the **Change Directory** button on the home screen.

## Capture plans

From the home screen:

- Click **+ New Plan** to open the capture plan form and create a plan
- Click any plan card to load it and open the simulation view (Project Detail screen)
- Click the **edit** button on a user plan card to re-open the form pre-filled for editing

Plans are stored as JSON files in your working directory. Example plans (loaded from `assets/example_capture_plans/`) are read-only and cannot be edited.

## The simulation screen

The sidebar on the right holds these sections, top to bottom:

| Section | What it holds |
|---|---|
| Project Information | Working directory, main file, the sim start epoch (UTC), **View / Edit Plan** |
| Simulation Controls | **Start Simulation**, dimmed until both a Target and a Chaser orbit are chosen. Once running: Map View, Satellite Indicator, Capture Gizmos, Cycle Target, **Capture**, **Reset Sim** |
| Orbital Selection | Before the start: the Target and Chaser cards with their six element sliders |
| Time Warp | Once running: the warp actually running, or `Paused` (see [Time Warp](../concepts/time-warp)) |
| Simulation HUD | The units toggle, the sim time as UTC and `T+h:mm:ss`, telemetry and capture guidance |
| Reference | What the telemetry is measured against and what the guidance shows |

**Units.** The **Metric / Imperial** toggle in Simulation HUD is shared with the capture plan form. It converts what is displayed (m→ft, km→mi, m/s→ft/s, N→lbf) in the HUD, the guidance, the slider labels, the plots and the form. Files, exports, logs and recordings stay metric, and the Transport window's values stay metric too. The app starts in Metric on every launch.

## Controls

The keys act on the sim screen, except while you type in a text field.

### Camera

| Input | Action |
|---|---|
| Right-click + drag | Orbit the camera around its target |
| Scroll wheel | Zoom in / out |
| `Tab` (or **Cycle Target (Tab)**) | Move the camera to the next body |

The camera starts on the tether root once the simulation runs. Clicking a body does not select it.

### Other keys

| Key | Action |
|---|---|
| `M` (or **Map View (M)**) | Toggle between the scene view and the orbital map view |
| `C` (or **Toggle Capture Gizmos (C)**) | Show or hide the capture gizmos: the capture sphere, the applied force and the relative velocity |
| `O` | Show or hide the developer gizmos: the origin axes and the physics-bubble sphere (scene view) |
| `,` / `.` | One step down / up the time-warp ladder; down at 1× pauses, up resumes (not in a replay) |

## Capture Log Terminal

A collapsible panel at the bottom of the sim screen shows the log of the simulation and the app. Click the arrow beside **Capture Log** to open it. Entries are colour-coded by level and can be filtered, selected, copied and saved. See [Capture Log](../concepts/capture-log).

## Running tests

```bash
cargo test --workspace --locked
```

See [Testing](../contributing/testing) for where the tests live and the slow tests run on demand.
