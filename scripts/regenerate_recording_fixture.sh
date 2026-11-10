#!/bin/sh
# Makes the committed recording crates/ssg-sim-cli/tests/fixtures/q13-6400 again:
# the reference scenario (Q13: the Hubble Space Telescope and the P0 chaser at
# the fixed epoch) for 6,400 steps, recorded by a release build of this checkout.
#
# Usage: sh scripts/regenerate_recording_fixture.sh
#
# Run it when a reference number ("golden") moves, and commit the six files with
# that change. The fixture's point is that one platform made it and every
# platform replays it bit for bit, so the script runs on macOS on Apple Silicon
# only. It prints the manifest's `built_with` and the run's `state_hash`.
#
# Exit 0: the six files were replaced. Exit 1: wrong platform or a failed step.
set -eu

ROOT=$(git rev-parse --show-toplevel)
cd "$ROOT"

platform=$(uname -sm)
if [ "$platform" != "Darwin arm64" ]; then
    echo "error: the fixture is made on macOS on Apple Silicon (uname -sm: Darwin arm64); this is $platform" >&2
    exit 1
fi

FIXTURE=crates/ssg-sim-cli/tests/fixtures/q13-6400
FILES="manifest.json frames.jsonl inputs.jsonl keyframes.parquet log.jsonl transitions.jsonl"

cargo build --release -p ssg-sim-cli --locked

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/project/datasets"
cp crates/ssg-sim/src/test_support/fixtures/q13_hubble_pair.json "$tmp/project/datasets/q13.json"

./target/release/ssg-sim run "$tmp/project" --target 20580 --chaser 99901 \
    --epoch 2026-04-03T16:03:47.002464 --steps 6400 --sample-every 64 \
    --record q13-6400 --assets assets --out "$tmp/out"

recording="$tmp/project/recordings/q13-6400"
for f in $FILES; do
    if [ ! -f "$recording/$f" ]; then
        echo "error: the recording has no $f" >&2
        exit 1
    fi
done

mkdir -p "$FIXTURE"
for f in $FILES; do
    cp "$recording/$f" "$FIXTURE/$f"
done

python3 - "$FIXTURE/manifest.json" "$tmp/out/summary.json" <<'PY'
import json, sys
manifest = json.load(open(sys.argv[1]))
summary = json.load(open(sys.argv[2]))
print("built_with", json.dumps(manifest["built_with"]))
print("state_hash", summary["state_hash"])
PY
