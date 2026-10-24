#!/usr/bin/env sh
# changelog_lib.sh — shared helpers for CHANGELOG.md handling.
# Source this file; it defines functions only and has no side effects.

# unreleased_body < CHANGELOG.md
#   Prints the content between "## [Unreleased]" and the next "## [" heading.
unreleased_body() {
  awk '
    /^## \[Unreleased\]/ { in_section = 1; next }
    /^## \[/             { if (in_section) exit }
    in_section           { print }
  '
}

# has_content
#   Reads stdin; exits 0 if it contains any non-blank, non-rule line
#   (a lone "---" separator does not count as content).
has_content() {
  grep -Eqv '^[[:space:]]*(---)?[[:space:]]*$'
}
