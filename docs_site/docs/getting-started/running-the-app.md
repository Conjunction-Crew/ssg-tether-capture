---
sidebar_position: 3
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

On first launch the app shows the **Working Directory Setup** screen. Enter a folder path or click **Browse** to choose a directory where your capture plan JSON files will be stored. Press **Confirm** to proceed to the home screen. You can change this directory at any time via the **Change Directory** button on the home screen.

## Capture plans

From the home screen:

- Click **+ New Plan** to open the new capture plan modal and create a plan
- Click any plan card to load it and open the simulation view (Project Detail screen)
- Click the **edit** button on a user plan card to re-open the form pre-filled for editing

Plans are stored as JSON files in your working directory. Example plans (loaded from `assets/capture_plans/`) are read-only and cannot be edited.

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
