---
sidebar_position: 4
---

# Release Workflow

Releases are managed with [cargo-release](https://github.com/crate-ci/cargo-release) and a set of GitHub Actions workflows. The key principle is that **merging to `dev` is the only manual step** — the version tag is created automatically by CI after each successful merge, and all downstream workflows (builds, GitHub Release, docs deploy) fire from that tag. For a full reference on how each workflow is structured and what triggers it, see [CI/CD Workflows](./cicd-workflows.md).

## Version scheme

The project follows [Semantic Versioning](https://semver.org/):

| Component | Meaning |
|---|---|
| **MAJOR** (`x.0.0`) | Breaking UI/API or simulation behaviour changes |
| **MINOR** (`0.x.0`) | New features, backwards compatible |
| **PATCH** (`0.0.x`) | Bug fixes only |

Pre-releases use a two-suffix convention:

| Suffix | Purpose | Builds triggered? | Docs deployed? |
|---|---|---|---|
| `-beta.N` | In-progress checkpoint — lightweight tag only | ❌ No | ❌ No |
| `-rc.N` | Release candidate — ready for final validation | ✅ Yes (pre-release) | ❌ No |
| *(none)* | Stable release | ✅ Yes (latest) | ✅ Major/minor only |

## PR requirements

Every PR to `dev` — regardless of branch name — must satisfy two checks enforced by the `verify-pr` CI job:

1. **Version bump**: `Cargo.toml` version must be strictly greater than the latest git tag.
2. **Changelog entry**: `CHANGELOG.md` must contain a `## [X.Y.Z]` (or `## vX.Y.Z`) section matching the new version.

These same checks re-run on the post-merge push to `dev` to catch any regressions introduced during merge.

## Cutting a release (developer steps)

### Prerequisites

```bash
cargo install cargo-release   # install once
```

### 1 — Create a branch from dev

```bash
git checkout dev
git pull
git checkout -b feature/my-feature   # or fix/my-fix, chore/my-chore, etc.
```

### 2 — Update CHANGELOG.md

Add a `## [X.Y.Z]` section above `## [Unreleased]` with notes for this version. This is **required** — the `verify-pr` CI job will reject the PR if the entry is missing or does not match the version in `Cargo.toml`.

### 3 — Bump the version with cargo-release

Use the level that matches the change:

```bash
# Beta checkpoint (lightweight — no builds or docs deploy will be triggered)
cargo release beta --execute --no-confirm

# Patch fix
cargo release patch --execute --no-confirm

# New feature
cargo release minor --execute --no-confirm

# Breaking change
cargo release major --execute --no-confirm
```

`cargo-release` will:

1. Run `scripts/verify_release_pr.sh` (pre-release hook) — fails fast if the CHANGELOG entry is missing or the version is not greater than the latest tag.
2. Bump the version in `Cargo.toml` and commit it.

Because `release.toml` sets `tag = false` and `push = false`, **no tag is created or pushed by cargo-release**. The tag is created automatically by CI after the PR merges.

:::note
**The versioned docs snapshot is created automatically after the tag is pushed.**
For major/minor releases, `deploy-docs.yml` snapshots `docs_site/docs/` as a
new versioned set and pushes the resulting commit to `dev`. No manual step is
required.
:::

### 4 — Open a PR to dev

```bash
git push origin feature/my-feature
# Open a PR targeting dev
```

CI runs three jobs in parallel: `verify-pr`, `build` (Linux / Windows / macOS matrix), and `docs-build`. All must be green before merging.

### 5 — Merge the PR

After review and CI is green, merge. The `auto-tag` job fires automatically and pushes `v{version}`. See [CI/CD Workflows — what each tag triggers](./cicd-workflows.md#workflow-reference) for the full breakdown of which workflows fire for each tag type.

## Upgrading a beta to an RC

When a beta is ready for final validation, promote it to an RC:

```bash
cargo release rc --execute --no-confirm
```

This bumps `v0.3.0-beta.2` → `v0.3.0-rc.1`. Open a PR, merge, and the `auto-tag` job will push the RC tag — triggering `release.yml` to build pre-release artifacts.

## GitHub Release description

The `release.yml` workflow automatically extracts the `## [X.Y.Z]` section from `CHANGELOG.md` and uses it as the GitHub Release description. No separate release notes file is needed.

## Artifacts produced

| Platform | Format | Notes |
|---|---|---|
| Linux x86_64 | `.tar.gz` — binary + `assets/` | Extract and run |
| Windows x86_64 | `.msi` — installer | Installs to `Program Files\SSG Tether Capture\` |
| macOS ARM64 | `.dmg` — `.app` bundle | Mount DMG, drag to Applications |

> **macOS Gatekeeper**: The `.app` is ad-hoc signed (no Apple Developer account required). Users must right-click → Open on first launch to bypass the unidentified developer warning. Apple Developer ID signing can be added to the workflow once a certificate is provisioned.
