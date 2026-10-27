# Changelog

All notable changes to **ssg-tether-capture** are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Versions adhere to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

**Contributors:** add a bullet for your change under `## [Unreleased]` in every
PR, grouped under `### Added`, `### Changed`, `### Fixed` or `### Removed`. The CI
`Changelog entry` check fails PRs that do not change that section (docs-only PRs
and PRs labelled `no-changelog` are exempt). Do not bump the version or add
version headings by hand.

**Releases:** maintainers run the **Release** workflow in GitHub Actions. It
renames `## [Unreleased]` to `## [vX.Y.Z] - date`, leaves a fresh empty
`## [Unreleased]` above it, bumps `Cargo.toml`, and uses that section as the
GitHub Release description.

---

## [Unreleased]

### Changed
- Time warp: explicit pause state, ladder tops out at 8192×, work per update is capped and the HUD shows the effective warp (F-12)

### Fixed
- Simulation start epoch is an explicit input (`SimConfig`); the simulation core no longer reads the wall clock; the physics clock advances once per step (F-06, F-38)


## [v0.3.1] - 2026-10-27

### Changed
- Cargo workspace: the desktop app moved to `crates/ssg-app` (binary name and packaging unchanged), new `crates/ssg-sim` and `crates/ssg-sim-cli` placeholders, the Bevy fork pinned to commit `f93dee9` and documented in `FORK.md` (no app behaviour change)
- Simulation core separated from rendering and UI inside the app crate: one plugin per feature, explicit physics system sets (same order), a sim-side anchor marker mirrored from the camera target, capture start as a sim command, one implementation per keyboard/button action, meshes attached to simulation bodies by the renderer (no behaviour change; the Tab and map-view keys now log like their buttons)
- `ssg-sim`: the simulation core is now a headless library crate (no render, window or UI dependencies) with the P2 headless harness behind the `test-support` feature (no behaviour change)
- UI screen modules split into folders; architecture docs point at the workspace layout (no behaviour change)

### Added
- `ssg-sim` headless runner (`crates/ssg-sim-cli`): `ssg-sim run <project-dir> …` runs a capture scenario without a window and writes `summary.json` and `trajectory.csv`
- Headless test harness (`src/tests/test_support.rs`): fixed sim epoch, ticks driven through `FixedUpdate` at exactly 1/64 s, and helpers for capture and orbit tests; three integration tests that asserted nothing now assert real behaviour (no app behaviour change)
- Characterization tests pinning current orbit propagation, floating-origin, physics-bubble and time-warp behaviour, including known defects marked for P4 (no app behaviour change)
- Characterization tests pinning current capture-plan transition semantics (conditions OR-ed, transitions cascading within one step) and plan load/compile round trips (no app behaviour change)
- Headless Hubble (HST) capture baseline: fixed-epoch regression guard in CI, plus an opt-in to-capture measurement whose run-to-run spread is P3's tolerance (no app behaviour change)


## [v0.3.0] - 2026-10-26

### Added
- `ci.yml` pull-request workflow: Linux `cargo fmt --check`, `cargo clippy` and `cargo test` as the merge gate, a cheaper `cargo check` on Windows and macOS (only when Rust, `Cargo.*`, `wix/` or `assets/` change), a docs build when docs change, and a single `ci-success` job to use as the required status check. Windows/macOS get the full build and test via the `full-ci` PR label, a weekday schedule, or a manual run
- PRs must now add an entry under `## [Unreleased]` in `CHANGELOG.md` (`Changelog entry` check; docs-only PRs, Dependabot and the `no-changelog` label are exempt)
- Reusable `build-packages.yml` workflow shared by the release and the manual packaging test, so the packaging steps no longer drift
- `rust-toolchain.toml` (stable, with rustfmt and clippy) and `.github/dependabot.yml` for GitHub Actions, Cargo and npm updates (Bevy and its lock-step crates are excluded from major/minor updates)
- Helper scripts `next_version.sh`, `prepare_changelog.sh`, `extract_release_notes.sh` and `check_changelog.sh` under `scripts/`
- Release workflow `preflight` job: fails in seconds, on dry runs too, if `RELEASE_APP_ID` or the `RELEASE_APP_PRIVATE_KEY` secret is missing, or if the key was stored as a variable. Dry runs now also mint the App token, so a bad key fails the dry run instead of the real release.

