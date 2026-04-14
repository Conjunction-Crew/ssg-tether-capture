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
#   Called automatically by cargo-release as pre-release-hook (see .release.toml).
#   Also called by the GitHub Actions verify-release-pr job in pr.yaml for any
#   PR whose source branch matches release/*.
#
# Exit codes:
#   0  — all checks passed
#   1  — one or more checks failed (message printed to stderr)

set -eu

# ---------------------------------------------------------------------------
# 1. Determine the new version
#    When invoked as a cargo-release pre-release-hook, cargo-release has not
#    yet written the bumped version to Cargo.toml, but it does export VERSION
#    with the new value.  Prefer that env var; fall back to Cargo.toml for
#    standalone / CI invocations (where the bump has already been committed).
# ---------------------------------------------------------------------------
if [ -n "${NEW_VERSION:-}" ]; then
  NEW_VER="$NEW_VERSION"
else
  NEW_VER=$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -n1)
fi
if [ -z "$NEW_VER" ]; then
  echo "ERROR: Could not determine new version (NEW_VERSION env var not set and Cargo.toml parse failed)" >&2
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

  # Strip pre-release suffix (everything from the first '-' onward) to get
  # the semver core, e.g. "0.2.0-beta.6" -> "0.2.0".
  NEW_CORE="${NEW_VER%%-*}"
  LATEST_CORE="${LATEST_VER%%-*}"

  if [ "$NEW_VER" = "$LATEST_VER" ]; then
    echo "ERROR: New version ($NEW_VER) must be greater than latest tag ($LATEST_VER) — did you forget to bump?" >&2
    exit 1
  elif [ "$NEW_CORE" = "$LATEST_CORE" ]; then
    # Same X.Y.Z core: per semver, a release (no suffix) is GREATER than any
    # pre-release of the same core.  sort -V gets this wrong, so handle it here.
    NEW_HAS_PRE=0; [ "$NEW_VER" != "$NEW_CORE" ] && NEW_HAS_PRE=1
    LATEST_HAS_PRE=0; [ "$LATEST_VER" != "$LATEST_CORE" ] && LATEST_HAS_PRE=1
    if [ "$NEW_HAS_PRE" -eq 1 ] && [ "$LATEST_HAS_PRE" -eq 0 ]; then
      echo "ERROR: New version ($NEW_VER) is a pre-release but latest tag ($LATEST_VER) is already a release" >&2
      exit 1
    fi
    # new=release, latest=pre-release → new is newer: OK
    # new=pre-release, latest=pre-release, different suffix → fall through to sort -V
    if [ "$NEW_HAS_PRE" -eq 1 ] && [ "$LATEST_HAS_PRE" -eq 1 ]; then
      LOWER=$(printf "%s\n%s" "$LATEST_VER" "$NEW_VER" | sort -V | head -n1)
      if [ "$LOWER" = "$NEW_VER" ]; then
        echo "ERROR: New version ($NEW_VER) is less than latest tag ($LATEST_VER)" >&2
        exit 1
      fi
    fi
  else
    # Different cores: compare only the X.Y.Z parts with sort -V to avoid
    # pre-release suffix confusion.
    LOWER=$(printf "%s\n%s" "$LATEST_CORE" "$NEW_CORE" | sort -V | head -n1)
    if [ "$LOWER" = "$NEW_CORE" ]; then
      echo "ERROR: New version ($NEW_VER) is less than latest tag ($LATEST_VER)" >&2
      exit 1
    fi
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
