---
sidebar_position: 2
---

# CI/CD Workflows

This page is a reference for all GitHub Actions workflows in the project. For the
step-by-step process of cutting a release, see [Release Workflow](./release-workflow.md).

## Overview

```
Pull request / push to main / weekday schedule / manual
      │
      └─ ci.yml
           ├─ Detect changes
           ├─ Lint and test (Linux)       ← fmt + clippy + cargo test   (the merge gate)
           ├─ Coverage (Linux)            ← cargo-llvm-cov, line coverage per area, gates enforced
           ├─ Check (Windows, macOS)      ← cargo check on PRs; full build + test with `full-ci`
           ├─ Build docs site             ← only when docs/changelog change
           ├─ Changelog entry             ← PRs only
           └─ ci-success                  ← the one required status check

Release (manual: Actions → Release → Run workflow)
      │
      └─ release.yml
           ├─ Verify (fmt, test)  ┐
           ├─ Prepare release commit   → pushes release-staging/vX.Y.Z
           ├─ Build packages ──── build-packages.yml (Linux / Windows / macOS)
           ├─ Publish GitHub Release   → fast-forwards main, pushes tag, creates release
           └─ Deploy docs ─────── deploy-docs.yml (stable major/minor only)

Packaging Test (manual) ──── build-packages.yml
```

Merging to `main` never creates a tag or a release. Only the Release workflow does.

## Workflow reference

### `ci.yml` — pull-request checks

**Triggers**: pull request (any base branch; re-runs when labels change), push to `main`, weekday schedule (07:00 UTC) and `workflow_dispatch`.

| Job | Runs on | Purpose |
|---|---|---|
| `changes` | ubuntu-latest | Decides which of the jobs below are relevant, based on the files a PR touches and on the `full-ci` label |
| `rust-linux` | ubuntu-latest | `cargo fmt --check`, `cargo clippy --all-targets`, `cargo test`, then prints the test execution time. Runs when `src/`, `crates/`, `assets/`, `wix/`, `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml` or the CI/packaging workflows change. Clippy fails the job on any warning except `clippy::type_complexity` and `clippy::too_many_arguments` (Bevy system signatures that are tracked, not yet fixed). A non-failing step lists those in the job summary ("Tracked clippy warnings: N"). A "Test execution time" step prints the sum of the `finished in` times of every test binary to the job summary and as an annotation (`gh run view <id>` shows it). On full runs (weekday schedule, manual runs, `full-ci` PRs) it also runs the ignored long determinism test `cli_three_runs_to_capture_identical` ("Determinism to capture (ignored tests)") |
| `coverage` | ubuntu-latest | Runs all workspace tests once under `cargo llvm-cov` (in parallel with `rust-linux`, so the uninstrumented test time stays the timing number), prints a per-area line-coverage table to the job summary and uploads the `coverage-report` artifact (HTML, `lcov.info`, `summary.json`; kept 14 days). The areas and thresholds are in `scripts/coverage_gates.conf`: `ssg-sim` 80 %, `capture/`, `orbit/`, `physics_bubble`, `floating_origin` and `time` 85 %, `ssg-sim-cli` 70 %, `ssg-app` reported only. **The gates are enforced** (the summary says "enforced"): a pull request fails the job, and so `ci-success`, if any gated area drops below its threshold. The instrumented build rebuilds every dependency, so it never reuses the `rust-linux` cache: expect about 20–30 min until `main` has saved a coverage cache |
| `cross-platform` | windows-latest, macos-latest | PRs: `cargo check --all-targets` only. With the `full-ci` label, on the weekday schedule and on manual runs: `cargo build` + `cargo test`, then the same "Test execution time" summary line, then the ignored determinism-to-capture tests: `cli_three_runs_to_capture_identical` on both platforms and, on macOS only, `baseline_q13_to_capture` (its goldens hold on macOS/aarch64 only; see [Determinism](../concepts/determinism.md)) |
| `docs-build` | ubuntu-latest | Docusaurus build, plus a check that `versions.json` has at most 10 entries. Runs when `docs_site/**` or `CHANGELOG.md` change |
| `changelog` | ubuntu-latest | PRs only: fails unless the `## [Unreleased]` section of `CHANGELOG.md` was changed. Exempt: docs-only PRs (`docs_site/**` and `*.md`), PRs labelled `no-changelog`, and Dependabot |
| `ci-success` | ubuntu-latest | Passes when every job above passed or was skipped. **This is the only status check branch protection needs to require**, so skipped jobs never block a merge |

