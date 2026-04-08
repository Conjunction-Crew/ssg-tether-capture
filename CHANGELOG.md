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

## [0.2.0-beta.1] - 2026-04-08

### Changed
- Adopts `cargo-release` for automated versioning and changelog management.
- Updates `deploy-docs.yml` to build and deploy the docs on pushes to main 
  or on version tag push
- Updates `pr.yaml` to
  - Support checks for `release/v*.*.*` and `release/v*.*.*-beta.*` branches
  - Remove version checking from this workflow
- Update the `contributing` docs section to explain release process.
- Updates `docusaurus` version from 3.9.2 → 3.10.0
  - Updates other NPM packages as required to update docusaurus.

### Added
- `.release.toml` configuration file for `cargo-release`.
- Adds scripts to manage pre and post release tasks.
  - `scripts/pre_release.sh` and `scripts/post_release.sh` for managing pre and post release tasks.
  - `scripts/post_release.sh` also keeps the docs site version in sync with the crate version.
- Adds `CHANGELOG.md` document to track changes between releases.
- Adds versioned docs for `v0.2.0-beta.1`

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
