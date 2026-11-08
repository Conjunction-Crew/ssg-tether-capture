#!/bin/sh
# Docs drift guard: every Rust-looking identifier in an inline code span of the
# current docs must exist in crates/ (as a word in a .rs file or a Cargo.toml),
# or be listed in scripts/docs_drift_allowlist.txt.
#
# Usage: sh scripts/check_docs_drift.sh [--list]
#   --list  print every checked identifier with its location and status, exit 0
#
# Checked: docs_site/docs/**/*.md(x) except the generated release-notes.md;
# inline code spans only (fenced code blocks are examples and shell and are
# skipped). A span is checked when it looks like a Rust identifier or path
# (`Foo`, `foo_bar`, `FOO_BAR`, `a::B`, `f()`, `Foo<T>`); a lone lowercase word
# without an underscore is prose and is skipped. A path is matched on its last
# segment. Versioned docs are history and are not scanned.
#
# Allowlist format (no wildcards; every entry needs a `# reason`):
#   [external]                 identifiers that are not Rust symbols of this
#   Name # reason              repo (Bevy names, workflow inputs, env vars, ...);
#                              valid on every page
#   [p10-debt]
#   # docs_site/docs/<page>.md stale symbols still in that page, valid only on
#   Name # reason              that page; a rewrite deletes its page's group
# An entry that is no longer needed (found in crates/, or used by no doc on its
# page) fails the check, so the list cannot rot.
#
# Exit 0: no drift. Exit 1: drift, a bad allowlist line or an unused entry.
set -eu

ROOT=$(git rev-parse --show-toplevel)
cd "$ROOT"
ALLOW=scripts/docs_drift_allowlist.txt
MODE=check
case "${1:-}" in
  --list) MODE=list ;;
  "") ;;
  *) echo "usage: sh scripts/check_docs_drift.sh [--list]" >&2; exit 2 ;;
esac
[ -f "$ALLOW" ] || { echo "missing $ALLOW" >&2; exit 1; }

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

# 1. Known words: every identifier-like token in crates/ (Rust sources and manifests).
find crates \( -name '*.rs' -o -name Cargo.toml \) -print0 \
  | xargs -0 grep -ohE '[A-Za-z_][A-Za-z0-9_]*' | sort -u > "$TMP/known"

# 2. Inline code spans of the current docs, outside fenced blocks.
find docs_site/docs \( -name '*.md' -o -name '*.mdx' \) ! -name release-notes.md \
  | LC_ALL=C sort > "$TMP/pages"
while read -r f; do
  awk -v F="$f" '
    /^[[:space:]]*(```|~~~)/ { fence = !fence; next }
    fence { next }
    { line = $0
      while (match(line, /`[^`]+`/)) {
        print F ":" FNR "\t" substr(line, RSTART + 1, RLENGTH - 2)
        line = substr(line, RSTART + RLENGTH)
      } }' "$f"
done < "$TMP/pages" > "$TMP/spans"

# 3. Check the spans against the known words and the allowlist.
awk -F'\t' -v mode="$MODE" -v allow="$ALLOW" '
  function trim(s) { sub(/^[[:space:]]+/, "", s); sub(/[[:space:]]+$/, "", s); return s }
  function lastseg(s,   n, p) { n = split(s, p, "::"); return p[n] }
  function bad(msg) { print allow ":" FNR ": " msg; err = 1 }
  BEGIN { idre = "^[A-Za-z_][A-Za-z0-9_]*(::[A-Za-z_][A-Za-z0-9_]*)*$" }
  FNR == 1 { pass++ }
  pass == 1 { known[$0] = 1; next }
  pass == 2 {
    line = trim($0)
    if (line == "") next
    if (line == "[external]" || line == "[p10-debt]") { section = line; page = ""; next }
    if (line ~ /^#/) {
      c = trim(substr(line, 2))
      if (section == "[p10-debt]" && c ~ /^docs_site\/docs\/.*\.mdx?$/) page = c
      next
    }
    h = index(line, "#")
    if (h == 0 || trim(substr(line, h + 1)) == "") { bad("entry without a `# reason`: " line); next }
    id = trim(substr(line, 1, h - 1))
    if (id !~ idre) { bad("not a single identifier (no wildcards, one per line): " id); next }
    if (section == "") { bad("entry outside [external] / [p10-debt]: " id); next }
    if (section == "[p10-debt]" && page == "") { bad("[p10-debt] entry before a `# docs_site/docs/<page>.md` header: " id); next }
    key = (section == "[external]") ? "*" SUBSEP id : page SUBSEP id
    if (key in entry) { bad("duplicate entry: " id); next }
    entry[key] = FNR; ename[key] = id; epage[key] = (section == "[external]") ? "every page" : page
    next
  }
  {
    loc = $1; s = $2
    sub(/\(\)$/, "", s); sub(/<.*>$/, "", s)
    if (s !~ idre) next
    last = lastseg(s)
    if (s !~ /::/ && last ~ /^[a-z][a-z0-9]*$/) next
    pg = loc; sub(/:[0-9]+$/, "", pg)
    status = "MISSING"
    if (last in known) status = "known"
    else {
      split("", hit)
      if (("*" SUBSEP s) in entry) hit["*" SUBSEP s] = 1
      if (("*" SUBSEP last) in entry) hit["*" SUBSEP last] = 1
      if ((pg SUBSEP s) in entry) hit[pg SUBSEP s] = 1
      if ((pg SUBSEP last) in entry) hit[pg SUBSEP last] = 1
      for (k in hit) { used[k] = 1; status = (substr(k, 1, 1) == "*" && index(k, SUBSEP) == 2) ? "external" : "p10-debt" }
    }
    checked++
    if (mode == "list") { print loc "\t" s "\t" status; next }
    if (status == "MISSING") {
      print loc ": `" s "` is not in crates/ (add it to " allow " [external] with a reason if it is not a Rust symbol)"
      miss++
    }
  }
  END {
    for (k in entry) {
      if (lastseg(ename[k]) in known) {
        print allow ":" entry[k] ": unused allowlist entry `" ename[k] "` (" epage[k] "): it is in crates/; delete it"; err = 1
      } else if (!(k in used)) {
        print allow ":" entry[k] ": unused allowlist entry `" ename[k] "` (" epage[k] "): no doc uses it; delete it"; err = 1
      }
    }
    if (mode == "list") exit 0
    printf "docs drift guard: %d identifiers checked, %d not in crates/\n", checked, miss + 0
    exit (miss > 0 || err) ? 1 : 0
  }' "$TMP/known" "$ALLOW" "$TMP/spans"
