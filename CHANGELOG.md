# Changelog

All notable changes to **ssg-tether-capture** are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Versions adhere to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

When preparing a release:
1. Add a new `## [X.Y.Z]` section below `## [Unreleased]` with your release notes.
2. Move items from `[Unreleased]` into the new section as appropriate.
3. The CI `verify-release-pr` job will fail if the CHANGELOG does not contain
   an entry matching the version in `Cargo.toml`.

---

## [v0.2.1-beta.1]

### Changed
- Refactors capture algorithm systems to emit `LogEvent` messages for significant events (state transitions, capture success/failure, errors) instead of direct `println!` statements.
- Updates docs to include information about the new capture log terminal panel and its features.

### Added
- Capture log system and terminal panel UI for real-time logging of capture events, errors, and debug information.
- Added new `capture_log.md` document detailing the features and usage of the capture log terminal.
- UI buttons to control orbit/zoom controls within the 3D view (in addition to mouse controls)
- Adds `.bat` and `.ps1` scripts for checking and installing prerequisites on Windows (e.g., WiX Toolset for MSI packaging)

### Removed

---

## [0.2.0-beta.6]

### Changed
- Updates `release.yml` to resolve ad-hoc signing issues making the resulting DMG/.app unusable on macOS
- Updates directory resolution strategy to use platform-correct asset paths (e.g., `assets/datasets/`) and support correct resolution when running from a macOS `.app` bundle
- Adds JSON config to store the working directory path and updates the UI to read/write from this config for persistence across app restarts

---

## [0.2.0-beta.5] - 2026-04-13

### Changed
- Updated release workflow and scripts to resolve issues with the Windows version of the app and improved the MSI installer
- Updated orbit controls to support holding Ctrl + mouse drag for orbiting in the 3D view
- Updated `post_release.sh` script to only trigger versioned docs for major and minor releases and limit versioned docs to last 10 releases
- Changes favicon for the docs site and updates the metadata used for link previews

### Added
- Adds `.bat` and `.ps1` scripts for checking and installing prerequisites on Windows (e.g., WiX Toolset for MSI packaging)
- Adds assets for the Windows MSI installer banner and dialog backgrounds
- UI buttons to control orbit/zoom controls within the 3D view (in addition to mouse controls)

### Removed

---


## [0.2.0-beta.1] - 2026-04-10

### Changed
- Adopts `cargo-release` for automated versioning and changelog management.
- Updates `deploy-docs.yml` to build and deploy the docs on pushes to main 
  or on version tag push
- Updates `pr.yaml` to
  - Support checks for `release/v*.*.*` and `release/v*.*.*-beta.*` branches
  - Delegate PR version verification to `scripts/verify_release_pr.sh`
- Update the `contributing` docs section to explain release process.
- Updates `docusaurus` version from 3.9.2 → 3.10.0
  - Updates other NPM packages as required to update docusaurus.
- Updates docs to explain changes to resources, UI structure, plugins, and instructions on how to run the app
- Updates the library to decouple the physics calculations from frame updates, moving to a fixed update frequency (`ManualPhysics` custom schedule driven by `fixed_physics_step` at `FIXED_HZ`)
- Updates the capture algorithms; capture plan states and transitions are now pre-compiled into `CompiledCapturePlan` for efficient O(1) runtime lookups
- Tether properties (length) are now defined by the capture plan (`CapturePlanDevice`) rather than a fixed constant; number of joints is no longer user-configurable
- Example capture plans are now read-only in the UI (view-only mode in the capture plan form)

### Added
- `.release.toml` configuration file for `cargo-release`.
- Adds scripts to manage pre and post release tasks.
  - `scripts/pre_release.sh` and `scripts/post_release.sh` for managing pre and post release tasks.
  - `scripts/post_release.sh` also keeps the docs site version in sync with the crate version.
  - `scripts/verify_release_pr.sh` for CI pull-request version verification.
- Adds `CHANGELOG.md` document to track changes between releases.
- Adds versioned docs for `v0.2.0-beta.1`
- Adds `assets/datasets` and `assets/shaders` directories
- Adds support for loading orbital data from JSON datasets (`assets/datasets/`) using `JsonOrbitalData` / `JsonOrbital`
- Adds GPU compute support for large datasets (`GpuComputePlugin`): instanced debris points on the map view rendered via a GPU compute pipeline without spawning individual ECS entities
- Adds `SpaceObjectCatalog` with search, filter, and pagination UI in the sim sidebar; includes show/hide toggles for the catalog panel and GPU debris points
- Adds UI for capture plan create / edit / view; form supports metric and imperial unit input with automatic conversion on save
- Adds overwrite confirmation dialog when saving a new capture plan with a conflicting filename
- Adds sim restart prompt (`SimPlanSyncState`) when a capture plan is edited and saved while the simulation is running
- Adds capture plan validation (`validate_capture_plan`, `CapturePlanLoadErrors`) with per-file error reporting
- Adds support for specifying a working directory for the app to read/write files
- Adds `resolve_asset_path()` for correct asset resolution when running from a macOS `.app` bundle
- Fixes font asset path referencing for Windows bundle packaging

---

## [0.1.1-beta.1] — 2026-04-08

### Changed
- Updated release workflow to build and package artifacts for Linux, Windows
  (MSI via WiX), and macOS (DMG via cargo-bundle) on tag push.
- Pre-release (beta) tags now create a GitHub pre-release; stable tags create
  the production release marked as latest.

---

## [0.1.0] - 2026-04-03

### Added
- Initial project scaffold: Bevy + Avian3d physics, orbital-mechanics plugin.
- CI: pull-request build matrix (Linux, Windows, macOS) and version-bump check.
- Docusaurus documentation site (`docs_site/`) covering getting started,
  architecture, concepts, API reference, and contributing guides.
- WiX-based Windows MSI installer and macOS DMG packaging.

[Unreleased]: https://github.com/Conjunction-Crew/ssg-tether-capture/compare/v0.2.0-beta.1...HEAD
[0.2.0-beta.1]: https://github.com/Conjunction-Crew/ssg-tether-capture/compare/v0.1.1-beta.1...v0.2.0-beta.1
[0.1.1-beta.1]: https://github.com/Conjunction-Crew/ssg-tether-capture/compare/v0.1.0...v0.1.1-beta.1
[0.1.0]: https://github.com/Conjunction-Crew/ssg-tether-capture/releases/tag/v0.1.0
