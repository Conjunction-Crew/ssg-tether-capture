#!/usr/bin/env sh
# docs_versions.sh — which docs versions the site gets for a release (Q48).
#
#   sh scripts/docs_versions.sh plan <version> [summary-file]
#       Print the plan: permanent snapshots (docs_site/versions.json), the two
#       rolling versions (latest patch, latest pre-release) and the default
#       version. Needs only git and the repo; runs locally and in CI.
#   sh scripts/docs_versions.sh apply <version>
#       CI only (needs node and `npm ci` in docs_site): snapshot the rolling
#       versions into the working tree (never committed), sort versions.json,
#       write docs_site/rolling-versions.json for docusaurus.config.ts.
#
# Rules (P6, Q48):
#   * Releases considered: every tag vX.Y.Z or vX.Y.Z-<pre>.<n> at or below
#     <version>, plus <version> itself (it may not be tagged yet).
#   * Permanent: the entries of docs_site/versions.json (stable X.Y.0 only,
#     capped at 10 by scripts/prune_versioned_docs.sh). A stable X.Y.0 release
#     not yet in it is its own snapshot (the release commit adds it).
#   * Latest patch: the newest stable X.Y.Z with Z > 0. It is replaced by the
#     next patch release and kept when a newer minor ships.
#   * Latest pre-release: the newest X.Y.Z-<pre>.<n> whose X.Y.Z is newer than
#     every stable release considered; dropped once X.Y.Z itself is released.
#   * Default: the newest stable docs version (permanent or latest patch).
#   * <version> must end up with a docs version, or the script exits 1.
set -eu

MODE=${1:-}
RELEASE=${2:-}
SUMMARY=${3:-}
VERSIONS_JSON=docs_site/versions.json

usage() { echo "usage: sh scripts/docs_versions.sh plan|apply <version> [summary-file]" >&2; exit 2; }
[ -n "$MODE" ] && [ -n "$RELEASE" ] || usage
case "$MODE" in plan|apply) ;; *) usage ;; esac

# Sort key: X.Y.Z → 000X.000Y.000Z.~ ; X.Y.Z-pre.N → 000X.000Y.000Z.pre.0000N.
# In the C locale "~" sorts after every lower-case pre-release label.
key() {
  echo "$1" | awk -F'[.-]' '{
    if (NF == 3) printf "%06d.%06d.%06d.~\n", $1, $2, $3;
    else         printf "%06d.%06d.%06d.%s.%06d\n", $1, $2, $3, $4, $5 }'
}
valid() { echo "$1" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+(-[a-z]+\.[0-9]+)?$'; }
is_stable() { case "$1" in *-*) return 1 ;; *) return 0 ;; esac; }
base() { echo "${1%%-*}"; }

valid "$RELEASE" || { echo "docs_versions.sh: '$RELEASE' is not X.Y.Z or X.Y.Z-pre.N" >&2; exit 2; }
RKEY=$(key "$RELEASE")

# Releases considered, one "key version" line each, sorted ascending.
CONSIDERED=$( { git tag -l 'v*' | sed 's/^v//'; echo "$RELEASE"; } | sort -u | while read -r v; do
  valid "$v" || continue
  k=$(key "$v")
  [ "$(printf '%s\n%s\n' "$k" "$RKEY" | LC_ALL=C sort | tail -n1)" = "$RKEY" ] && echo "$k $v"
done | LC_ALL=C sort)

PERMANENT=$(sed -n 's/^[[:space:]]*"\([^"]*\)".*/\1/p' "$VERSIONS_JSON")
if is_stable "$RELEASE" && [ "${RELEASE##*.}" = "0" ] && ! echo "$PERMANENT" | grep -qx "$RELEASE"; then
  PERMANENT=$(printf '%s\n%s\n' "$RELEASE" "$PERMANENT")
  NEW_SNAPSHOT=$RELEASE
else
  NEW_SNAPSHOT=
fi

PATCH=$(echo "$CONSIDERED" | while read -r k v; do
  is_stable "$v" && [ "${v##*.}" != "0" ] && echo "$v"; done | tail -n1)
NEWEST_STABLE=$(echo "$CONSIDERED" | while read -r k v; do is_stable "$v" && echo "$v"; done | tail -n1)
PRERELEASE=$(echo "$CONSIDERED" | while read -r k v; do is_stable "$v" || echo "$v"; done | tail -n1)
if [ -n "$PRERELEASE" ] && [ -n "$NEWEST_STABLE" ]; then
  bk=$(key "$(base "$PRERELEASE")"); sk=$(key "$NEWEST_STABLE")
  [ "$(printf '%s\n%s\n' "$bk" "$sk" | LC_ALL=C sort | tail -n1)" = "$bk" ] && [ "$bk" != "$sk" ] || PRERELEASE=
fi