Why Windows and macOS only run `cargo check` on PRs: the full cross-platform build was the slowest part of every PR (Windows especially). `cargo check` still catches compile errors in platform-specific code. Linking and packaging problems are caught by the weekday run and, definitively, by the release build. Add the **`full-ci`** label to a PR when you change platform-specific code or build configuration and want the full run before merging.

The workflow sets `CARGO_INCREMENTAL=0` and `CARGO_PROFILE_DEV_DEBUG=0`. The `dev` profile enables debug info, which makes Windows link times and cache sizes balloon and which CI never needs. Caches use [`Swatinem/rust-cache`](https://github.com/Swatinem/rust-cache) and are saved only from `main`, so PR runs cannot evict the shared cache.

**Coverage**: the gates count line coverage of production code from one instrumented run of all workspace tests (`cargo llvm-cov --workspace`), so tests in `ssg-app` and `ssg-sim-cli` count towards the `ssg-sim` lines they execute. Test code is never counted: `crates/*/src/tests/`, `crates/ssg-sim/src/test_support/` and `crates/*/tests/` are excluded by `scripts/coverage_gate.sh` (inline `#[cfg(test)]` modules were moved into `tests/` files for this). To reproduce the CI numbers locally, with the same settings (without them the instrumented build needs tens of GB of disk):

```bash
CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0 cargo llvm-cov --workspace --locked \
  --json --summary-only --output-path target/coverage-summary.json
sh scripts/coverage_gate.sh target/coverage-summary.json
```

This needs `cargo-llvm-cov` 0.9.1 and `rustup component add llvm-tools-preview`. Local numbers can differ from CI by a few lines (platform `cfg`); the CI job is the gate.

**Cache keys**: rust-cache hashes every installed Rust toolchain into its key, and the set preinstalled on the runner image alternates between images (1.99.0 present or not), which used to turn most PR runs into total cache misses (Q39). Each job therefore runs a "Remove other preinstalled toolchains" step before the cache step, so only the toolchain pinned in `rust-toolchain.toml` is installed and the key depends only on that pin, the lockfile and the manifests. (rust-cache's `add-rust-environment-hash-key: false` is not used: in the pinned version it also drops the lockfile hash, so the key would never change and `main` would never refresh its cache.) On Windows and macOS the key also carries `check` (PRs and pushes to `main`, which only run `cargo check`) or `build` (full runs), so the check cache cannot shadow the build cache. Only `main` saves, so the schedule is what refreshes the `build` cache.

**Test time**: the "Test execution time" line (job summary and annotation) is the number later stages compare against when a change might slow the tests down (a trigger fires at the reference plus 25 s). It is noisy: the same code has varied by about 10 s between runs and more between runner images (60 to 87 s before the P5 scenario tests, which added roughly 25 s), so it is a tripwire, not a benchmark. The reference is the median of the two v0.3.3 runs (the release PR and the push to `main` after it). See [Testing](./testing.md) for what the tests are and how coverage is gated.

---

### `release.yml` — manual release

**Triggers**: `workflow_dispatch` only. See [Release Workflow](./release-workflow.md) for how to run it.

| Input | Values | Purpose |
|---|---|---|
| `bump` | `patch`, `minor`, `major` | Which component to bump. Ignored when the current version is an `-rc` |
| `channel` | `stable`, `rc` | `rc` publishes a pre-release candidate; `stable` a full release |
| `dry_run` | `true` / `false` | Computes the version, changelog and docs changes and shows them in the run summary, but pushes and publishes nothing |

| Job | Purpose |
|---|---|
| `verify` | `cargo fmt --check` and `cargo test` on the commit being released |
| `prepare` | Computes the next version (`scripts/next_version.sh`), bumps `Cargo.toml` and `Cargo.lock`, promotes `## [Unreleased]` in `CHANGELOG.md` (`scripts/prepare_changelog.sh`), snapshots the versioned docs for stable major/minor releases, and pushes the single result to a scratch branch `release-staging/vX.Y.Z`. `main` is **not** modified |
| `build` | Calls `build-packages.yml` on the staged commit |
| `publish` | Only if everything above succeeded: fast-forwards `main` to the staged commit, pushes the `vX.Y.Z` tag, creates the GitHub Release (using the changelog section as its description) and deletes the staging branch. Safe to re-run after a partial failure |
| `docs` | Calls `deploy-docs.yml` for stable major/minor releases |

Because `main` only moves after all platform builds are green, a failed build leaves no half-released state behind. If `main` moved while the release was building, `publish` refuses to continue; run the workflow again.

---

### `build-packages.yml` — reusable packaging

**Triggers**: `workflow_call` only (from `release.yml` and `packaging-test.yml`).

| Job | Runs on | Output |
|---|---|---|
| `build-linux` | ubuntu-latest | `.tar.gz` (binary + `assets/`) |
| `build-windows` | windows-latest | WiX `.msi` (pre-release suffix stripped from the MSI `PackageVersion`) |
| `build-macos` | macos-latest | ad-hoc signed `.app` in a `.dmg` |

Artifacts are uploaded as `linux`, `windows` and `macos`.

---

### `deploy-docs.yml` — Docusaurus deployment to GitHub Pages

**Triggers**: `workflow_call` (from `release.yml`, stable major/minor releases only) and `workflow_dispatch` (re-deploy an existing release by giving its tag as `ref`).

| Job | Purpose |
|---|---|
| `build` | Builds the site from the release tag, syncs `CHANGELOG.md` into `docs/release-notes.md` and the matching versioned snapshot, and verifies that the snapshot exists and `versions.json` has at most 10 entries |
| `deploy` | Deploys the artifact to the `github-pages` environment |

Patch releases and release candidates do not add a versioned docs set, so the live site is left unchanged for them. The versioned docs snapshot itself is created by `release.yml` as part of the release commit, so the tag always contains it.

---

### `packaging-test.yml` — Manual packaging smoke test

**Triggers**: `workflow_dispatch` only (manual)

Runs `build-packages.yml` (the same packaging jobs as a real release) on any branch without publishing anything. Use this to validate packaging changes before a release.

---

### Dependabot

`.github/dependabot.yml` opens weekly grouped PRs for GitHub Actions versions (actions are pinned to commit SHAs), Cargo dependencies and the docs-site npm packages. Bevy and the crates that move in lock-step with it (`bevy*`, `avian3d`, `egui_plot`) are excluded from major/minor updates because Bevy is patched to the project's fork. Upgrade those by hand.

---

## Repository setup

These settings live in GitHub, not in workflow files, and must be configured by a repository admin.

### Branch ruleset — `main`

| Setting | Value |
|---|---|
| Pattern | `main` (the default branch) |
| Require a pull request before merging | ✅ |
| Required status checks | `ci-success` only |
| Prevent force pushes | ✅ |
| Prevent deletion | ✅ |
| Bypass list | The release GitHub App (so the Release workflow can fast-forward `main`) |

Also enable **Automatically delete head branches** under Settings → General.

### Tag ruleset — version tags

| Setting | Value |
|---|---|
| Pattern | `v*.*.*` |
| Bypass list | The release GitHub App only |
| Enforcement | Active |

This blocks developers from pushing version tags directly. The only source of a `v*.*.*` tag is the Release workflow.

### Release GitHub App

| Item | Used by | Purpose |
|---|---|---|
| GitHub App with **Contents: read & write** installed on this repository | `release.yml` | Pushes the staging branch, fast-forwards `main` and pushes the tag. Add it to the bypass list of both rulesets above. Unlike a personal access token it is not tied to one person's account |
| Repository variable `RELEASE_APP_ID` | `release.yml` | The App's ID |
| Repository secret `RELEASE_APP_PRIVATE_KEY` | `release.yml` | The App's private key (PEM) |

Add the variable and secret under **Settings → Secrets and variables → Actions**. The private key goes in the **Secrets** tab, never in **Variables** (variables are stored and shown in plaintext).

The Release workflow's `preflight` job enforces this. It runs first, on dry runs too, and fails within seconds if `RELEASE_APP_ID` or the `RELEASE_APP_PRIVATE_KEY` secret is empty or missing, or if a repository *variable* named `RELEASE_APP_PRIVATE_KEY` exists. If it reports a variable, delete it, rotate the App's private key and store the new key as a secret.
