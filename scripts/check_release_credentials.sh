#!/usr/bin/env sh
# check_release_credentials.sh
#
# Release preflight: fail fast, before any build starts, if the credentials the
# Release workflow needs to mint its GitHub App token are missing or misplaced.
#
# Reads three environment variables (set by the `preflight` job in release.yml):
#   APP_ID                    value of the repository variable RELEASE_APP_ID
#   HAS_PRIVATE_KEY_SECRET    "true" if the secret RELEASE_APP_PRIVATE_KEY is non-empty
#   HAS_PRIVATE_KEY_VARIABLE  "true" if a repository *variable* named
#                             RELEASE_APP_PRIVATE_KEY exists (variables are plaintext)
#
# Only the booleans reach this script: the private key itself is never placed in
# an environment variable or printed, and APP_ID's value is never echoed.
#
# All three checks run, then the script exits 1 if any failed, printing one
# ::error:: line per failure.
#
# Exit codes: 0 credentials present, 1 at least one problem.

set -eu

FAILED=0

if [ -z "${APP_ID:-}" ]; then
  echo "::error::Repository variable RELEASE_APP_ID is empty or missing."
  FAILED=1
fi

if [ "${HAS_PRIVATE_KEY_SECRET:-}" != "true" ]; then
  echo "::error::Repository secret RELEASE_APP_PRIVATE_KEY is empty or missing (Settings → Secrets and variables → Actions → **Secrets**)."
  FAILED=1
fi

if [ "${HAS_PRIVATE_KEY_VARIABLE:-}" = "true" ]; then
  echo "::error::A repository **variable** named RELEASE_APP_PRIVATE_KEY exists. Variables are plaintext: delete it, rotate the App key, and store the key as a secret."
  FAILED=1
fi

if [ "$FAILED" -ne 0 ]; then
  exit 1
fi

echo "Release credentials present."
