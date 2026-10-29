---
sidebar_position: 4
---

# Release Workflow

Releases are cut by **running the Release workflow in GitHub Actions**. Merging a PR to `main` never creates a release, and nobody bumps versions or writes version headings by hand: the workflow does it. For how each workflow is structured, see [CI/CD Workflows](./cicd-workflows.md).

## Version scheme

The project follows [Semantic Versioning](https://semver.org/):

| Component | Meaning |
|---|---|
| **MAJOR** (`x.0.0`) | Breaking UI/API or simulation behaviour changes |
| **MINOR** (`0.x.0`) | New features, backwards compatible |
| **PATCH** (`0.0.x`) | Bug fixes only |

Release candidates use an `-rc.N` suffix:

| Version | Purpose | Builds? | Docs deployed? |
|---|---|---|---|
| `X.Y.Z-rc.N` | Release candidate, ready for final validation | ✅ Yes (marked pre-release) | ✅ Rolling *latest pre-release* version |
| `X.Y.Z` (`Z > 0`) | Patch release | ✅ Yes (marked latest) | ✅ Rolling *latest patch* version |
| `X.Y.0` | Major/minor release | ✅ Yes (marked latest) | ✅ Permanent snapshot |

See [Versioned Docs Policy](./versioned-docs-policy.md) for what the rolling versions are and when they are replaced.

`Cargo.toml` always holds the version of the **last release**. It only changes when the Release workflow runs.

## What contributors do

For every PR:

1. Branch from `main`.
2. Add a bullet for your change under `## [Unreleased]` in `CHANGELOG.md`, in the matching `### Added`, `### Changed`, `### Fixed` or `### Removed` group.
3. Open a PR to `main`. The `ci-success` check must pass before merging.

Do **not** change the version in `Cargo.toml` or add `## [X.Y.Z]` headings.

The `Changelog entry` check fails a PR that does not change the `[Unreleased]` section. Docs-only PRs (only `docs_site/**` or `*.md` files) and Dependabot PRs are exempt. A maintainer can add the `no-changelog` label to exempt any other PR that genuinely needs no entry.

## Cutting a release (maintainers)

Go to **Actions → Release → Run workflow**, select `main`, and choose the inputs:

| Input | Values | Meaning |
|---|---|---|
| `bump` | `patch` / `minor` / `major` | Which component to bump. Ignored when the current version is already an RC |
| `channel` | `stable` / `rc` | `stable` = full release, `rc` = release candidate |
| `dry_run` | checkbox | Compute and validate everything, publish nothing |

What the next version will be (`scripts/next_version.sh`):

| Current version | `bump` | `channel` | Next version |
|---|---|---|---|
| `0.3.0` | `patch` | `stable` | `0.3.1` |
| `0.3.0` | `minor` | `stable` | `0.4.0` |
| `0.3.0` | `minor` | `rc` | `0.4.0-rc.1` |
| `0.4.0-rc.1` | any | `rc` | `0.4.0-rc.2` |
| `0.4.0-rc.2` | any | `stable` | `0.4.0` (promotes the RC) |

Tip: do a **`dry_run`** first. The run summary shows the next version, the release notes that would be published and the commits that would land on `main`. A dry run also checks the release credentials (the `preflight` job) and mints a real GitHub App token, so a missing or wrong App key fails the dry run instead of the real release.

The workflow then:

1. **Verifies** `cargo fmt` and `cargo test` on `main`.
2. **Prepares one release commit** on a scratch branch `release-staging/vX.Y.Z`: bumps `Cargo.toml` and `Cargo.lock`, renames `## [Unreleased]` to `## [vX.Y.Z] - date` (leaving a new empty `[Unreleased]`), and, for stable major/minor releases, snapshots the versioned docs. It also prints which docs versions the release gets (`scripts/docs_versions.sh plan`) to the run summary. The release fails here if `[Unreleased]` is empty.
3. **Builds** the Linux, Windows and macOS packages from that commit.
4. **Publishes**, only if everything above is green: fast-forwards `main` to the release commit, pushes the `vX.Y.Z` tag and creates the GitHub Release. The release description is the changelog section.
5. **Deploys the docs** for every release: a permanent snapshot for `X.Y.0`, the rolling *latest patch* or *latest pre-release* version otherwise. A dry run skips this step but prints the plan.

If any step fails before step 4, `main` is untouched. Fix the problem and run the workflow again. If step 4 or 5 fails part-way, use **Re-run failed jobs**; the publish step is safe to repeat.

## Artifacts produced

| Platform | Format | Notes |
|---|---|---|
| Linux x86_64 | `.tar.gz` — binary + `assets/` | Extract and run |
| Windows x86_64 | `.msi` — installer | Installs to `Program Files\SSG Tether Capture\` |
| macOS ARM64 | `.dmg` — `.app` bundle | Mount DMG, drag to Applications |

> **macOS Gatekeeper**: The `.app` is ad-hoc signed (no Apple Developer account required). Users must right-click → Open on first launch to bypass the unidentified developer warning. Apple Developer ID signing can be added to the workflow once a certificate is provisioned.

To test packaging without releasing anything, run **Actions → Packaging Test** on any branch.

## One-time repository setup

See [CI/CD Workflows → Repository setup](./cicd-workflows.md#repository-setup) for the `main` and tag rulesets, and for the release GitHub App (`RELEASE_APP_ID` variable and `RELEASE_APP_PRIVATE_KEY` secret) that lets the workflow push to protected `main` and create `v*.*.*` tags.

## Hotfixes

Fix on `main` like any other change, then run the Release workflow with `bump: patch`. There are no long-lived release branches.
