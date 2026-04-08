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

## [Unreleased]

_Changes staged for the next release go here._

---

## [0.1.1-beta.1] — 2025-04-04

### Changed
- Updated release workflow to build and package artifacts for Linux, Windows
  (MSI via WiX), and macOS (DMG via cargo-bundle) on tag push.
- Pre-release (beta) tags now create a GitHub pre-release; stable tags create
  the production release marked as latest.

---

## [0.1.0]

### Added
- Initial project scaffold: Bevy + Avian3d physics, orbital-mechanics plugin.
- CI: pull-request build matrix (Linux, Windows, macOS) and version-bump check.
- Docusaurus documentation site (`docs_site/`) covering getting started,
  architecture, concepts, API reference, and contributing guides.
- WiX-based Windows MSI installer and macOS DMG packaging.

[Unreleased]: https://github.com/Conjunction-Crew/ssg-tether-capture/compare/v0.1.1-beta.1...HEAD
[0.1.1-beta.1]: https://github.com/Conjunction-Crew/ssg-tether-capture/compare/v0.1.0...v0.1.1-beta.1
[0.1.0]: https://github.com/Conjunction-Crew/ssg-tether-capture/releases/tag/v0.1.0
