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

## Controls

| Input | Action |
|---|---|
| Right-click + drag | Pan the orbit camera |
| Scroll wheel | Zoom in / out |
| `M` | Toggle between scene view and orbital map view |
| `O` | Toggle the floating origin debug marker |
| `,` / `.` | Decrease / increase time warp multiplier |
| Click an object | Select it as the camera tracking target |

## Running tests

```bash
cargo test
```

Both unit and integration tests live under `src/tests/`. See [Testing](../contributing/testing) for details.
