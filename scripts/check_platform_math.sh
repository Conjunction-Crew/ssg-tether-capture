#!/bin/sh
# Platform-math guard: the simulation must give the same bits on Linux, macOS
# and Windows, so its library code may not reach any platform's own math
# library or C code. Three parts:
#
#   1. calls   clippy with scripts/platform_math_guard/clippy.toml over the
#              library code of ssg-sim and sgp4-portable (`--lib`: tests may
#              call `f64::sin` and brahe's propagators): the `std` float
#              methods (sin, powi, ...) and the brahe items that compute with
#              them are refused; `crate::math` (libm) is the way. No
#              `allow`/`expect` of the two lints in those crates.
#   2. brahe   every `brahe::` item named by ssg-sim's library code (files
#              under crates/ssg-sim/src outside tests/ and test_support/) must
#              be on scripts/platform_math_guard/brahe_allowlist.txt, so the
#              list of brahe items in use is closed.
#   3. deps    from `cargo tree`: avian3d has `enhanced-determinism` and no
#              `simd`; parry3d-f64 has `enhanced-determinism`; every glam has
#              `libm` (except GLAM_ALLOWED, each reached only through its named
#              dependent); simba has `libm_force`; rsofa and sgp4 (C code and
#              upstream SGP4) are reached through brahe only; AVIAN_FEATURES in
#              crates/ssg-sim/src/project/manifest/mod.rs matches avian's
#              `enhanced-determinism`/`simd`/`parallel` features in the tree.
#
# Usage: sh scripts/check_platform_math.sh [--self-test]
#   --self-test  run each part on the seeded violations under
#                scripts/platform_math_guard/selftest/ and check that each is
#                reported (one line per case); the tree is not checked
#
# Exit 0: clean (or every seeded violation reported). Exit 1: a violation (or a
# seeded violation not reported). Exit 2: usage, or a tool failed.
set -eu

ROOT=$(git rev-parse --show-toplevel)
cd "$ROOT"
GUARD=scripts/platform_math_guard
ALLOW=$GUARD/brahe_allowlist.txt
MANIFEST_RS=crates/ssg-sim/src/project/manifest/mod.rs

# glam versions allowed without `libm`, each as version:only-dependent.
# 0.31.1 is avian's broad-phase tree (obvhs); it measured identical on the
# three platforms to capture.
GLAM_ALLOWED="0.31.1:obvhs"

MODE=check
case "${1:-}" in
  --self-test) MODE=selftest ;;
  "") ;;
  *) echo "usage: sh scripts/check_platform_math.sh [--self-test]" >&2; exit 2 ;;
