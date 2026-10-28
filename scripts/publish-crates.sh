#!/bin/bash
# Print the release plan by default; --dry-run verifies packages, --publish uploads them.
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR/.."
exec python3 "$SCRIPT_DIR/publish_crates.py" "$@"
