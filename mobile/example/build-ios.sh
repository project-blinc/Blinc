#!/bin/bash
# Build script for iOS mobile example

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# Project configuration
PROJECT_NAME="BlincApp"
BUNDLE_ID="com.blinc.example"
LIB_NAME="libexample.a"

# iOS targets
TARGET_ARM64="aarch64-apple-ios"
TARGET_SIM_ARM64="aarch64-apple-ios-sim"
TARGET_SIM_X86="x86_64-apple-ios"

# Detect build mode
BUILD_MODE="${1:-debug}"
CARGO_FLAGS=""
TARGET_DIR="debug"

if [ "$BUILD_MODE" = "release" ]; then
    CARGO_FLAGS="--release"
    TARGET_DIR="release"
    echo "Building in RELEASE mode..."
else
    echo "Building in DEBUG mode..."
fi

# Ensure iOS targets are installed
echo "Checking Rust iOS targets..."
if ! rustup target list --installed | grep -q "$TARGET_ARM64"; then
    echo "Installing $TARGET_ARM64..."
    rustup target add "$TARGET_ARM64"
fi

if ! rustup target list --installed | grep -q "$TARGET_SIM_ARM64"; then
    echo "Installing $TARGET_SIM_ARM64..."
    rustup target add "$TARGET_SIM_ARM64"
fi

# Step 1: Build Rust static library
echo ""
echo "=== Building Rust static library ==="
cd "$SCRIPT_DIR"

# Build for device (arm64)
echo "Building for device ($TARGET_ARM64)..."
cargo build --lib --features ios $CARGO_FLAGS --target "$TARGET_ARM64"

# Build for simulator (arm64 for Apple Silicon Macs)
echo "Building for simulator ($TARGET_SIM_ARM64)..."
cargo build --lib --features ios $CARGO_FLAGS --target "$TARGET_SIM_ARM64"

# Copy libraries to iOS project
LIBS_DIR="$SCRIPT_DIR/platforms/ios/libs"
mkdir -p "$LIBS_DIR/device"
mkdir -p "$LIBS_DIR/simulator"

echo "Copying libraries..."
cp "target/$TARGET_ARM64/$TARGET_DIR/$LIB_NAME" "$LIBS_DIR/device/"
cp "target/$TARGET_SIM_ARM64/$TARGET_DIR/$LIB_NAME" "$LIBS_DIR/simulator/"

# Create universal library for simulator (arm64 + x86_64) if both exist
if [ -f "$SCRIPT_DIR/target/$TARGET_SIM_X86/$TARGET_DIR/$LIB_NAME" ]; then
    echo "Creating universal simulator library..."
    lipo -create \
        "$SCRIPT_DIR/target/$TARGET_SIM_ARM64/$TARGET_DIR/$LIB_NAME" \
        "$SCRIPT_DIR/target/$TARGET_SIM_X86/$TARGET_DIR/$LIB_NAME" \
        -output "$LIBS_DIR/simulator/$LIB_NAME"
fi

echo ""
echo "=== Build complete ==="
echo ""
echo "Libraries are at:"
echo "  Device:    $LIBS_DIR/device/$LIB_NAME"
echo "  Simulator: $LIBS_DIR/simulator/$LIB_NAME"
echo ""
echo "Next steps:"
echo "  1. Open platforms/ios/$PROJECT_NAME.xcodeproj in Xcode"
echo "  2. Select your target device/simulator"
echo "  3. Build and run (Cmd+R)"
echo ""
