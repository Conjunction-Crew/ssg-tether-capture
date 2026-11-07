---
sidebar_position: 6
---

# Running the App

## Development build

From the repository root:

```bash
cargo run
```

Bevy will compile the project and open the application window. The first build takes longer due to shader compilation and asset preprocessing.

## Release build

For better runtime performance:

```bash
cargo run --release
```

## Working directory

The app resolves asset paths relative to the working directory. Always run from the **repository root** (i.e., the directory containing `Cargo.toml` and `assets/`), not from within `src/`.

## First launch

On launch the app shows the **Select Working Directory** screen. Type a folder path or click **Browse** to choose the directory where your capture plan JSON files will be stored, then click **Select** or press Enter to go to the home screen. A folder that cannot be created keeps you on this screen, with the error under the path. The next launch opens here with the same folder filled in. You can change the directory at any time with the **Change Directory** button on the home screen.

## Capture plans

From the home screen:

- Click **+ New Plan** to open the capture plan form and create a plan
- Click any plan card to load it and open the simulation view (Project Detail screen)
- Click the **edit** button on a user plan card to re-open the form pre-filled for editing

Plans are stored as JSON files in your working directory. Example plans (loaded from `assets/capture_plans/`) are read-only and cannot be edited.

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

### Camera navigation

| Input | Action |
|---|---|
| Right-click + drag | Orbit the camera |
| `Ctrl` + left-click + drag | Orbit the camera (trackpad-friendly alternative) |
| Scroll wheel | Zoom in / out |
| `Tab` | Cycle camera tracking target |

### On-screen orbit controls widget

A small control pad is displayed in the bottom-left corner of the 3D view. It
provides an alternative to right-click drag that works well on trackpads, touch
screens, or any pointer device.

| Control | Action |
|---|---|
| ← ↑ ↓ → arrows | Orbit the camera left, up, down, right |
| `+` / `−` | Zoom in / out |
| o | Reset view to default position |
| Left-click + drag inside the widget box | Orbit the camera (same as right-click drag) |

**Hold-to-accelerate:** Holding any direction or zoom button for more than 5
seconds ramps the movement speed up to 2.5× normal, making large viewpoint
changes faster without sacrificing precision on short taps.

### Other controls

| Input | Action |
|---|---|
| `M` | Toggle between scene view and orbital map view |
| `O` | Toggle the floating origin debug marker |
| `,` / `.` | Decrease / increase time warp multiplier |
| Click an object | Select it as the camera tracking target |

## Capture Log Terminal

A collapsible panel at the bottom of the sim screen displays real-time structured log output from the capture state machine, propagation system, and simulation events. Click **^** to expand it. Entries are colour-coded by severity and can be filtered, selected, and copied to the clipboard. See [Capture Log](../concepts/capture-log) for full details.

## Running tests

```bash
cargo test
```

Both unit and integration tests live under `src/tests/`. See [Testing](../contributing/testing) for details.
