#!/usr/bin/env sh
# next_version.sh CURRENT BUMP CHANNEL
#
# Prints the next release version to stdout.  Used by the Release workflow.
#
#   CURRENT  Version currently in Cargo.toml (the last released version),
#            e.g. 0.3.0 or 0.4.0-rc.2
#   BUMP     patch | minor | major   (ignored when CURRENT is an -rc)
#   CHANNEL  stable | rc
#
# Rules:
#   CURRENT is stable (X.Y.Z)
#     stable → bump(CURRENT)                e.g. 0.3.0 + minor        → 0.4.0
#     rc     → bump(CURRENT)-rc.1           e.g. 0.3.0 + minor + rc   → 0.4.0-rc.1
#   CURRENT is a release candidate (X.Y.Z-rc.N)
#     rc     → X.Y.Z-rc.(N+1)               e.g. 0.4.0-rc.1           → 0.4.0-rc.2
#     stable → X.Y.Z (promote the RC)       e.g. 0.4.0-rc.2           → 0.4.0
#
# Exit codes: 0 on success, 1 on invalid input.

set -eu

if [ "$#" -ne 3 ]; then
  echo "usage: $0 CURRENT BUMP CHANNEL" >&2
  exit 1
fi

CURRENT=$1
BUMP=$2
CHANNEL=$3

case "$CHANNEL" in
  stable | rc) ;;
  *) echo "ERROR: channel must be 'stable' or 'rc' (got '$CHANNEL')" >&2; exit 1 ;;
esac

# Split CURRENT into BASE (X.Y.Z) and an optional "-rc.N" suffix.
BASE=${CURRENT%%-*}
RC_N=""
case "$CURRENT" in
  *-rc.*) RC_N=${CURRENT##*-rc.} ;;
  *-*) echo "ERROR: unsupported pre-release suffix in '$CURRENT' (only -rc.N is supported)" >&2; exit 1 ;;
esac

case "$BASE" in
  [0-9]*.[0-9]*.[0-9]*) ;;
  *) echo "ERROR: '$CURRENT' is not a valid X.Y.Z[-rc.N] version" >&2; exit 1 ;;
esac

MAJOR=${BASE%%.*}
REST=${BASE#*.}
MINOR=${REST%%.*}
PATCH=${REST#*.}

if [ -n "$RC_N" ]; then
  case "$RC_N" in
    '' | *[!0-9]*) echo "ERROR: invalid rc number in '$CURRENT'" >&2; exit 1 ;;
  esac
  if [ "$CHANNEL" = "rc" ]; then
    echo "${BASE}-rc.$((RC_N + 1))"
  else
    echo "${BASE}"
  fi
  exit 0
fi

case "$BUMP" in
  patch) PATCH=$((PATCH + 1)) ;;
  minor) MINOR=$((MINOR + 1)); PATCH=0 ;;
  major) MAJOR=$((MAJOR + 1)); MINOR=0; PATCH=0 ;;
  *) echo "ERROR: bump must be patch, minor or major (got '$BUMP')" >&2; exit 1 ;;
esac

NEXT="${MAJOR}.${MINOR}.${PATCH}"
if [ "$CHANNEL" = "rc" ]; then
  NEXT="${NEXT}-rc.1"
fi
echo "$NEXT"
