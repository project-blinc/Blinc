#!/bin/bash
# Script to update metadata for all Blinc crates
# Usage: ./scripts/update-crate-metadata.sh

set -e

VERSION="0.1.1"
REPO="https://github.com/project-blinc/Blinc"

# List of all crates to update
CRATES=(
    "crates/blinc_animation"
    "crates/blinc_app"
    "crates/blinc_cli"
    "crates/blinc_cn"
    "crates/blinc_core"
    "crates/blinc_debugger"
    "crates/blinc_flow"
    "crates/blinc_gpu"
    "crates/blinc_icons"
    "crates/blinc_image"
    "crates/blinc_layout"
    "crates/blinc_macros"
    "crates/blinc_paint"
    "crates/blinc_platform"
    "crates/blinc_recorder"
    "crates/blinc_runtime"
    "crates/blinc_svg"
    "crates/blinc_test_suite"
    "crates/blinc_text"
    "crates/blinc_theme"
    "extensions/blinc_platform_android"
    "extensions/blinc_platform_desktop"
    "extensions/blinc_platform_ios"
)

echo "Updating metadata for all crates..."

for crate_path in "${CRATES[@]}"; do
    cargo_file="$crate_path/Cargo.toml"
    crate_name=$(basename "$crate_path")

    if [ -f "$cargo_file" ]; then
        echo "Processing $crate_name..."

        # Add repository.workspace if not present
        if ! grep -q "^repository.workspace" "$cargo_file"; then
            sed -i '' '/^license.workspace/a\
repository.workspace = true
' "$cargo_file"
        fi

        # Add rust-version.workspace if not present
        if ! grep -q "^rust-version.workspace" "$cargo_file"; then
            sed -i '' '/^repository.workspace/a\
rust-version.workspace = true
' "$cargo_file"
        fi

        # Add documentation URL if not present
        if ! grep -q "^documentation" "$cargo_file"; then
            sed -i '' "/^repository.workspace/a\\
documentation = \"https://docs.rs/$crate_name\"
" "$cargo_file"
        fi

        # Add version to internal dependencies
        sed -i '' "s/blinc_\\([a-z_]*\\) = { path = \"\\([^\"]*\\)\" }/blinc_\\1 = { path = \"\\2\", version = \"$VERSION\" }/g" "$cargo_file"
        sed -i '' "s/blinc_\\([a-z_]*\\) = { path = \"\\([^\"]*\\)\", optional = true }/blinc_\\1 = { path = \"\\2\", version = \"$VERSION\", optional = true }/g" "$cargo_file"
        sed -i '' "s/blinc_\\([a-z_]*\\) = { path = \"\\([^\"]*\\)\", default-features = false }/blinc_\\1 = { path = \"\\2\", version = \"$VERSION\", default-features = false }/g" "$cargo_file"
        sed -i '' "s/blinc_\\([a-z_]*\\) = { path = \"\\([^\"]*\\)\", default-features = false, features = \\[\\([^]]*\\)\\] }/blinc_\\1 = { path = \"\\2\", version = \"$VERSION\", default-features = false, features = [\\3] }/g" "$cargo_file"
    else
        echo "Warning: $cargo_file not found"
    fi
done

echo "Done! Metadata updated for all crates."
