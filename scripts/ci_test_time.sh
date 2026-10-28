#!/usr/bin/env sh
# Test execution time of a `cargo test` log: the sum of libtest's
# `finished in <s>s` lines, i.e. without compilation (Q39).
# Usage: scripts/ci_test_time.sh <cargo-test-log> [<markdown-out>]
# Prints one line to stdout; appends a markdown table to <markdown-out> if given
# (CI passes "$GITHUB_STEP_SUMMARY"). Exits 1 if the log has no result lines.
set -eu
log=$1
md=${2:-}
esc=$(printf '\033')
LC_ALL=C sed "s/${esc}\[[0-9;]*[mK]//g" "$log" | awk -v md="$md" '
  /^ *(Running|Doc-tests) / {
    name = $0
    sub(/^ *(Running|Doc-tests) /, "", name)
    if (match(name, /deps\/[A-Za-z0-9_]+-/)) {
      crate = substr(name, RSTART + 5, RLENGTH - 6); sub(/ \(.*$/, "", name); name = crate " " name
    }
  }
  /^test result: .*finished in [0-9.]+s/ {
    match($0, /finished in [0-9.]+s/)
    t = substr($0, RSTART + 12, RLENGTH - 13) + 0
    total += t; n++
    rows = rows sprintf("| `%s` | %.2f |\n", name, t)
  }
  END {
    if (n == 0) { print "ci_test_time: no `finished in` lines in the log" > "/dev/stderr"; exit 1 }
    printf "Test execution time: %.1f s (sum of %d test binaries)\n", total, n
    if (md != "") {
      printf "### Test execution time\n\n**%.1f s** (sum of the %d `finished in` lines; compile time excluded, Q39)\n\n| Test binary | s |\n|---|---|\n%s\n", total, n, rows >> md
    }
  }'
