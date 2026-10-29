#!/usr/bin/env sh
# Line-coverage gate over a `cargo llvm-cov report --json --summary-only` export.
# Usage: scripts/coverage_gate.sh <summary.json> [<markdown-out>] [--report-only]
# Areas and thresholds: scripts/coverage_gates.conf. Production code only: files
# under crates/*/src/tests/, crates/ssg-sim/src/test_support/ and crates/*/tests/
# are test code and are not counted. Exit 1 if an area is below its threshold
# (unless --report-only), 2 on usage or input errors.
set -eu
json=${1:?usage: coverage_gate.sh <summary.json> [<markdown-out>] [--report-only]}
shift
md=""; enforce=1
for arg in "$@"; do
  case $arg in
    --report-only) enforce=0 ;;
    *) md=$arg ;;
  esac
done
conf="$(dirname "$0")/coverage_gates.conf"
root=$(git rev-parse --show-toplevel 2>/dev/null || pwd)

files=$(jq -r --arg root "$root/" '
  .data[0].files[]
  | (.filename | ltrimstr($root)) as $f
  | select($f | startswith("crates/"))
  | select($f | test("^crates/[^/]+/src/tests/|^crates/ssg-sim/src/test_support/|^crates/[^/]+/tests/") | not)
  | "\($f)\t\(.summary.lines.count)\t\(.summary.lines.covered)"' "$json")
[ -n "$files" ] || { echo "coverage_gate: no workspace files in $json" >&2; exit 2; }

fail=0
table="| Area | Lines | Covered | Line % | Gate | Result |
|---|---|---|---|---|---|"
while read -r area path min; do
  case $area in ''|'#'*) continue ;; esac
  set -- $(printf '%s\n' "$files" | awk -F'\t' -v p="$path" 'index($1, p) == 1 { n += $2; c += $3 } END { printf "%d %d\n", n, c }')
  n=$1; c=$2
  [ "$n" -gt 0 ] || { echo "coverage_gate: area $area ($path) has no lines" >&2; exit 2; }
  pct=$(awk -v c="$c" -v n="$n" 'BEGIN { printf "%.2f", 100 * c / n }')
  if [ "$min" = report ]; then gate="—"; result="reported"
  elif awk -v p="$pct" -v m="$min" 'BEGIN { exit !(p >= m) }'; then gate=">= $min %"; result="pass"
  else gate=">= $min %"; result="**below**"; fail=1
  fi
  table="$table
| \`$area\` | $n | $c | $pct | $gate | $result |"
  printf '%-16s %6d / %-6d %6s %%  %s\n' "$area" "$c" "$n" "$pct" "$result"
done < "$conf"

if [ -n "$md" ]; then
  mode="enforced"; [ "$enforce" -eq 1 ] || mode="report only, not enforced yet"
  printf '### Line coverage (%s)\n\n%s\n\nTest code (`src/tests/`, `test_support/`, `tests/`) is not counted.\n\n' "$mode" "$table" >> "$md"
fi
if [ "$fail" -eq 1 ] && [ "$enforce" -eq 1 ]; then
  echo "coverage_gate: an area is below its gate" >&2; exit 1
fi
exit 0
