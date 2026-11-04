#!/usr/bin/env bash
# Strip the local-dev `[patch."…project-blinc/blinc_{canvas_kit,portal_ui,
# node_editor,game_kit}.git"]` block from Cargo.toml.
#
# The committed Cargo.toml never contains this block — the sibling crates
# resolve from their published git pins so a fresh clone builds with no
# setup. `scripts/use-local-packages.sh` appends the block when a dev opts
# into local checkouts under `packages/*` (gitignored); this script is the
# inverse (also exposed as `scripts/use-published-packages.sh`). CI still
# runs it defensively — a no-op on the already-clean committed manifest.
#
# Removes everything from the `# Local-dev redirects for the /packages/*`
# comment marker to EOF, then trims trailing blank lines so repeated
# enable/disable cycles converge on one canonical form. The earlier
# `[patch."…project-blinc/Blinc.git"]` block (redirecting to in-repo
# `crates/*`, present on every clone) is untouched.
#
# Idempotent: safe to run even if the block is already absent.
#
# Every CI job runs this first, so it also installs the tracked lockfile
# (`scripts/ci-use-lock.sh`).

set -euo pipefail

CARGO_TOML="${1:-Cargo.toml}"

if [[ ! -f "$CARGO_TOML" ]]; then
    echo "error: $CARGO_TOML not found" >&2
    exit 1
fi

if ! grep -q '^# Local-dev redirects for the /packages' "$CARGO_TOML"; then
    echo "ci-strip-local-patch: no local-dev patch block found in $CARGO_TOML, nothing to do"
    bash "$(dirname "$0")/ci-use-lock.sh"
    exit 0
fi

# Portable across BSD (macOS) and GNU: write to a temp file then move.
# Keep lines up to (not including) the marker, and drop trailing blank
# lines so the result is a single canonical form.
tmp="$(mktemp)"
awk '
    /^# Local-dev redirects for the \/packages/ { stop = 1 }
    stop { next }
    { lines[++n] = $0; if (NF) last = n }
    END { for (i = 1; i <= last; i++) print lines[i] }
' "$CARGO_TOML" > "$tmp"
mv "$tmp" "$CARGO_TOML"

echo "ci-strip-local-patch: stripped local-dev patch block from $CARGO_TOML"
bash "$(dirname "$0")/ci-use-lock.sh"