esac
[ $# -le 1 ] || { echo "usage: sh scripts/check_platform_math.sh [--self-test]" >&2; exit 2; }
[ -f "$GUARD/clippy.toml" ] || { echo "missing $GUARD/clippy.toml" >&2; exit 2; }
[ -f "$ALLOW" ] || { echo "missing $ALLOW" >&2; exit 2; }

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

CLIPPY_LINTS="-A clippy::all -D clippy::disallowed_methods -D clippy::disallowed_types"

# Part 1 helper. $1 = clippy's output, $2 = its exit status. Prints each
# disallowed use (what, where, what instead); returns 0 clean, 1 found, 2 a
# failure that is not the guard's (a compile error).
clippy_findings() {
  if [ "$2" -eq 0 ]; then return 0; fi
  if ! grep -q 'error: use of a disallowed' "$1"; then
    echo "part 1 (calls): clippy failed for another reason:" >&2
    tail -n 30 "$1" >&2
    return 2
  fi
  awk '
    /^error: use of a disallowed/ { what = $0; sub(/^error: use of a /, "", what); next }
    what != "" && /-->/ { where = $0; sub(/^.*--> */, "", where); next }
    what != "" && /= note: / && !/requested on the command line/ {
      why = $0; sub(/^.*= note: */, "", why)
      printf "part 1 (calls): %s at %s\n    instead: %s\n", what, where, why
      what = ""
    }' "$1"
  return 1
}

part1() {
  st=0
  CLIPPY_CONF_DIR="$ROOT/$GUARD" cargo clippy -p ssg-sim -p sgp4-portable --lib --locked \
    -- $CLIPPY_LINTS > "$TMP/clippy.log" 2>&1 || st=$?
  clippy_findings "$TMP/clippy.log" "$st" || return $?
  if grep -rnE '(allow|expect)\(.*disallowed_(methods|types)' crates/ssg-sim/src crates/sgp4-portable/src > "$TMP/allow.txt"; then
    awk '{ printf "part 1 (calls): the guard switched off at %s\n    instead: remove the attribute; use crate::math or crate::orbit\n", $0 }' "$TMP/allow.txt"
    return 1
  fi
  return 0
}

# Part 2 helper: every brahe item named in the given files, as
# `file:line<TAB>item`. `//` comments are skipped; `use` statements may span
# lines. An item is the first path segment that starts with a capital letter
# (a type or constant: `Epoch::from_x` is `Epoch`), else the last segment (a
# function). A glob import gives `*`; brahe imported whole or renamed gives
# `brahe`.
brahe_items() {
  awk '
    function item(p,   n, s, i) {
      n = split(p, s, "::")
      for (i = 1; i <= n; i++) if (s[i] ~ /^[A-Z]/) return s[i]
      return s[n]
    }
    function leaf(t, d, at,   p) {
      sub(/[[:space:]]+as[[:space:]]+[A-Za-z0-9_]+[[:space:]]*$/, "", t)
      gsub(/[[:space:]]/, "", t)
      if (t == "") return
      if (t ~ /\*$/) { print at "\t*"; return }
      if (t == "self") {
        p = (d > 0) ? pre[d] : ""
        gsub(/[[:space:]]/, "", p); sub(/::$/, "", p)
        t = (p == "") ? "brahe" : p
      }
      print at "\t" item(t)
    }
    function use_stmt(text, at,   n, i, c, tok, d) {
      sub(/^[^A-Za-z]*use[[:space:]]+/, "", text)
      sub(/;.*$/, "", text)
      sub(/^::/, "", text)
      if (text ~ /^brahe[[:space:]]*$/ || text ~ /^brahe[[:space:]]+as[[:space:]]/) { print at "\tbrahe"; return }
      sub(/^brahe[[:space:]]*::[[:space:]]*/, "", text)
      n = length(text); tok = ""; d = 0
      for (i = 1; i <= n + 1; i++) {
        c = (i <= n) ? substr(text, i, 1) : ","
        if (c == "{") { pre[++d] = tok; tok = "" }
        else if (c == "," || c == "}") { leaf(tok, d, at); tok = ""; if (c == "}") d-- }
        else tok = tok c
      }
    }
    FNR == 1 { inuse = 0 }
    {
      line = $0
      sub(/\/\/.*$/, "", line)
      if (inuse) {
        buf = buf " " line
        if (index(line, ";")) { use_stmt(buf, start); inuse = 0 }
        next
      }
      if (match(line, /(^|[^A-Za-z0-9_:])use[[:space:]]+(::)?brahe([^A-Za-z0-9_]|$)/)) {
        rest = substr(line, RSTART)
        if (index(rest, ";")) use_stmt(rest, FILENAME ":" FNR)
        else { inuse = 1; buf = rest; start = FILENAME ":" FNR }
        next
      }
      while (match(line, /(^|[^A-Za-z0-9_:])(::)?brahe[[:space:]]*::[[:space:]]*[A-Za-z_][A-Za-z0-9_]*([[:space:]]*::[[:space:]]*[A-Za-z_][A-Za-z0-9_]*)*/)) {
        p = substr(line, RSTART, RLENGTH)
        line = substr(line, RSTART + RLENGTH)
        gsub(/[[:space:]]/, "", p)
        sub(/^.*brahe::/, "", p)
        print FILENAME ":" FNR "\t" item(p)
      }
    }' "$@"
}

# Part 2 check over the given files: prints each item that is not allowed;
# returns 1 if any, 2 on a bad allow-list line.
brahe_check() {
  if awk '
      /^[[:space:]]*(#|$)/ { next }
      { h = index($0, "#"); r = (h > 0) ? substr($0, h + 1) : ""
        gsub(/[[:space:]]/, "", r)
        if (h == 0 || r == "") { print FILENAME ":" FNR ": entry without a `# reason`: " $0; bad = 1 } }
      END { exit bad }' "$ALLOW" > "$TMP/allow-bad.txt"; then :; else
    cat "$TMP/allow-bad.txt" >&2
    return 2
  fi
  sed -e 's/#.*$//' -e 's/[[:space:]]//g' -e '/^$/d' "$ALLOW" > "$TMP/allowed.txt"
  brahe_items "$@" > "$TMP/items.txt"
  awk -F'\t' '
    FNR == NR { ok[$0] = 1; next }
    $2 == "*" { printf "part 2 (brahe): a glob import of brahe at %s\n    instead: import the items one by one, each on %s\n", $1, allow; bad = 1; next }
    $2 == "brahe" { printf "part 2 (brahe): brahe imported whole or renamed at %s\n    instead: name its items with `brahe::`, each on %s\n", $1, allow; bad = 1; next }
    !($2 in ok) { printf "part 2 (brahe): brahe item `%s` at %s is not on the allow-list: brahe computes with platform math\n    instead: use crate::orbit::{kepler, frames, sgp4_start} or crate::math, or add the item to %s with a reason\n", $2, $1, allow; bad = 1 }
    END { exit bad }' allow="$ALLOW" "$TMP/allowed.txt" "$TMP/items.txt"
}

part2() {
  find crates/ssg-sim/src -name '*.rs' ! -path '*/tests/*' ! -path '*/test_support/*' \
    | LC_ALL=C sort > "$TMP/libfiles.txt"
  # shellcheck disable=SC2046
  brahe_check $(cat "$TMP/libfiles.txt")
}

# Part 3 helper over a `cargo tree -f '{p} [{f}]' --prefix none` output: prints
# one line per failed assertion; returns 1 if any.
tree_checks() {
  awk -v glam_allowed="$GLAM_ALLOWED" '
    function feats(l,   f) { f = l; sub(/^[^[]*\[/, "", f); sub(/\].*$/, "", f); return "," f "," }
    function has(l, x) { return index(feats(l), "," x ",") > 0 }
    function fail(k, msg) { if (!(k in done)) { done[k] = 1; out[++n] = msg } }
    BEGIN { m = split(glam_allowed, g, " "); for (i = 1; i <= m; i++) { split(g[i], vd, ":"); allowed["v" vd[1]] = 1 } }
    /^#/ { next }
    $1 == "avian3d" {
      seen["avian3d"] = 1
      if (!has($0, "enhanced-determinism")) fail("avian-ed", "part 3 (deps): avian3d " $2 " lacks the feature `enhanced-determinism`\n    instead: list it in Cargo.toml'"'"'s avian3d features (it puts parry, glam and simba on libm)")
      if (has($0, "simd")) fail("avian-simd", "part 3 (deps): avian3d " $2 " has the feature `simd`: SIMD paths give other bits on other platforms\n    instead: remove `simd` from Cargo.toml'"'"'s avian3d features")
    }
    $1 == "parry3d-f64" {
      seen["parry3d-f64"] = 1
      if (!has($0, "enhanced-determinism")) fail("parry", "part 3 (deps): parry3d-f64 " $2 " lacks the feature `enhanced-determinism`\n    instead: turn on avian3d'"'"'s `enhanced-determinism`, which turns it on")
    }
    $1 == "glam" {
      seen["glam"] = 1
      if (!has($0, "libm") && !(("" $2) in allowed)) fail("glam" $2, "part 3 (deps): glam " $2 " lacks the feature `libm`: its math would call the platform'"'"'s library\n    instead: turn on avian3d'"'"'s `enhanced-determinism`, or add the version to GLAM_ALLOWED in scripts/check_platform_math.sh with the reason")
    }
    $1 == "simba" {
      seen["simba"] = 1
      if (!has($0, "libm_force")) fail("simba", "part 3 (deps): simba " $2 " lacks the feature `libm_force`: nalgebra'"'"'s math would call the platform'"'"'s library\n    instead: turn on avian3d'"'"'s `enhanced-determinism`, which turns it on")
    }
    END {
      split("avian3d parry3d-f64 glam simba", want, " ")
      for (i = 1; i <= 4; i++) if (!(want[i] in seen)) out[++n] = "part 3 (deps): " want[i] " is not in the dependency tree, so its features cannot be checked\n    instead: update the checks in scripts/check_platform_math.sh"
      for (i = 1; i <= n; i++) print out[i]
      exit (n > 0)
    }' "$1"
}

# `cargo tree -i <pkg>@<ver> --depth 1`: the names of the package's dependents.
dependents() {
  cargo tree --workspace -i "$1" -e normal --depth 1 --prefix none --locked 2> "$TMP/tree-i.err" \
    | sed 1d | awk '{ print $1 }' | LC_ALL=C sort -u
}

part3() {
  bad=0
  cargo tree --workspace -e normal -f '{p} [{f}]' --prefix none --locked > "$TMP/tree.txt" 2> "$TMP/tree.err" \
    || { echo "part 3 (deps): cargo tree failed:" >&2; cat "$TMP/tree.err" >&2; return 2; }
  tree_checks "$TMP/tree.txt" || bad=1

  for entry in $GLAM_ALLOWED; do
    v=${entry%%:*}; only=${entry#*:}
    grep -q "^glam v$v " "$TMP/tree.txt" || continue
    got=$(dependents "glam@$v" | tr '\n' ' ' | sed 's/ $//')
    if [ "$got" != "$only" ]; then
      echo "part 3 (deps): glam $v (allowed without libm only through $only) is reached through: $got"
      echo "    instead: keep it to $only, or check the new path for platform math and update GLAM_ALLOWED"
      bad=1
    fi
  done

  for pkg in rsofa sgp4; do
    grep -q "^$pkg v" "$TMP/tree.txt" || continue
    got=$(dependents "$pkg" | tr '\n' ' ' | sed 's/ $//')
    if [ "$got" != "brahe" ]; then
      echo "part 3 (deps): $pkg is reached through: $got (brahe only is allowed: part 2 closes brahe)"
      echo "    instead: use crate::orbit::{frames, sgp4_start}"
      bad=1
    fi
  done

  declared=$(grep -E '^(pub(\(crate\))? )?const AVIAN_FEATURES' "$MANIFEST_RS" \
    | sed 's/^[^=]*=//' | grep -oE '"[^"]+"' | tr -d '"' | LC_ALL=C sort | tr '\n' ' ' | sed 's/ $//')
  in_tree=$(grep -m1 '^avian3d ' "$TMP/tree.txt" | sed 's/^[^[]*\[//; s/\].*$//' | tr ',' '\n' \
    | grep -xE 'enhanced-determinism|simd|parallel' | LC_ALL=C sort | tr '\n' ' ' | sed 's/ $//')
  if [ -z "$declared" ] || [ "$declared" != "$in_tree" ]; then
    echo "part 3 (deps): AVIAN_FEATURES in $MANIFEST_RS is [$declared]; the tree's avian3d has [$in_tree]"
    echo "    instead: make the constant list what Cargo.toml turns on (recordings carry it)"
    bad=1
  fi
  return $bad
}

if [ "$MODE" = selftest ]; then
  S=$GUARD/selftest
  failed=0
  report() { # $1 = 0 when the seeded violation was reported, $2 = the case
    if [ "$1" -eq 0 ]; then echo "self-test ok:     $2"; else echo "self-test FAILED: $2"; failed=1; fi
  }

  st=0
  CLIPPY_CONF_DIR="$ROOT/$GUARD" cargo clippy --manifest-path "$S/seeded_crate/Cargo.toml" --lib --locked \
    --target-dir "$ROOT/target/platform-math-guard-selftest" -- $CLIPPY_LINTS > "$TMP/clippy.log" 2>&1 || st=$?
  r=0; clippy_findings "$TMP/clippy.log" "$st" > "$TMP/p1.txt" || r=$?
  [ "$r" -eq 2 ] && exit 2
  grep -q 'disallowed method `f64::sin`' "$TMP/p1.txt" && r=0 || r=1
  report "$r" "part 1 reports f64::sin in $S/seeded_crate/src/lib.rs"

  r=0; brahe_check "$S/seeded_brahe_use.txt" > "$TMP/p2.txt" || r=$?
  [ "$r" -eq 2 ] && exit 2
  if grep -q 'item `SGPPropagator`' "$TMP/p2.txt" && ! grep -q 'item `Epoch`' "$TMP/p2.txt"; then r=0; else r=1; fi
  report "$r" "part 2 reports SGPPropagator (and not Epoch) in $S/seeded_brahe_use.txt"

  tree_checks "$S/cargo-tree-v0.4.1.txt" > "$TMP/p3.txt" || true
  for c in "avian3d .* has the feature \`simd\`|avian3d with simd" \
           "parry3d-f64 .* lacks the feature \`enhanced-determinism\`|parry3d-f64 without enhanced-determinism" \
           "glam .* lacks the feature \`libm\`|glam without libm" \
           "simba .* lacks the feature \`libm_force\`|simba without libm_force"; do
    pat=${c%%|*}; name=${c#*|}
    grep -qE "part 3 \(deps\): $pat" "$TMP/p3.txt" && r=0 || r=1
    report "$r" "part 3 reports $name in $S/cargo-tree-v0.4.1.txt"
  done

  if [ "$failed" -ne 0 ]; then
    echo "platform-math guard self-test: a seeded violation was not reported; the guard would miss it"
    exit 1
  fi
  echo "platform-math guard self-test: every seeded violation reported"
  exit 0
fi

status=0
for p in 1 2 3; do
  r=0; "part$p" || r=$?
  if [ "$r" -eq 2 ]; then exit 2; fi
  if [ "$r" -ne 0 ]; then status=1; fi
done
if [ "$status" -ne 0 ]; then
  echo "platform-math guard: violations above (see docs_site/docs/contributing/testing.md, \"Platform math\")"
  exit 1
fi
echo "platform-math guard: clean (part 1 calls, part 2 brahe items $(cut -f2 "$TMP/items.txt" | LC_ALL=C sort -u | tr '\n' ' ' | sed 's/ $//'), part 3 dependency features)"
exit 0
