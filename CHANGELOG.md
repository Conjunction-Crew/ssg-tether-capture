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
- Capture actuation (F-31, F-41): the controller pushes the tether root in every phase, and the tail only in a phase that sets the new optional plan key `phases[].tail_actuation: true` (the example plan sets it nowhere, so its capture phase no longer winds the tail the other way). `max_force_n` is now an upper bound: the force rises from 0 to it over 1 s at the capture start and at every phase entry (a change of direction does not restart the rise), and the controller's three branch thresholds (the inner and outer edges of the capture sphere and the speed limit) have a 2 % dead band, so a node at a threshold no longer flips direction every step. The app's capture gizmo draws the force the controller applied instead of re-deriving it, and the HUD calls `max_force_n` a bound. `project.json` now applies the chaser's `mass_kg` (the tether root is the chaser) and the `inertia_kg_m2` of both bodies (a symmetric, positive-definite tensor in the sim's ECI-aligned axes at spawn); `orbit.relative_to_target` is still refused, now "applied from v0.4.0-rc.5". The fixed-epoch results move: the Q13 6,400-step state hash, the first transition (step 4,318 to 4,769 on macOS) and the floating-origin reset steps; the plan, project and recording-manifest JSON Schemas are regenerated.


## [v0.4.0-rc.2] 🔬 - 2026-10-31

