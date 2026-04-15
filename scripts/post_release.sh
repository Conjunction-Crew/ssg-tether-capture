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
#   2. Checks whether this is a Major or Minor release (X.Y.0 with no pre-release
#      suffix). Patch releases and pre-releases skip versioned doc snapshots.
#   3. Runs `npm run docusaurus docs:version <version>` inside docs_site/ to
#      snapshot the current docs as a versioned set.
#   4. Prunes the oldest versioned doc set(s) if the total count exceeds 10,
#      and records dropped versions in the versioned-docs-policy page.
#   5. Commits the generated versioned-docs files so that the release branch PR
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
GITHUB_REPO="conjunction-crew/ssg-tether-capture"
MAX_VERSIONS=10
OLDER_RELEASES_DOC="docs/contributing/versioned-docs-policy.md"

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

# ---------------------------------------------------------------------------
# 2. Only snapshot docs for Major/Minor releases (X.Y.0, no pre-release suffix)
# ---------------------------------------------------------------------------
# A pre-release suffix is anything after a hyphen, e.g. "-beta.1"
VER_CORE="${VER%%-*}"  # strip everything from the first '-' onward
if [ "$VER_CORE" != "$VER" ]; then
  echo "post_release.sh: v$VER is a pre-release — skipping versioned docs snapshot"
  exit 0
fi

# Parse MAJOR.MINOR.PATCH
MAJOR=$(echo "$VER_CORE" | cut -d. -f1)
MINOR=$(echo "$VER_CORE" | cut -d. -f2)
PATCH=$(echo "$VER_CORE" | cut -d. -f3)

if [ "${PATCH:-0}" != "0" ] && [ -n "${PATCH:-}" ]; then
  echo "post_release.sh: v$VER is a patch release (PATCH=$PATCH) — skipping versioned docs snapshot"
  exit 0
fi

echo "post_release.sh: v$VER is a Major/Minor release — creating versioned docs snapshot"

# ---------------------------------------------------------------------------
# 3. Install node deps and snapshot the docs
# ---------------------------------------------------------------------------
if [ ! -d "$DOCS_DIR" ]; then
  echo "post_release.sh: WARNING — $DOCS_DIR not found; skipping docs versioning" >&2
  exit 0
fi

cd "$DOCS_DIR"

# Install dependencies (uses the locked package-lock.json for reproducibility).
npm ci
# Sync docs_site `package.json` version to match crate version (no git changes).
# Try `npm pkg set` (safer), fall back to `npm version --no-git-tag-version` if needed.
npm pkg set version "$VER" || npm version --no-git-tag-version "$VER" || true

# Create the versioned snapshot.  This generates:
#   versioned_docs/version-<VER>/
#   versioned_sidebars/version-<VER>-sidebars.json
#   versions.json  (updated to include <VER>)
npm run docusaurus -- docs:version "$VER"

# ---------------------------------------------------------------------------
# 4a. Prune oldest versions if count exceeds MAX_VERSIONS
# ---------------------------------------------------------------------------
# versions.json is a JSON array ordered newest-first, e.g. ["1.2.0","1.1.0",...]
VERSIONS_JSON="versions.json"
VERSION_COUNT=$(node -e "console.log(require('./$VERSIONS_JSON').length)")

if [ "$VERSION_COUNT" -gt "$MAX_VERSIONS" ]; then
  echo "post_release.sh: $VERSION_COUNT versioned sets found — pruning to $MAX_VERSIONS"

  # Collect the dropped versions (everything beyond index MAX_VERSIONS-1)
  DROPPED_VERSIONS=$(node -e "
    const vers = require('./$VERSIONS_JSON');
    const dropped = vers.slice($MAX_VERSIONS);
    console.log(dropped.join('\n'));
  ")

  # Rewrite versions.json to keep only the first MAX_VERSIONS entries
  node -e "
    const fs = require('fs');
    const vers = require('./$VERSIONS_JSON');
    fs.writeFileSync('$VERSIONS_JSON', JSON.stringify(vers.slice(0, $MAX_VERSIONS), null, 2) + '\n');
  "

  # Remove the pruned versioned doc directories and sidebars
  for DROPPED_VER in $DROPPED_VERSIONS; do
    echo "post_release.sh: removing versioned docs for v$DROPPED_VER"
    rm -rf "versioned_docs/version-$DROPPED_VER"
    rm -f "versioned_sidebars/version-$DROPPED_VER-sidebars.json"
  done
else
  DROPPED_VERSIONS=""
fi

cd ..

# ---------------------------------------------------------------------------
# 4b. Update versioned-docs-policy.md with links for dropped versions
# ---------------------------------------------------------------------------
POLICY_FILE="$DOCS_DIR/$OLDER_RELEASES_DOC"

if [ -n "$DROPPED_VERSIONS" ] && [ -f "$POLICY_FILE" ]; then
  for DROPPED_VER in $DROPPED_VERSIONS; do
    BRANCH="release/v$DROPPED_VER"
    BRANCH_URL="https://github.com/$GITHUB_REPO/tree/$BRANCH/docs_site/docs"

    # Check whether the remote branch exists
    if git ls-remote --exit-code origin "refs/heads/$BRANCH" > /dev/null 2>&1; then
      ENTRY="- [v${DROPPED_VER} docs](${BRANCH_URL})"
      echo "post_release.sh: adding link for v$DROPPED_VER (branch found)"
    else
      ENTRY="- v${DROPPED_VER} *(branch \`${BRANCH}\` not found on remote)*"
      echo "post_release.sh: noting v$DROPPED_VER without link (branch not found)"
    fi

    # Insert the new entry just before the closing OLDER_RELEASES_END marker.
    # If the placeholder "No versioned doc sets" line is still there, replace it first.
    if grep -q "No versioned doc sets have been retired" "$POLICY_FILE"; then
      # Replace the placeholder line with the first real entry
      # Use a temp file for portability (macOS sed -i requires a backup suffix)
      TMPFILE=$(mktemp)
      sed "s|_No versioned doc sets have been retired yet\. Links will appear here automatically as versions age out\._|${ENTRY}|" \
        "$POLICY_FILE" > "$TMPFILE"
      mv "$TMPFILE" "$POLICY_FILE"
    else
      # Append the new entry before the closing marker
      TMPFILE=$(mktemp)
      awk -v entry="$ENTRY" \
        '/\{\/* OLDER_RELEASES_END \*\/\}/ { print entry }
         { print }' \
        "$POLICY_FILE" > "$TMPFILE"
      mv "$TMPFILE" "$POLICY_FILE"
    fi
  done
fi

# ---------------------------------------------------------------------------
# 5. Commit the generated versioned-docs files
# ---------------------------------------------------------------------------
git add \
  "$DOCS_DIR/versioned_docs" \
  "$DOCS_DIR/versioned_sidebars" \
  "$DOCS_DIR/package.json" \
  "$DOCS_DIR/package-lock.json" \
  "$DOCS_DIR/versions.json" || true

# Stage the policy file if it was modified (dropped versions section)
[ -f "$POLICY_FILE" ] && git add "$POLICY_FILE" || true

# Stage any files removed during pruning
git add -u "$DOCS_DIR/versioned_docs" "$DOCS_DIR/versioned_sidebars" 2>/dev/null || true

# Only commit if there are staged changes (first-time versioning will always
# have changes; idempotent re-runs might not).
if git diff --cached --quiet; then
  echo "post_release.sh: no new versioned-docs files to commit (already up to date)"
else
  git commit -m "chore(docs): add versioned docs snapshot for v$VER"
  echo "post_release.sh: committed versioned docs for v$VER"
fi