DEFAULT=$( { echo "$PERMANENT"; [ -n "$PATCH" ] && echo "$PATCH"; } | while read -r v; do
  [ -n "$v" ] && echo "$(key "$v") $v"; done | LC_ALL=C sort | tail -n1 | cut -d' ' -f2)

covered=no
for v in $PERMANENT $PATCH $PRERELEASE; do [ "$v" = "$RELEASE" ] && covered=yes; done

tag_of() { if [ "$1" = "$RELEASE" ]; then echo "working tree"; else echo "v$1"; fi; }
{
  echo "| Docs version | Kind | Built from |"
  echo "|---|---|---|"
  [ -n "$PRERELEASE" ] && echo "| $PRERELEASE | rolling: latest pre-release | $(tag_of "$PRERELEASE") |"
  [ -n "$PATCH" ] && echo "| $PATCH | rolling: latest patch | $(tag_of "$PATCH") |"
  for v in $PERMANENT; do
    if [ "$v" = "$NEW_SNAPSHOT" ]; then echo "| $v | permanent (snapshot added by this release) | working tree |"
    else echo "| $v | permanent | versions.json |"; fi
  done
  echo ""
  echo "Release: \`$RELEASE\`. Default version: \`$DEFAULT\`. Release covered: $covered."
} > "${TMPDIR:-/tmp}/docs_versions.$$"
cat "${TMPDIR:-/tmp}/docs_versions.$$"
[ -n "$SUMMARY" ] && cat "${TMPDIR:-/tmp}/docs_versions.$$" >> "$SUMMARY"
rm -f "${TMPDIR:-/tmp}/docs_versions.$$"

[ "$covered" = yes ] || { echo "docs_versions.sh: no docs version for $RELEASE" >&2; exit 1; }
[ "$MODE" = plan ] && exit 0

# ── apply (CI) ────────────────────────────────────────────────────────────────
# For each rolling version V (latest patch, latest pre-release): snapshot the
# docs of V into docs_site/versioned_docs (never committed) and add a
# release-notes.md copy of CHANGELOG.md to the snapshot.
# V = RELEASE snapshots the working tree; any other V is read from its tag.
# Then versions.json is rewritten newest first and
# docs_site/rolling-versions.json tells docusaurus.config.ts which entries are
# rolling and which one is the default.
[ ! -e docs_site/rolling-versions.json ] \
  || { echo "docs_versions.sh: apply already ran in this checkout" >&2; exit 1; }

snapshot() {
  v=$1
  if [ -d "docs_site/versioned_docs/version-$v" ]; then
    echo "docs_versions.sh: docs_site/versioned_docs/version-$v exists; rolling snapshots are never committed" >&2
    exit 1
  fi
  if [ "$v" = "$RELEASE" ]; then
    (cd docs_site && npx docusaurus docs:version "$v")
  else
    git rev-parse -q --verify "refs/tags/v$v" > /dev/null \
      || { echo "docs_versions.sh: tag v$v not found (fetch-depth: 0 needed)" >&2; exit 1; }
    aside=$(mktemp -d)
    mv docs_site/docs "$aside/docs"
    git archive "v$v" docs_site/docs | tar -x
    # Restore the working tree's docs even when the snapshot fails.
    if (cd docs_site && npx docusaurus docs:version "$v"); then ok=yes; else ok=no; fi
    rm -rf docs_site/docs
    mv "$aside/docs" docs_site/docs
    rmdir "$aside"
    [ "$ok" = yes ] || { echo "docs_versions.sh: docs:version $v failed" >&2; exit 1; }
  fi
  cp CHANGELOG.md "docs_site/versioned_docs/version-$v/release-notes.md"
}

[ -n "$PATCH" ] && snapshot "$PATCH"
[ -n "$PRERELEASE" ] && snapshot "$PRERELEASE"

# versions.json newest first, by semver (not by position).
SORTED=$(sed -n 's/^[[:space:]]*"\([^"]*\)".*/\1/p' "$VERSIONS_JSON" | while read -r v; do
  [ -n "$v" ] && echo "$(key "$v") $v"; done | LC_ALL=C sort -r | cut -d' ' -f2)
{
  echo "["
  echo "$SORTED" | sed 's/.*/  "&"/' | sed '$!s/$/,/'
  echo "]"
} > "$VERSIONS_JSON.new"
mv "$VERSIONS_JSON.new" "$VERSIONS_JSON"

json_or_null() { if [ -n "$1" ]; then printf '"%s"' "$1"; else printf 'null'; fi; }
printf '{ "patch": %s, "prerelease": %s, "default": "%s" }\n' \
  "$(json_or_null "$PATCH")" "$(json_or_null "$PRERELEASE")" "$DEFAULT" \
  > docs_site/rolling-versions.json
echo "docs_versions.sh: versions.json -> $(tr -d ' \n' < "$VERSIONS_JSON")"
echo "docs_versions.sh: rolling-versions.json -> $(cat docs_site/rolling-versions.json)"
