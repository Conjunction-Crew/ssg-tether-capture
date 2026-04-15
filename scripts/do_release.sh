#!/usr/bin/env sh
# do_release.sh
#
# Wrapper around cargo-release that also runs post_release.sh automatically.
#
# cargo-release has no post-release-hook, so this script bridges that gap.
#
# Usage:
#   ./scripts/do_release.sh <level>
#
#   <level> is any cargo-release bump level: release, patch, minor, major,
#           alpha, beta, rc, or an explicit version string (e.g. "1.2.0").
#
# Examples:
#   ./scripts/do_release.sh release   # promote 0.2.0-beta.6 -> 0.2.0
#   ./scripts/do_release.sh beta      # bump beta pre-release
#   ./scripts/do_release.sh minor     # bump minor version

set -eu

LEVEL="${1:-release}"

# ---------------------------------------------------------------------------
# 1. Run cargo-release (version bump + pre-release-hook + commit)
# ---------------------------------------------------------------------------
cargo release "$LEVEL" --execute

# ---------------------------------------------------------------------------
# 2. Extract the new version from Cargo.toml (cargo-release has just written it)
# ---------------------------------------------------------------------------
NEW_VER=$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -n1)
if [ -z "$NEW_VER" ]; then
  echo "do_release.sh: ERROR — could not read new version from Cargo.toml" >&2
  exit 1
fi

echo ""
echo "do_release.sh: cargo-release complete (v$NEW_VER). Running post_release.sh..."

# ---------------------------------------------------------------------------
# 3. Run post_release.sh (versioned docs — skips betas and patches internally)
# ---------------------------------------------------------------------------
VERSION="$NEW_VER" ./scripts/post_release.sh
