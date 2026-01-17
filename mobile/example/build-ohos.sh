#!/bin/bash
# Build script for HarmonyOS/OpenHarmony
# Usage: ./build-ohos.sh [aarch64|x86_64|armv7] [debug|release]

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

# Load .env if it exists
if [ -f "$SCRIPT_DIR/.env" ]; then
    export $(grep -v '^#' "$SCRIPT_DIR/.env" | xargs)
fi

# Check OHOS_NDK_HOME
if [ -z "$OHOS_NDK_HOME" ]; then
    echo "Error: OHOS_NDK_HOME not set"
    echo "Either set it in .env or export it:"
    echo "  export OHOS_NDK_HOME=/path/to/openharmony/native"
    exit 1
fi

# Add LLVM bin to PATH
export PATH="$OHOS_NDK_HOME/llvm/bin:$PATH"

# Default values
ARCH="${1:-aarch64}"
PROFILE="${2:-debug}"

# Map arch to Rust target
case "$ARCH" in
    aarch64|arm64)
        TARGET="aarch64-unknown-linux-ohos"
        ;;
    x86_64|x64)
        TARGET="x86_64-unknown-linux-ohos"
        ;;
    armv7|arm)
        TARGET="armv7-unknown-linux-ohos"
        ;;
    *)
        echo "Unknown architecture: $ARCH"
        echo "Supported: aarch64, x86_64, armv7"
        exit 1
        ;;
esac

# Build flags
if [ "$PROFILE" = "release" ]; then
    CARGO_FLAGS="--release"
else
    CARGO_FLAGS=""
fi

echo "Building for $TARGET ($PROFILE)..."
echo "OHOS_NDK_HOME: $OHOS_NDK_HOME"

cargo build --target "$TARGET" --no-default-features $CARGO_FLAGS

echo "Build complete: target/$TARGET/$PROFILE/libexample.so"
