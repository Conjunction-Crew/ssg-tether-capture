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
3. Register the entity in `OrbitalEntities` if it needs to be reachable from the UI.

## Project structure conventions

- **One plugin per `plugins/` file** — each file declares and implements exactly one `Plugin`.
- **No logic in `mod.rs`** — `mod.rs` files only contain `pub mod` declarations.
- **Resources in `resources/`** — global mutable state goes in a `Resource`, not a `static` or thread-local.
- **Constants in `constants.rs`** — no magic numbers inline in systems or setup code.
