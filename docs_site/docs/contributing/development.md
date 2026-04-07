---
sidebar_position: 1
---

# Development Setup

## Clone and build

```bash
git clone https://github.com/conjunction-crew/ssg-tether-capture.git
cd ssg-tether-capture
cargo build
```

The first build will take several minutes — Bevy compiles a large dependency tree including shaders.

## Faster iteration builds

Bevy recommends enabling dynamic linking during development for faster incremental compile times. Add the following to your local `.cargo/config.toml` (not committed to the repo):

```toml
[profile.dev]
opt-level = 1

[profile.dev.package."*"]
opt-level = 3
```

And run with the `dynamic_linking` feature (if available in the workspace) or set:

```toml
# .cargo/config.toml
[build]
rustflags = ["-C", "link-arg=-Wl,-rpath,./target/debug/deps"]
```

See [Bevy's fast compile guide](https://bevyengine.org/learn/quick-start/getting-started/setup/#enable-fast-compiles-optional) for the full recommended configuration.

## Adding a new orbital entity

1. In `setup.rs` (or a new setup system), spawn an entity with an `Orbit` variant:

```rust
commands.spawn((
    Orbit::FromTle("1 25544U ...".to_string()),
    // or Orbit::FromElements(elements), Orbit::FromParams(params)
));
```

2. `OrbitalMechanicsPlugin::init_orbitals` will automatically initialise the `Orbital` and `TrueParams` components on the next `PreUpdate`.
3. Register the entity in `OrbitalCache` if it needs to be reachable from the UI.

## Project structure conventions

- **One plugin per `plugins/` file** — each file declares and implements exactly one `Plugin`.
- **No logic in `mod.rs`** — `mod.rs` files only contain `pub mod` declarations.
- **Resources in `resources/`** — global mutable state goes in a `Resource`, not a `static` or thread-local.
- **Constants in `constants.rs`** — no magic numbers inline in systems or setup code.

## Releasing

Releases are tag-driven. The release workflow in `.github/workflows/release.yml` triggers automatically when a version tag is pushed and publishes a GitHub Release with signed archives for all three platforms.

### Version numbering

The project follows [Semantic Versioning](https://semver.org/):

| Component | Meaning |
|-----------|---------|
| **MAJOR** (`x.0.0`) | Breaking UI/API or simulation behaviour changes |
| **MINOR** (`0.x.0`) | New features, backwards compatible |
| **PATCH** (`0.0.x`) | Bug fixes only |

Pre-1.0, `v0.x.y` is expected. Increment MINOR for features and PATCH for fixes.

### Tag conventions

| Tag format | Result |
|---|---|
| `v0.2.0-beta.1` | GitHub **pre-release** — use on `dev` to test a release candidate |
| `v0.2.0` | GitHub **production release** — use on `main` after beta sign-off |

### How to cut a release

1. Bump the version in `Cargo.toml`:
   ```toml
   [package]
   version = "0.2.0"
   ```
2. Commit the version bump and merge to the appropriate branch (`dev` for beta, `main` for production).
3. Tag the commit and push the tag:
   ```bash
   # Beta release from dev
   git tag v0.2.0-beta.1
   git push origin v0.2.0-beta.1

   # Production release from main
   git tag v0.2.0
   git push origin v0.2.0
   ```
4. The release workflow builds and packages the app for Linux, Windows, and macOS, then publishes a GitHub Release with release notes auto-generated from merged PRs.

### Artifacts produced

| Platform | Format | Notes |
|---|---|---|
| Linux x86_64 | `.tar.gz` — binary + `assets/` | Extract and run |
| Windows x86_64 | `.msi` — installer | Installs to `Program Files\SSG Tether Capture\` |
| macOS ARM64 | `.dmg` — `.app` bundle | Mount DMG, drag to Applications |

> **macOS Gatekeeper**: The `.app` is currently unsigned. Users must right-click → Open on first launch to bypass the unidentified developer warning. Authenticode (Windows) and Apple Developer ID (macOS) signing can be added to the workflow once certificates are provisioned.
>
> **Icons**: `cargo-bundle` and the WiX installer both support custom icons. Add a PNG icon set or `.icns` file (macOS) and `.ico` (Windows) derived from `assets/logo/tether-capture-logo.svg` and reference them in `[package.metadata.bundle]` (Cargo.toml) and `wix/main.wxs` respectively.