### Changed
- First stable release of the 0.3 line; promotes v0.3.0-rc.3. App changes in this line are listed under v0.3.0-rc.1 – rc.3 below.
- `rust-toolchain.toml` pins Rust 1.99.1 instead of tracking `stable`, so CI and contributors build and lint with the same compiler
- Releases are now cut by manually running the **Release** workflow (`workflow_dispatch`) with a `bump` (patch/minor/major), `channel` (stable/rc) and optional `dry_run`. The workflow bumps `Cargo.toml`/`Cargo.lock`, promotes the changelog, snapshots versioned docs for stable major/minor releases, builds all platforms from a staging branch and only then fast-forwards `main`, tags and publishes. Merging to `main` no longer creates a tag or release
- `deploy-docs.yml` is now called by the release workflow (and can be run by hand with a `ref`) instead of firing on tag pushes, and no longer pushes a commit back to the repository
- GitHub Actions are pinned to commit SHAs and CI uses `Swatinem/rust-cache`, concurrency groups and job timeouts; CI sets `CARGO_INCREMENTAL=0` and `CARGO_PROFILE_DEV_DEBUG=0` to speed up Windows builds
- `prune_versioned_docs.sh` links retired docs to the release tag instead of a `release/vX.Y.Z` branch
- GitHub Actions bumped (checkout 7.0.1, upload-artifact 7.0.2, download-artifact 8.0.2, setup-node 7.1.0, setup-dotnet 6.0.0, upload-pages-artifact 5.0.0, deploy-pages 5.0.1, create-github-app-token 3.3.0, rust-cache), ported from Dependabot #46
- CI fails on any clippy warning except `type_complexity` and `too_many_arguments`; those 42 Bevy-signature warnings are listed in the CI job summary until the files are restructured. The rest of the 121-warning backlog is cleared

### Removed
- `pr.yaml`, `release.toml`, `scripts/verify_release_pr.sh` and the `auto-tag` job: version bumps, changelog headings and tags are no longer managed by hand
- `-beta.N` pre-release tags; release candidates (`-rc.N`) are the only pre-release channel
- Unused `lfs: true` checkout options (the repository does not use Git LFS)

### Fixed
- Clippy warnings fixed in `src/systems/` (no behaviour change)
- `cargo fmt` drift and two deny-by-default `clippy::absurd_extreme_comparisons` errors (always-true `u32 / 2 >= 0` time-warp checks) so the new fmt/clippy gate starts green
- Clippy warnings fixed in `src/components/`, `src/resources/`, `src/plugins/` and `src/tests/` (no behaviour change)
- GitHub Release descriptions were empty because the changelog headings (`## [v0.3.0-rc.3]`) did not match what the release job searched for (`## [0.3.0-rc.3]`); extraction now accepts both forms
- Clippy warnings fixed in `src/ui/` (no behaviour change)
- Dependency updates from Dependabot #48 (serde 1.0.229, serde_json 1.0.154, arboard 3.7.0). brahe stays at 1.3.2: brahe 1.6 pulls nalgebra 0.35 next to our 0.34 and the build fails on mismatched `Vector6` types. Dependabot no longer proposes brahe/nalgebra minor bumps on their own.
- Release workflow: the versioned-docs snapshot step failed on current npm (`npm pkg set version "$VERSION"` → `npm pkg set version="$VERSION"`), which would have broken the first stable major/minor release

---

## [v0.3.0-rc.3] 🔬 - 2026-10-18

### Fixed
- Fixes `cargo build` failing on Rust 1.99+ with `E0512: cannot transmute between types of different sizes` by updating the transitive `ethnum` dependency (via `brahe` → `polars`) from 1.5.2 to 1.5.3 in `Cargo.lock`

### Changed
- Updated guides in the contributing section to link to new install guides
- Removed `base_url` from `tsconfig.json`
- Updated `docusaurus.config.js` to show the versions dropdown in the upper right corner and to allow for the latest unreleased docs to be viewed
- Updates `custom.css` to add styling for screenshots in the install guides
- Updates `prerequisites.md` and `development.md` to link to the new macOS development setup guide
- Reorders the Getting Started sidebar so the install guides come before Prerequisites and Running the App, and splits the Introduction's "Where to go next" links into install, build-from-source, and architecture groups
- Corrects the macOS install guide (DMG filename now includes `-arm64`, uses the System Settings → Privacy & Security wording) and the Windows install guide (MSI filename, completed installation, first launch, update/uninstall, and troubleshooting sections)
- Updates `pr.yaml` and `deploy-docs.yml` to build the docs site with the latest Node.js LTS (`node-version: lts/*`) instead of Node 20
- Updates the docs site npm dependencies and adds an `allowScripts` allowlist to `docs_site/package.json` for the `@swc/core`, `core-js`, and `fsevents` install scripts

### Added
- Install guides for Linux, Windows, and macOS in the docs site. The Windows and macOS guides include installation steps and screenshots; the Linux guide is based on the release packaging and has not yet been verified on a Linux install.
- Adds `macos-dev-setup.md` to the contributing docs with a from-scratch Apple Silicon setup guide (Xcode Command Line Tools, Homebrew, Rust, VS Code, Node.js, release tooling, and Tracy)

### Removed
- Deleted old assets from initial Docusaurus setup that are no longer used in the current docs site design


## [v0.3.0-rc.2] 🔬 - 2026-04-21

### Changed
- Updates systems to support orbital selection for the chaser and target


## [v0.3.0-rc.1] 🔬 - 2026-04-14

### Changed
- Updates comments in `release.toml` to clarify the updated release process
- Updates `deploy-docs.yml` to trigger on pushes to tags only, adds steps to sync the changlog with the docs to publish it, build the versioned docs, adds step to verify the versioned docs integrity before deploying
- Updates `pr.yaml` to trigger on PRs raised against `dev` and pushes on `dev`, adds steps to test build the docs (but not deploy), and adds steps to auto-tag the release
- Updates docs in the `/contributing` section to break out release info into its own doc and clarify info about versioned docs
- Update `post_release.sh` to clarify comments and add the CHANGELOG → docs sync
- Adds `v0.2.0` docs version from branch `release/v0.2.0`

