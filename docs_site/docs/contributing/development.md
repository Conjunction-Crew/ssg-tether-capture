---
sidebar_position: 1
---

# Development Setup

On a Mac? Start with [macOS Development Setup (Apple Silicon)](./macos-dev-setup.md) to install the toolchain.

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

## Adding a body with an orbit

Bodies are spawned when the simulation starts (`spawn_selected_bodies` in `crates/ssg-sim/src/tether.rs`, on `OnEnter(SimState::Running)`). A body that should follow an orbit carries an `Orbit` (`ssg_sim::orbit::Orbit`):

```rust
commands.spawn((
    Orbit::FromElements(elements),                 // [a, e, i, Ω, ω, M] at the start epoch, metres and radians
    // or Orbit::FromElementsAt { elements, epoch }, or Orbit::FromGp(gp_record)
    RigidBody::Dynamic,
    Transform::from_translation(position),
));
```

1. `Orbit` requires `Orbital`, so the component is added with it.
2. `init_orbitals` (in `First`) builds the body's propagator at the start epoch on the next update.
3. To find the body later by name, register it in `OrbitalCache` (`debris` or `tethers`), as `spawn_debris` and `spawn_tether` do.

Only the target and the tether are recorded (`RecordingBodies`); a new body is not in recordings unless that table is extended. See [Architecture Overview](../architecture/overview) for where each system runs.

## Project structure conventions

- **The simulation is headless.** `ssg-sim` uses no render, window or UI type and never reads the wall clock; the app and the CLI depend on it, never the other way round. A CI check counts render crates in its dependency tree (it must be 0).
- **One plugin per feature module.** Each module of `ssg-sim` registers its systems through its own `Plugin`, collected in `SimPlugins`; the app's rendering plugins are in `RenderPlugins`.
- **Tests live in test folders.** Unit and system tests go under `crates/*/src/tests/`, out-of-crate tests under `crates/*/tests/`. A source file may declare its test module with `#[path]` pointing into `src/tests/`, but the test code never sits in the source file ([Testing](./testing)).
- **Schemas are generated.** The JSON Schemas in `docs_site/static/schemas/` are written from the Rust types; after changing a file type, regenerate them with `SSG_UPDATE_SCHEMAS=1 cargo test -p ssg-sim --locked schema_files`.

## Before opening a pull request

Branch from `main` and open the PR against `main`. CI runs the same checks you can run locally:

```bash
cargo fmt --all --check   # run `cargo fmt` to fix
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
sh scripts/check_docs_drift.sh   # when you change docs or Rust names
```

Also add a bullet under `## [Unreleased]` in `CHANGELOG.md` describing your change (docs-only PRs are exempt). Don't bump the version; releases do that. Windows and macOS only get a quick `cargo check` on PRs; add the `full-ci` label if your change affects platform-specific code or the build setup.

For guidance on cutting a release, see [Release Workflow](./release-workflow.md). For what each CI job does, see [CI/CD Workflows](./cicd-workflows.md).
