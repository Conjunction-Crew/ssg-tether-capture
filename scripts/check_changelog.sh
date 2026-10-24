#!/usr/bin/env sh
# check_changelog.sh BASE_REF
#
# PR check: CHANGELOG.md's "## [Unreleased]" section must have changed compared
# to BASE_REF (any commit-ish, usually the PR base SHA), so every PR records
# what it changed for the next release.
#
# Skipped (exit 0) when:
#   - SKIP_CHANGELOG_CHECK=true  (the 'no-changelog' label, or Dependabot), or
#   - every changed file is documentation-only (docs_site/** or *.md).
#
# Exit codes: 0 pass/skipped, 1 the Unreleased section was not updated.

set -eu

if [ "$#" -ne 1 ]; then
  echo "usage: $0 BASE_REF" >&2
  exit 1
fi

BASE=$1
SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
# shellcheck source=scripts/changelog_lib.sh
. "$SCRIPT_DIR/changelog_lib.sh"

if [ "${SKIP_CHANGELOG_CHECK:-false}" = "true" ]; then
  echo "Changelog check skipped (no-changelog label or automated PR)."
  exit 0
fi

CHANGED=$(git diff --name-only "$BASE"...HEAD)
if [ -z "$CHANGED" ]; then
  echo "No changed files — nothing to check."
  exit 0
fi

NON_DOCS=$(printf '%s\n' "$CHANGED" | grep -Ev '^docs_site/|\.md$' || true)
if [ -z "$NON_DOCS" ]; then
  echo "Documentation-only change — changelog entry not required."
  exit 0
fi

BEFORE=$(git show "$BASE:CHANGELOG.md" 2>/dev/null | unreleased_body || true)
AFTER=$(unreleased_body < CHANGELOG.md)

if ! printf '%s\n' "$AFTER" | has_content; then
  echo "ERROR: CHANGELOG.md has no content under '## [Unreleased]'." >&2
  echo "       Add a bullet describing your change under Added / Changed / Fixed / Removed." >&2
  echo "       (Add the 'no-changelog' label to the PR if it genuinely needs no entry.)" >&2
  exit 1
fi

if [ "$BEFORE" = "$AFTER" ]; then
  echo "ERROR: the '## [Unreleased]' section of CHANGELOG.md was not changed by this PR." >&2
  echo "       Add a bullet describing your change under Added / Changed / Fixed / Removed." >&2
  echo "       (Add the 'no-changelog' label to the PR if it genuinely needs no entry.)" >&2
  exit 1
fi

echo "CHANGELOG.md [Unreleased] section updated — OK"
