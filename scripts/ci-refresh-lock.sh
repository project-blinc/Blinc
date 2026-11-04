#!/usr/bin/env bash
# Regenerate `ci/workspace.lock` from a fresh resolve of the published
# manifest. Run from a checkout with no local-dev patch block, then build
# and test before committing the new lock.

set -euo pipefail

bash scripts/ci-strip-local-patch.sh
rm -f Cargo.lock
cargo generate-lockfile
mkdir -p ci
cp Cargo.lock ci/workspace.lock
echo "ci-refresh-lock: wrote ci/workspace.lock"