### Added
- Adds `release-workflow.md` to the contributing docs with detailed instructions on how to cut a release
- Adds `cicd-workflows.md` to the contributing docs with an overview and reference for all CI/CD workflows in the project
- Adds `prune_versioned_docs.sh` script to manage pruning old versioned doc sets beyond the 10-version cap

### Removed
- Removes `post_release.sh` as cargo-release 1.x does not support a post-release hook; the versioned docs snapshot is now created automatically by `deploy-docs.yml` on tag push for major/minor releases, and the pruning script can be run manually if needed.
- Removes `v0.2.0-beta.1` docs version from the versioned docs site as part of the pruning process; the docs for this release are still available in the `release/v0.2.0-beta.1` branch on GitHub.


## [v0.3.0-beta.2] 🧪 - 2026-04-14

### Changed
- Refactors capture algorithm systems to emit `LogEvent` messages for significant events (state transitions, capture success/failure, errors) instead of direct `println!` statements.
- Updates docs to include information about the new capture log terminal panel and its features.

### Added
- Capture log system and terminal panel UI for real-time logging of capture events, errors, and debug information.
- Added new `capture_log.md` document detailing the features and usage of the capture log terminal.


## [v0.3.0-beta.1] 🧪 - 2026-04-14

### Changed
- Updates `orbital_entities` to be names `orbital_cache`
- Updates required packages to support the use of `egui` and to use certain crate packages from our `bevy` forked version

### Added
- A new feature to graph the relative velocity and forces applied to the target object during capture attempts, with a toggle in the UI to show/hide the graph.
- A data collection feature which would eventually allow you to export data captured by the app matching the graphs presented in the UI
- Adds center of mass gizmo which highlights RSOs with a box around them with a label


## [0.2.0] 🚀 - 2026-04-14

### Release Notes
- This is the first public production release of SSG Tether Capture, minor version 0.2.0
- App installers are available for Windows (MSI), macOS (DMG), and Linux (tar.gz)
- This release of the application includes the following new features:
  - Project level support to create, edit, and save capture plans to directory on your machine
  - A user interface to simulate the capture of a RSO (Resident Space Object) using a flexible tether
  - Capture plans allow you to define various parameters of the capture scenario including distance, force, and velocity thresholds to define when the capture moves through phases
  - The Project Detail view presents various simulation controls, a simulation HUD (heads up display), a section which provides detailed information on the progress within the capture plan, and a catalog of space objects to choose from when defining your capture scenario
  - The app includes an example capture plan (`assets/capture_plans/example_capture_plan.json`) which simulates the capture of a piece of debris by a tethered satellite; you can use this as a starting point to create your own capture plans
- This release includes an updated documentation site which includes various information about the project, how to get started, and guides on how to use the app and create your own capture plans; the docs site is available at https://conjunction-crew.github.io/ssg-tether-capture/


## [0.2.0-beta.6] 🧪

### Changed
- Updates `release.yml` to resolve ad-hoc signing issues making the resulting DMG/.app unusable on macOS
- Updates directory resolution strategy to use platform-correct asset paths (e.g., `assets/datasets/`) and support correct resolution when running from a macOS `.app` bundle
- Adds JSON config to store the working directory path and updates the UI to read/write from this config for persistence across app restarts


## [0.2.0-beta.5] 🧪 - 2026-04-13

### Changed
- Updated release workflow and scripts to resolve issues with the Windows version of the app and improved the MSI installer
- Updated orbit controls to support holding Ctrl + mouse drag for orbiting in the 3D view
- Updated `post_release.sh` script to only trigger versioned docs for major and minor releases and limit versioned docs to last 10 releases
- Changes favicon for the docs site and updates the metadata used for link previews

### Added
- Adds `.bat` and `.ps1` scripts for checking and installing prerequisites on Windows (e.g., WiX Toolset for MSI packaging)
- Adds assets for the Windows MSI installer banner and dialog backgrounds
- UI buttons to control orbit/zoom controls within the 3D view (in addition to mouse controls)

## [0.2.0-beta.4]
- No change log entry provided for this pre-release version. Please refer to the commit history for details on changes included in this release.

## [0.2.0-beta.3]
- No change log entry provided for this pre-release version. Please refer to the commit history for details on changes included in this release.

## [0.2.0-beta.2]
- No change log entry provided for this pre-release version. Please refer to the commit history for details on changes included in this release.


## [0.2.0-beta.1] 🧪 - 2026-04-10

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


## [0.1.1-beta.1] 🧪 — 2026-04-08

### Changed
- Updated release workflow to build and package artifacts for Linux, Windows
  (MSI via WiX), and macOS (DMG via cargo-bundle) on tag push.
- Pre-release (beta) tags now create a GitHub pre-release; stable tags create
  the production release marked as latest.


## [0.1.0] 🚀 - 2026-04-03

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
