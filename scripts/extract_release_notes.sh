#!/usr/bin/env sh
# extract_release_notes.sh VERSION
#
# Prints the CHANGELOG.md section for VERSION (without its heading) to stdout.
# Accepts VERSION with or without a leading "v".  Matches headings written as
# "## [0.4.0]", "## [v0.4.0]" or "## [v0.4.0] 🔬 - 2026-10-20".
#
# Exits 1 if the section is missing or empty.

set -eu

if [ "$#" -ne 1 ]; then
  echo "usage: $0 VERSION" >&2
  exit 1
fi

VERSION=${1#v}
FILE=${CHANGELOG_FILE:-CHANGELOG.md}

BODY=$(awk -v ver="$VERSION" '
  /^## \[/ {
    if (found) exit
    if (index($0, "## [" ver "]") == 1 || index($0, "## [v" ver "]") == 1) { found = 1; next }
  }
  found { print }
' "$FILE")

# Trim leading/trailing blank lines and a trailing "---" rule.
BODY=$(printf '%s\n' "$BODY" | awk '
  { lines[NR] = $0 }
  END {
    s = 1; e = NR
    while (s <= e && lines[s] ~ /^[[:space:]]*$/) s++
    while (e >= s && (lines[e] ~ /^[[:space:]]*$/ || lines[e] ~ /^---[[:space:]]*$/)) e--
    for (i = s; i <= e; i++) print lines[i]
  }
')

if [ -z "$BODY" ]; then
  echo "ERROR: no CHANGELOG.md section found for version $VERSION" >&2
  exit 1
fi

printf '%s\n' "$BODY"