### Added
- Simulation recording core: a capture that enters an end phase (`end: success` or `abort`) now writes a `CaptureEnded` event and the log line `Capture ended: <outcome> (phase <id>)`, and the controller keeps running. The sim keeps a log of its inputs for every session: the fixed steps run before the orbits start, then each capture start with its step. A recorder can be started and stopped by hand, or starts on its own when the capture enters a chosen phase. It takes full-state frames of the target and every tether body every K steps (default 32), each with the frame origin's orbit and the floating-origin shift since the previous frame, and keeps the capture transitions. It stops on its own 60 s after the end event, at the duration cap (30 min) or at the size cap (100 MB), warning at 80 %. It refuses a second recording, and a tether of more than 200 bodies. A run with a recording is bit-identical to the run without.
- Recording files: `ssg-sim run --record NAME` writes the run as a recording folder, `<project>/recordings/NAME/`, built in `NAME.partial/` and renamed when it is complete: `manifest.json`, `inputs.jsonl` (the input log from step 0, the spawn-update step count first), `keyframes.parquet` (one row per body per frame, exact doubles, 64 frames per row group), `frames.jsonl` (step, epoch, phase, sphere radius, frame origin, origin shift), `transitions.jsonl` and `log.jsonl`. The recording starts with the capture command, or when the capture enters `recording.auto_start_phase` of `project.json` (new optional block: automatic start and stop, frame interval, duration, size and tether-body caps) or `--record-from-phase`, and ends with the run; the run's state hash is unchanged. `ssg-sim validate` checks every recording under `recordings/` and warns on an unfinished `.partial` folder (a crash leaves no readable Parquet footer; make the recording again). The recording manifest gains optional keys (`project`, `source`, `end_step`, `end_reason`, `start_trigger`, `start_phase`, `limits.max_size_mb`) and both JSON Schemas are regenerated. New dependency: `parquet` 60.0.0 without Arrow or compression codecs.
- Recording replay: `ssg-sim replay <recording-dir> [--assets DIR] [--frames-only]` checks a recording's files and re-simulates it from step 0 with its scenario, plan and input log, including the number of fixed steps the spawn update ran, and compares every frame bit for bit. Exit 0: every frame identical; 1: a difference with the same target and version (a defect, or a different C library or toolchain); 3: a difference with another target or version (expected across platforms and versions); 2: usage error, invalid recording, or an app recording without its spawn-update step count. A library player (`ssg_sim::recording::player`) shows a recording at any step: frames as recorded, and between frames an interpolation that accounts for floating-origin resets, with the exact capture phase, step to the next or previous frame, seek, play and reverse at 1/8× to 64×. The sim's per-update step cap is now a resource (`StepCap`, default 64); the CLI's `--warp` limit is unchanged.
- Recording and replay in the app (egui): a **Recording** window on the sim screen (Record and Stop, the state, duration, frames and size so far, the projected size at the caps with a warning at 80 %, and the per-project recording settings, which Save writes into `project.json`), a **Recordings** window in the sim setup that lists `<project>/recordings/` (name, duration, bodies, size, and how the recording's build differs from this one) and opens one, and a replay mode, which opens in the detail view following the tether root, with a **Transport** window: jump to the start or end, step to the previous or next frame, play and pause, reverse, speeds from 1/8× to 64×, a scrub slider, step, epoch, phase, range, relative-speed and capture-sphere readouts with a marker for interpolated states, buttons to switch between the map and the detail view and to follow the debris or the tether root, and Exit back to the setup screen. A replay shows the recorded states and runs no physics; the Earth is placed from the recording's frame origin, so it and the scene move continuously across a floating-origin reset; the capture button, plan editing and the warp keys are off. Recordings made in the app start at a click or when the capture enters the phase set in `project.json` and stop at the end event plus the set delay, at the duration or size cap, or at a click; `ssg-sim replay` reads them, but they do not yet re-simulate bit for bit (see the known limit under Documentation).
- Documentation: a **Recordings** page (what a recording holds, including the spawn-update step count, the frame origin and the origin shifts; starting and stopping, the end event; sizes; playback controls, origin-aware interpolation and step snapping; `ssg-sim replay` and its exit codes; unfinished `.partial` recordings), and updates to Project files (the recording layout and manifest keys, the `recording` block, the end phase; the `keyframes.csv` fallback is gone), Determinism, Time warp (the adaptive cap) and Running without a window (`--record`, `--record-from-phase`, `replay`). Known limit: a recording made in the app does not yet re-simulate bit for bit, even on the machine that made it; recordings made by `ssg-sim run --record` do.
- The app adapts the per-update step cap to the machine: it halves the cap (down to 8) after frames longer than 50 ms that ran at the cap and doubles it (up to 512) after frames under 25 ms while the requested warp is higher, at most every half second. The cap changes how many steps run per update, never which steps. The `--warp` limit of the CLI stays 64.

### Changed
- CLI: `ssg-sim run` prints the warnings of `project.json` and of the plan it runs (a v0 plan's unmapped values, a missing dataset, an oversized tether) on stderr as `ssg-sim: warning: <line>`, worded and ordered as `ssg-sim validate` prints them; stdout and the output files are unchanged.
- Starting a simulation in the app rewrites `project.json` from the current selection and now keeps its `recording` block.

### Fixed
- OEM export: `OBJECT_ID` is the GP record's own (for Hubble `1990-037B`) in `ephemeris.oem` from `ssg-sim run` and in the app's Export OEM, and `project.json` keeps it in the GP snapshot; before, it repeated the body's name.
- The release build of `ssg-sim` has no unused-import warning; the headless CLI docs list every `run` output and lose a stray line.

### Removed
- The unused `csv` dependency of `ssg-sim-cli` and `ssg-app`.

## [v0.4.0-rc.1] 🔬 - 2026-10-29

### Added
- Project files: a project folder can hold a `project.json` manifest (schema v1): the start epoch (microsecond UTC), the active plan, and the target and chaser as a copy of their Space-Track record or as osculating elements in metres and radians, with the build that wrote it. The app writes it when a run starts and starts from what it read back; opening a project restores its epoch and bodies, and a new project starts at the wall clock rounded to the microsecond. `<project>/datasets/*.json` are part of the catalog. `config.json` carries `"schema_version": 1` (a config without it is read as before; one from a newer version is ignored with a warning). Chaser mass, both inertias and a chaser placed relative to the target are in the schema but refused until v0.4.0-rc.4. The JSON Schema is published as `schemas/project.v1.schema.json` (F-24)
- CLI: `ssg-sim validate <project-dir>` checks `project.json`, every plan file, the active plan and the orbits and prints `error:`, `warning:` and `info:` lines (exit 0 / 1 / 2); `ssg-sim run <project-dir>` takes the target, chaser, epoch and plan from `project.json` when it exists (the flags override it and say so on stderr) and then writes the same files as the flag run of the same scenario
- Capture plans and the manifest: the simulation applies `device.segment_spacing_m`, `device.segment_mass_kg`, `device.end_mass_kg` (root and tail), `device.joint_damping_linear` / `joint_damping_angular` and `capture_sphere.initial_radius_m` from the plan, and the target's `mass_kg` from `project.json` (a plan or manifest that gives none takes the values the simulation used before: 0.1 m, 0.1 kg, 2 kg, damping 1.0 / 1.0, 25 m, 2,500 kg, so every existing run is unchanged); a tether of more than 1,000 bodies is refused and one of more than 200 warns. The chaser's mass, both inertias and the relative start stay refused until v0.4.0-rc.4
- Tests: `manifest_run_equals_flag_run` (a manifest run and a flag run give byte-identical `trajectory.csv` and equal `summary.json`), `validate` outputs and exit codes, one test per manifest rule, 17-digit elements bit for bit through `project.json`, 300 GP records and 300 selections through the manifest, 2,000 epoch strings
- Exports: CCSDS OEM ephemerides (OEM 2.0 text, GCRF, UTC; target and chaser segments; creation date = the run's start epoch, so the same run writes the same bytes). `ssg-sim run` writes `ephemeris.oem` next to `trajectory.csv` (one state per sample), and the app's Data Collection window has an **Export OEM** button (one state per simulated second of the running capture). Saved capture logs carry `epoch_utc` (microsecond UTC) on every line next to `timestamp`
- Recordings format: the layout of `recordings/<name>/` (`manifest.json`, `inputs.jsonl`, `keyframes.parquet` or `keyframes.csv`) is fixed, and `ssg_sim::project::recording` has the `RecordingManifest` type (schema v1: the build, the plan id and a copy of the plan, start step and epoch, keyframe interval, the body table and the segment limits) with its reader (unknown keys and newer `schema_version`s refused, the key named), validator and writer; the JSON Schema is published as `schemas/recording-manifest.v1.schema.json`. Nothing records yet; the recorder arrives in a later release. New "Project files" concepts page describing the project folder, `project.json`, plan schema v1 and how v0 plans open, validation, the compatibility rule for new keys, the exports and their columns, the recordings layout and the three schema files

### Changed
- Docs: every non-dry-run release now deploys the docs site. Patch and release-candidate releases appear as two rolling versions (latest patch, latest pre-release) built from the release tags at deploy time, outside the 10-version cap; stable X.Y.0 releases keep their permanent snapshot. The default docs version is now the newest stable one computed from `versions.json` (it was hard-coded to 0.2.0 while 0.3.0 existed, F-54). `scripts/docs_versions.sh plan <version>` shows what a release publishes; the Release run summary prints it, dry runs included. `deploy-docs.yml` gains `version` and `deploy` inputs (a build-only run uploads the site as an artifact).
- Capture plans: plan files have a typed schema v1 (`"schema_version": 1`): `phases` instead of `states`, metric unit-suffixed keys (`max_velocity_mps`, `max_force_n`, `shrink_rate_mps`, `device.tether_length_m`), transition conditions under `all_of` (all must hold), optional device and `capture_sphere` fields with documented defaults, and an `end: "success" | "abort"` phase marker. Older (v0) plan files still load: they are migrated in memory without loss (every value v1 has no field for is kept under `migrated_from_v0.unmapped` with a warning; a transition with no numeric bound, which never fired, is dropped from the phase) and become v1 when saved from the plan form. Unknown keys and newer schema versions are refused, naming the key. The JSON Schema is published as `schemas/capture-plan.v1.schema.json` on the docs site; the example plan is a v1 file (F-15, F-24)
- Build: `serde_json` reads numbers with its `float_roundtrip` feature, so a number with 16 or 17 significant digits comes back bit for bit after a write and a read (about one in ten 17-digit numbers came back one ULP off before); it matters for the 17-digit orbital elements in `project.json`, and every golden, the determinism tests and the Q13 `state_hash` are unchanged
- CLI: `--epoch` also accepts a trailing `Z`; the epoch format and parse moved into `ssg_sim::project` (`format_utc`, `parse_utc`) and are shared with the manifest
- Exports: CSV columns carry units and the phase term. CLI `trajectory.csv`: `step,epoch_utc,phase,range_m,rel_speed_mps` (was `step,epoch,state,…`), and `summary.json` has `final.phase` (was `final.state`); rows and `state_hash` are unchanged. App CSV: `seconds_since_start_s,range_m,rel_speed_mps` (was `seconds_since_start,rel_position,rel_velocity`), written to `<project>/exports/` (was the project folder itself). All export files are written by `ssg_sim::project::export`

### Fixed
- Capture plans: a wrong type is reported as such with its location (a string number was reported as a missing parameter), and negative limits, shrink rates and condition bounds are refused (a plan with a negative force loaded and reversed the push); every message reads `[<plan-id>] <json-pointer>: <message>` (F-32)
- Capture plans: the tether is always registered under the plan's own tether id, so the sidebar and the spawner agree; `Tether1` is used only when no plan is active, and a blank id cannot load (F-43)
- Capture plans: reloading the plans drops the compiled entry of a deleted or no longer valid plan (F-53)
- Exports: a saved capture log is named after the active plan id (it took the plan form's name, so it was `unnamed_…` outside the plan editor), and the app's CSV export name is the plan id with unsafe characters replaced (it was the plan's display name, unsanitised) (F-26, F-33)

## [v0.3.3] - 2026-10-29

### Added
- CI: a report-only `coverage` job runs the workspace tests under `cargo-llvm-cov`, prints per-area line coverage against the Q7 thresholds to the job summary and uploads a `coverage-report` artifact; the inline test modules of `orbit/lvlh.rs` and `ssg-sim-cli` `output.rs` moved to `tests/` files so coverage counts production code only (F-14)
- Tests: branch tests for the capture controller (force direction and magnitude per branch, the missing-root, tether and plan paths), for `apply_sim_commands` and for `assert_finite_state`, and property-style tests (fixed-seed generator, 300 plans per property) for capture-plan validation and compilation; `capture/` coverage reaches the 85 % gate; `plans/stage-5/results/pub-fn-inventory.md` starts the one-test-per-public-function inventory (F-14)
- Tests: `orbit/` tests for the catalog propagation (against brahe Kepler, with the F-50 mu difference bounded), display names, record conversion, every dataset-loader branch and log line, `init_orbitals`' SGP4 failure path, `cache_eci_states`, the selection types and the LVLH frame, with property-style element-conversion tests; `orbit/` coverage reaches the 85 % gate (F-14)
- Tests: every remaining public function of `ssg-sim` has a test (`plans/stage-5/results/pub-fn-inventory.md` is complete): plan loading and saving, `load_all` / `reload_user_plans` and the config-file helpers in temporary directories, the tether and debris spawners, the session lifecycle and capture-log wiring, the warp ladder and fixed-step loop, relative gravity and the centre-of-mass edge cases; `project.rs` gains two path-taking helpers (`read_config_file`, `write_config_file`) that `load_from_config` / `save_to_config` call, with no change in behaviour (F-14)
- Tests: out-of-crate scenario tests in `crates/ssg-sim/tests/` (the Q13 run, every example capture plan on the Q13 pair, pause and warp), `ssg-sim-cli`'s `harness_parity` test that closes F-51 (the CLI trajectory and the harness checkpoints are bit-identical at the same absolute step; the step offset is documented in `scenario.rs` and `q13::build`) and four CLI error-path tests; `ssg-sim` and `ssg-sim-cli` get a `test-support` dev-dependency on `ssg-sim` (F-51)

### Changed
- CI: the coverage gates are enforced (the `--report-only` flag is gone): a PR that drops `ssg-sim` below 80 %, `capture/`, `orbit/`, `physics_bubble`, `floating_origin` or `time` below 85 %, or `ssg-sim-cli` below 70 % fails the `coverage` job (F-14, Q7)
- Docs: the testing page is rewritten as the testing strategy (layers, the `test_support` harness, property-style tests, determinism in tests and the F-51 offset, coverage gates, test-time budget) and no longer mentions `create_app` (F-16)
- Tests: `harness_parity` runs the CLI binary and the in-process harness at the same time, so the test costs the slower run instead of the sum (about 3 s locally, about 13 s on Linux CI), keeping the Linux test execution time down (Q45)
- CI: the long determinism-to-capture tests (`cli_three_runs_to_capture_identical` on Linux, macOS and Windows; `baseline_q13_to_capture` on macOS only) now run on the weekday schedule, manual runs and `full-ci` PRs, so the release PR checks them before a release
- CI: only the pinned Rust toolchain is installed before the cache step, so the Rust cache key no longer alternates with the runner image's preinstalled toolchains and PR runs stop missing the cache; Windows and macOS use separate check and build keys; the Linux and full cross-platform test jobs print "Test execution time" to the job summary (Q39)

## [v0.3.2] - 2026-10-28

### Added
- `ssg-sim run`: `--steps` and `--sample-every` count fixed steps and must be multiples of `--warp` (at most 64); `summary.json` carries a `state_hash`; transitions carry their exact step; determinism tests (two runs identical, 1× vs 16× identical) and the determinism contract page (F-17)

### Changed
- Time warp: explicit pause state, ladder tops out at 8192×, work per update is capped and the HUD shows the effective warp (F-12)
- Cycling the camera target is a pure viewing action: the floating origin and the physics bubble follow the simulation anchor (the tether root), not the camera (F-25)
- Capture plan transitions: all conditions of a transition must hold, at most one transition fires per physics step, no cascades (F-24, Q14)
- Bodies inside the physics bubble feel the differential two-body acceleration about the anchor's orbit (Clohessy-Wiltshire relative dynamics, F-09); relative velocity for the controller, transitions, HUD, plots and CSV is measured in the target's LVLH frame (F-35); HUD altitude uses the WGS-84 equatorial radius (F-28)
- Catalog objects start from an SGP4 state at the simulation start epoch, computed from the full-precision record and its own epoch; edited elements are honoured at their epoch (F-10, F-27). `Orbit::FromTle` is replaced by `Orbit::FromGp` (the catalog is GP/OMM JSON, not TLE lines). Map points keep the lightweight Kepler model (documented offset)

### Fixed
- Simulation start epoch is an explicit input (`SimConfig`); the simulation core no longer reads the wall clock; the physics clock advances once per step (F-06, F-38)
- Physics bubble: one body without an orbit no longer stops bubble processing, bodies entering the bubble get their orbit rebuilt, and each leaving body gets its own relative state (F-07, F-23)
- Capture controller: zero-vector forces no longer produce NaN, a capture entity without a rigid body no longer stops the controller, the capture-sphere radius resets per capture (F-11, F-39)
- Scene and map are drawn in ECI axes; the Earth is on the physical nadir (F-08)


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
