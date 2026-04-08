---
sidebar_position: 2
---

# Prerequisites

Before building or running SSG Tether Capture, make sure the following are installed.

## Rust toolchain

Install Rust via [rustup](https://rustup.rs/):

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

The project targets **stable Rust**. Check `Cargo.toml` for the minimum supported version.

## System dependencies

Bevy has a few platform-level dependencies depending on your OS.

### macOS

```bash
xcode-select --install
```

### Linux (Debian/Ubuntu)

```bash
sudo apt-get install -y \
  libasound2-dev \
  libudev-dev \
  libwayland-dev \
  libxkbcommon-dev
```

Full dependency list: [Bevy Linux dependencies](https://github.com/bevyengine/bevy/blob/main/docs/linux_dependencies.md)

### Windows

No additional system dependencies are required. Ensure you have the [Visual Studio C++ build tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) installed.

## Assets

The app loads KTX2-compressed textures and font files at runtime from the `assets/` directory. These are checked into the repository and do not require separate installation:

| Asset | Path |
|---|---|
| Earth texture (8K) | `assets/textures/earth_8192x4096_uastc.ktx2` |
| Star field (8K) | `assets/textures/8k_stars_uastc.ktx2` |
| HDR cubemap | `assets/textures/hdr-cubemap-2048x2048.ktx2` |
| UI font | `assets/fonts/FiraMono-Medium.ttf` |

KTX2/Basis Universal decoding is handled automatically by Bevy's asset pipeline.
