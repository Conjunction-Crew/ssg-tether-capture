---
sidebar_position: 2
---

# CI/CD Workflows

This page is a reference for all GitHub Actions workflows in the project. For the
step-by-step process of cutting a release, see [Release Workflow](./release-workflow.md).

## Overview

```
PR / push to dev
      │
      ├─ verify-pr        ← version bump + CHANGELOG entry check (all PRs)
      ├─ build            ← cargo build + test (Linux / Windows / macOS)
      ├─ docs-build       ← Docusaurus build + versioned-docs count check
      │
      └─ auto-tag (post-merge only)
               │
               └─ pushes vX.Y.Z tag via GH_PAT
                        │
                        ├─ release.yml    ← stable or RC tags only
                        └─ deploy-docs.yml ← stable major/minor tags only
```

## Workflow reference

### `pr.yaml` — PR checks and auto-tagging

**Triggers**: push to `dev`, pull request targeting `dev`

| Job | Runs on | Purpose |
|---|---|---|
| `verify-pr` | ubuntu-latest | Checks `Cargo.toml` version > latest tag; checks `CHANGELOG.md` has a matching entry |
| `build` | ubuntu / windows / macos | `cargo build` + `cargo test` full matrix; needs `verify-pr` |
| `docs-build` | ubuntu-latest | `npm run build` of the Docusaurus site; syncs `CHANGELOG.md` → `docs/release-notes.md`; fails if `versions.json` has > 10 entries; needs `verify-pr` |
| `auto-tag` | ubuntu-latest | Fires only on post-merge pushes to `dev`; reads version from `Cargo.toml`, skips if tag already exists, pushes `v{version}` using `GH_PAT`; needs `build` + `docs-build` |

The `verify-pr` check runs unconditionally on **all** PRs to `dev` — there is no special branch-name convention required to trigger version or changelog validation.

---

### `release.yml` — Cross-platform builds and GitHub Release

**Triggers**: push of tags matching `v[0-9]+.[0-9]+.[0-9]+` or `v[0-9]+.[0-9]+.[0-9]+-rc*`

Beta tags (`-beta.N`) do **not** trigger this workflow.

| Job | Runs on | Purpose |
|---|---|---|
| `preflight` | ubuntu-latest | Reads `github.ref_name`, sets `is_prerelease` (`-rc.*` → true) and `make_latest` (stable → true) |
| `build-linux` | ubuntu-latest | `cargo build --release` → `.tar.gz` (binary + `assets/`) |
| `build-windows` | windows-latest | `cargo build --release` → WiX MSI (pre-release suffix stripped from MSI `PackageVersion`) |
| `build-macos` | macos-latest | `cargo bundle --release` → ad-hoc signed `.app` → `.dmg` |
| `release` | ubuntu-latest | Downloads all artifacts; extracts matching `CHANGELOG.md` section as release body; publishes GitHub Release via `ncipollo/release-action` |

The GitHub Release is marked as **pre-release** for RC tags and **latest** for stable tags.

---

### `deploy-docs.yml` — Docusaurus deployment to GitHub Pages

**Triggers**: push of tags matching `v[0-9]+.[0-9]+.[0-9]+` (stable only — no RC or beta)

| Job | Runs on | Purpose |
|---|---|---|
| `build` | ubuntu-latest | Installs deps, syncs `CHANGELOG.md` into `docs/release-notes.md` and the matching versioned snapshot dir, runs the release-type check, builds the site, conditionally uploads the pages artifact |
| `deploy` | ubuntu-latest | Deploys the artifact to GitHub Pages; only runs when `build` outputs `should-deploy=true` |

**Release-type logic** in the `build` job:

- **Major/minor** (`PATCH = 0`): requires a `versioned_docs/version-{VERSION}/` snapshot; verifies it matches `docs/` exactly (excluding `release-notes.md`); verifies `versions.json` count ≤ 10; sets `should-deploy=true`.
- **Patch** (`PATCH ≠ 0`): no snapshot required; builds the site for validation but sets `should-deploy=false` — the live site is left unchanged since no new versioned entry was added.

---

### `packaging-test.yml` — Manual packaging smoke test

**Triggers**: `workflow_dispatch` only (manual)

Runs all three platform release builds (Linux tar.gz, Windows MSI, macOS DMG) without publishing a GitHub Release. Use this to validate packaging changes before a real release.

---

## Branch and tag protection

Both protections are configured as GitHub **Rulesets** (Settings → Rules → Rulesets) and must be set up manually — they are not defined in workflow files.

### Branch Ruleset — `dev`

| Setting | Value |
|---|---|
| Pattern | `dev` |
| Require a pull request before merging | ✅ |
| Required status checks | `verify-pr`, `build (ubuntu-latest)`, `build (windows-latest)`, `build (macos-latest)`, `Build docs site` |
| Prevent force pushes | ✅ |
| Prevent deletion | ✅ |

### Tag Ruleset — version tags

| Setting | Value |
|---|---|
| Pattern | `v*.*.*` |
| Bypass list | Service account that owns `GH_PAT` only |
| Enforcement | Active |

This blocks all developers from pushing version tags directly. The only source of a `v*.*.*` tag is the `auto-tag` job, ensuring the full PR + CI pipeline is always exercised before a tag exists.

## Secrets

| Secret | Used by | Purpose |
|---|---|---|
| `GH_PAT` | `pr.yaml` `auto-tag` job | A Personal Access Token (contents: write) belonging to the service account in the Tag Ruleset bypass list. Using a PAT (rather than `GITHUB_TOKEN`) allows the pushed tag to trigger downstream workflow runs. |

Add this secret under repo **Settings → Secrets and variables → Actions**.
