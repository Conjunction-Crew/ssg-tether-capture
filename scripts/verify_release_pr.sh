#!/usr/bin/env sh
# verify_release_pr.sh
#
# Verifies that a release branch/PR is ready to be reviewed and eventually
# tagged.  Checks two things:
#
#   1. The version in Cargo.toml is strictly greater than the latest git tag.
#   2. CHANGELOG.md contains an entry for the new version.
#
# Usage:
#   Called automatically by cargo-release as pre-release-hook (see release.toml).
#   Also called by the GitHub Actions verify-pr job in pr.yaml for every PR and
#   post-merge push to dev.
#
# Exit codes:
#   0  — all checks passed
#   1  — one or more checks failed (message printed to stderr)

set -eu

# ---------------------------------------------------------------------------
# 1. Extract new version from Cargo.toml
# ---------------------------------------------------------------------------
NEW_VER=$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -n1)
if [ -z "$NEW_VER" ]; then
  echo "ERROR: Could not extract version from Cargo.toml" >&2
  exit 1
fi
echo "Cargo.toml version: $NEW_VER"

# ---------------------------------------------------------------------------
# 2. Find the latest semver-style git tag (v0.0.0 or v0.0.0-beta.x)
# ---------------------------------------------------------------------------
# Fetch tags in case we are in a shallow clone (CI environment).
git fetch --tags --quiet 2>/dev/null || true

LATEST_TAG=$(git tag --sort=-v:refname | grep -E '^v[0-9]+\.[0-9]+\.[0-9]+' | head -n1 || true)
LATEST_VER=${LATEST_TAG#v}

if [ -z "$LATEST_VER" ]; then
  echo "No existing tags found; treating as first release — skipping version-comparison check."
else
  echo "Latest tag: $LATEST_TAG (version $LATEST_VER)"

  LOWER=$(printf "%s\n%s" "$LATEST_VER" "$NEW_VER" | sort -V | head -n1)
  if [ "$LOWER" = "$NEW_VER" ]; then
    # $NEW_VER sorts first, meaning it is less than or equal to $LATEST_VER.
    if [ "$NEW_VER" = "$LATEST_VER" ]; then
      echo "ERROR: New version ($NEW_VER) must be greater than latest tag ($LATEST_VER) — did you forget to bump?" >&2
    else
      echo "ERROR: New version ($NEW_VER) is less than latest tag ($LATEST_VER)" >&2
    fi
    exit 1
  fi
  echo "Version comparison OK ($LATEST_VER → $NEW_VER)"
fi

# ---------------------------------------------------------------------------
# 3. Verify CHANGELOG.md contains an entry for the new version
# ---------------------------------------------------------------------------
if [ ! -f CHANGELOG.md ]; then
  echo "ERROR: CHANGELOG.md not found. Add a CHANGELOG.md with a section for v$NEW_VER." >&2
  exit 1
fi

if ! grep -qE "^## \[?v?${NEW_VER}\]?" CHANGELOG.md; then
  echo "ERROR: CHANGELOG.md does not contain an entry for version $NEW_VER." >&2
  echo "       Add a '## [${NEW_VER}]' (or '## v${NEW_VER}') section with release notes." >&2
  exit 1
fi

echo "CHANGELOG.md entry for $NEW_VER found — OK"
echo ""
echo "All release-PR checks passed."
