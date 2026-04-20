#!/usr/bin/env sh
# prune_versioned_docs.sh
#
# Enforces the 10-versioned-docs cap by removing the oldest versioned doc
# set(s) from docs_site/ whenever the count exceeds MAX_VERSIONS.  Also
# records links to the dropped versions in versioned-docs-policy.md.
#
# Called from deploy-docs.yml after post_release.sh has created the new
# snapshot.  Can also be run manually if needed (safe to run at any time).
#
# Run from the repo root.

set -eu

DOCS_DIR="docs_site"
GITHUB_REPO="conjunction-crew/ssg-tether-capture"
MAX_VERSIONS=10
POLICY_FILE="$DOCS_DIR/docs/contributing/versioned-docs-policy.md"
VERSIONS_JSON="$DOCS_DIR/versions.json"

# ---------------------------------------------------------------------------
# 1. Check whether pruning is needed
# ---------------------------------------------------------------------------
VERSION_COUNT=$(node -e "console.log(require('./$VERSIONS_JSON').length)")

if [ "$VERSION_COUNT" -le "$MAX_VERSIONS" ]; then
  echo "prune_versioned_docs.sh: $VERSION_COUNT versioned sets — no pruning needed"
  exit 0
fi

echo "prune_versioned_docs.sh: $VERSION_COUNT versioned sets found — pruning to $MAX_VERSIONS"

# ---------------------------------------------------------------------------
# 2. Collect the versions to drop (everything beyond index MAX_VERSIONS-1)
# ---------------------------------------------------------------------------
DROPPED_VERSIONS=$(node -e "
  const vers = require('./$VERSIONS_JSON');
  const dropped = vers.slice($MAX_VERSIONS);
  console.log(dropped.join('\n'));
")

# ---------------------------------------------------------------------------
# 3. Rewrite versions.json to keep only the first MAX_VERSIONS entries
# ---------------------------------------------------------------------------
node -e "
  const fs = require('fs');
  const vers = require('./$VERSIONS_JSON');
  fs.writeFileSync('$VERSIONS_JSON', JSON.stringify(vers.slice(0, $MAX_VERSIONS), null, 2) + '\n');
"

# ---------------------------------------------------------------------------
# 4. Remove the pruned versioned doc directories and sidebars
# ---------------------------------------------------------------------------
for DROPPED_VER in $DROPPED_VERSIONS; do
  echo "prune_versioned_docs.sh: removing versioned docs for v$DROPPED_VER"
  rm -rf "$DOCS_DIR/versioned_docs/version-$DROPPED_VER"
  rm -f  "$DOCS_DIR/versioned_sidebars/version-$DROPPED_VER-sidebars.json"
done

# ---------------------------------------------------------------------------
# 5. Record dropped versions in versioned-docs-policy.md
# ---------------------------------------------------------------------------
if [ -f "$POLICY_FILE" ]; then
  for DROPPED_VER in $DROPPED_VERSIONS; do
    BRANCH="release/v$DROPPED_VER"
    BRANCH_URL="https://github.com/$GITHUB_REPO/tree/$BRANCH/docs_site/docs"

    # Check whether the remote branch exists
    if git ls-remote --exit-code origin "refs/heads/$BRANCH" > /dev/null 2>&1; then
      ENTRY="- [v${DROPPED_VER} docs](${BRANCH_URL})"
      echo "prune_versioned_docs.sh: adding link for v$DROPPED_VER (branch found)"
    else
      ENTRY="- v${DROPPED_VER} *(branch \`${BRANCH}\` not found on remote)*"
      echo "prune_versioned_docs.sh: noting v$DROPPED_VER without link (branch not found)"
    fi

    # Insert the new entry just before the closing OLDER_RELEASES_END marker.
    # If the placeholder line is still present, replace it with the first real entry.
    if grep -q "No versioned doc sets have been retired" "$POLICY_FILE"; then
      TMPFILE=$(mktemp)
      sed "s|_No versioned doc sets have been retired yet\. Links will appear here automatically as versions age out\._|${ENTRY}|" \
        "$POLICY_FILE" > "$TMPFILE"
      mv "$TMPFILE" "$POLICY_FILE"
    else
      TMPFILE=$(mktemp)
      awk -v entry="$ENTRY" \
        '/\{\/\* OLDER_RELEASES_END \*\/\}/ { print entry }
         { print }' \
        "$POLICY_FILE" > "$TMPFILE"
      mv "$TMPFILE" "$POLICY_FILE"
    fi
  done
fi

# ---------------------------------------------------------------------------
# 6. Commit all pruning changes
# ---------------------------------------------------------------------------
git add \
  "$DOCS_DIR/versions.json" \
  "$DOCS_DIR/versioned_docs" \
  "$DOCS_DIR/versioned_sidebars" || true

[ -f "$POLICY_FILE" ] && git add "$POLICY_FILE" || true

# Stage deletions of the removed directories
git add -u "$DOCS_DIR/versioned_docs" "$DOCS_DIR/versioned_sidebars" 2>/dev/null || true

if git diff --cached --quiet; then
  echo "prune_versioned_docs.sh: nothing to commit (no pruning was performed)"
else
  git commit -m "chore(docs): prune versioned docs to ${MAX_VERSIONS}-version cap"
  echo "prune_versioned_docs.sh: committed versioned docs pruning"
fi
