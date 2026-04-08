#!/usr/bin/env sh
# post_release.sh
#
# cargo-release post-release hook.  Runs after cargo-release has bumped the
# version in Cargo.toml and created the "Release X.Y.Z" commit (but before
# any tag or push, which are both disabled in .release.toml).
#
# What this script does:
#   1. Resolves the new version (from the VERSION env var set by cargo-release,
#      or falls back to parsing Cargo.toml directly).
#   2. Runs `npm run docusaurus docs:version <version>` inside docs_site/ to
#      snapshot the current docs as a versioned set.
#   3. Commits the generated versioned-docs files so that the release branch PR
#      contains the docs snapshot for reviewers to inspect.
#
# The commit produced here is an extra commit on top of the cargo-release
# version-bump commit, both of which land in the release branch and are
# reviewed as part of the release PR before any tag is pushed.
#
# Environment variables set by cargo-release:
#   VERSION          — new version string (e.g. "0.2.0")
#   PREV_VERSION     — previous version string
#   CRATE_NAME       — crate name

set -eu

DOCS_DIR="docs_site"

# ---------------------------------------------------------------------------
# 1. Determine the new version
# ---------------------------------------------------------------------------
if [ -n "${VERSION:-}" ]; then
  VER="$VERSION"
else
  VER=$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -n1)
fi

if [ -z "$VER" ]; then
  echo "post_release.sh: ERROR — could not determine new version" >&2
  exit 1
fi

echo "post_release.sh: creating versioned docs for v$VER"

# ---------------------------------------------------------------------------
# 2. Install node deps and snapshot the docs
# ---------------------------------------------------------------------------
if [ ! -d "$DOCS_DIR" ]; then
  echo "post_release.sh: WARNING — $DOCS_DIR not found; skipping docs versioning" >&2
  exit 0
fi

cd "$DOCS_DIR"

# Install dependencies (uses the locked package-lock.json for reproducibility).
npm ci

# Create the versioned snapshot.  This generates:
#   versioned_docs/version-<VER>/
#   versioned_sidebars/version-<VER>-sidebars.json
#   versions.json  (updated to include <VER>)
npm run docusaurus -- docs:version "$VER"

cd ..

# ---------------------------------------------------------------------------
# 3. Commit the generated versioned-docs files
# ---------------------------------------------------------------------------
git add \
  "$DOCS_DIR/versioned_docs" \
  "$DOCS_DIR/versioned_sidebars" \
  "$DOCS_DIR/versions.json" || true

# Only commit if there are staged changes (first-time versioning will always
# have changes; idempotent re-runs might not).
if git diff --cached --quiet; then
  echo "post_release.sh: no new versioned-docs files to commit (already up to date)"
else
  git commit -m "chore(docs): add versioned docs snapshot for v$VER"
  echo "post_release.sh: committed versioned docs for v$VER"
fi
