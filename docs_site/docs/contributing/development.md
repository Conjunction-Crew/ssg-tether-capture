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

Releases use [cargo-release](https://github.com/crate-ci/cargo-release) to
automate version bumping and docs snapshotting. **Tags are pushed manually** by
maintainers — this is the only step that triggers the release and docs-deploy
workflows, giving the team full control over when a release goes live.

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

### How to cut a release (maintainer steps)

#### Prerequisites

```bash
cargo install cargo-release   # install once
```

#### 1 — Create a release branch

```bash
git checkout main              # or dev for a beta
git pull
git checkout -b release/v0.2.0
```

#### 2 — Update CHANGELOG.md

Add a new `## [0.2.0]` section above `## [Unreleased]` with the release notes
for this version. CI will fail if the CHANGELOG does not contain an entry
matching the Cargo.toml version.

#### 3 — Run cargo-release (bump + docs snapshot)

```bash
# Replace "minor" with "major", "patch", or an explicit version like "0.2.0".
# --no-confirm skips the interactive prompt; remove it if you prefer to review.
cargo release minor --execute --no-confirm
```

`cargo-release` will:
1. Bump the version in `Cargo.toml`.
2. Run `scripts/verify_release_pr.sh` (pre-release hook) — fails fast if the
   CHANGELOG entry is missing or the version is not greater than the latest tag.
3. Commit the version bump.
4. Run `scripts/post_release.sh` (post-release hook) — snapshots the current
   docs with `npm run docusaurus docs:version <version>` and commits the
   generated files into the release branch.

Because `.release.toml` sets `tag = false` and `push = false`, **no tag is
created and nothing is pushed automatically**.

#### 4 — Open a release PR

```bash
git push origin release/v0.2.0
# Open a PR: release/v0.2.0 → main (or dev for beta)
```

The CI `verify-release-pr` job re-runs `verify_release_pr.sh` so reviewers
can see the checks pass before approving. The regular `build` job also runs the
full matrix build to confirm nothing is broken.

#### 5 — Merge the PR

After review and CI is green, merge the release PR into `main` (or `dev`).

#### 6 — Push the tag (triggers release + docs deploy)

After the merge commit is on `main`, pull and tag it:

```bash
git checkout main
git pull
git tag v0.2.0            # stable release
git push origin v0.2.0
```

For a beta:

```bash
git checkout dev
git pull
git tag v0.2.0-beta.1
git push origin v0.2.0-beta.1
```

Pushing the tag triggers two workflows:
- **`release.yml`** — builds Linux/Windows/macOS artifacts and publishes a
  GitHub Release (pre-release flag set automatically for `-beta.*` tags).
- **`deploy-docs.yml`** — builds and deploys the Docusaurus site to GitHub
  Pages (runs for both stable and beta tags).

### Artifacts produced

| Platform | Format | Notes |
|---|---|---|
| Linux x86_64 | `.tar.gz` — binary + `assets/` | Extract and run |
| Windows x86_64 | `.msi` — installer | Installs to `Program Files\SSG Tether Capture\` |
| macOS ARM64 | `.dmg` — `.app` bundle | Mount DMG, drag to Applications |

> **macOS Gatekeeper**: The `.app` is currently unsigned. Users must right-click → Open on first launch to bypass the unidentified developer warning. Authenticode (Windows) and Apple Developer ID (macOS) signing can be added to the workflow once certificates are provisioned.
>
> **Icons**: `cargo-bundle` and the WiX installer both support custom icons. Add a PNG icon set or `.icns` file (macOS) and `.ico` (Windows) derived from `assets/logo/tether-capture-logo.svg` and reference them in `[package.metadata.bundle]` (Cargo.toml) and `wix/main.wxs` respectively.
