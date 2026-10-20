#!/usr/bin/env sh
# prepare_changelog.sh VERSION [DATE]
#
# Promotes the "## [Unreleased]" section of CHANGELOG.md to a released section
# and leaves a fresh, empty "## [Unreleased]" section above it.  Used by the
# Release workflow.
#
#   VERSION  New version without a leading "v", e.g. 0.4.0 or 0.4.0-rc.1
#   DATE     Release date (default: today, UTC), YYYY-MM-DD
#
# The new heading follows the existing style:
#   ## [v0.4.0] - 2026-10-20          (stable)
#   ## [v0.4.0-rc.1] 🔬 - 2026-10-20  (release candidate)
#
# Fails if there is no "## [Unreleased]" heading or it has no content.

set -eu

if [ "$#" -lt 1 ] || [ "$#" -gt 2 ]; then
  echo "usage: $0 VERSION [DATE]" >&2
  exit 1
fi

VERSION=$1
DATE=${2:-$(date -u +%Y-%m-%d)}
FILE=${CHANGELOG_FILE:-CHANGELOG.md}

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
# shellcheck source=scripts/changelog_lib.sh
. "$SCRIPT_DIR/changelog_lib.sh"

if ! grep -q '^## \[Unreleased\]' "$FILE"; then
  echo "ERROR: $FILE has no '## [Unreleased]' section." >&2
  exit 1
fi

if ! unreleased_body < "$FILE" | has_content; then
  echo "ERROR: the '## [Unreleased]' section of $FILE is empty — nothing to release." >&2
  exit 1
fi

case "$VERSION" in
  *-rc.*) HEADING="## [v${VERSION}] 🔬 - ${DATE}" ;;
  *)      HEADING="## [v${VERSION}] - ${DATE}" ;;
esac

TMP=$(mktemp)
awk -v heading="$HEADING" '
  /^## \[Unreleased\]/ && !done {
    print "## [Unreleased]"
    print ""
    print ""
    print heading
    done = 1
    next
  }
  { print }
' "$FILE" > "$TMP"
mv "$TMP" "$FILE"

echo "Promoted [Unreleased] → ${HEADING#\#\# }"
