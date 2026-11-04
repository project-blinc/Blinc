#!/usr/bin/env bash
# Build CI from the tracked lockfile instead of resolving the crates.io
# index afresh, so one upstream yank cannot fail every workflow before it
# compiles anything. `Cargo.lock` is gitignored, so the tracked copy is
# `ci/workspace.lock`; refresh it with `scripts/ci-refresh-lock.sh`.
#
# `BLINC_FRESH_LOCK=1` skips the copy and resolves afresh, as a user's
# build does (the scheduled fresh-resolve workflow sets it).

set -euo pipefail

if [[ "${BLINC_FRESH_LOCK:-}" == "1" ]]; then
    echo "ci-use-lock: BLINC_FRESH_LOCK=1, resolving afresh"
    exit 0
fi

if [[ ! -f ci/workspace.lock ]]; then
    echo "ci-use-lock: ci/workspace.lock not found, resolving afresh"
    exit 0
fi

cp ci/workspace.lock Cargo.lock
echo "ci-use-lock: using ci/workspace.lock"
