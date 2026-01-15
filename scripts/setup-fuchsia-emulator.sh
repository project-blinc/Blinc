#!/usr/bin/env bash
#
# setup-fuchsia-emulator.sh - Set up Fuchsia Emulator (FEMU) for Blinc development
#
# This script sets up the Fuchsia emulator environment for testing Blinc apps.
# Requires the Fuchsia SDK to be installed first (run setup-fuchsia-sdk.sh).
#
# Usage:
#   ./scripts/setup-fuchsia-emulator.sh [--help] [--no-start]
#
# Options:
#   --help      Show this help message
#   --no-start  Setup only, don't start the emulator
#
# Prerequisites:
#   - Fuchsia SDK installed (via setup-fuchsia-sdk.sh)
#   - 16GB+ RAM recommended
#   - Hardware virtualization support (KVM on Linux, Hypervisor.framework on macOS)

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

info() { echo -e "${BLUE}[INFO]${NC} $*"; }
success() { echo -e "${GREEN}[SUCCESS]${NC} $*"; }
warning() { echo -e "${YELLOW}[WARNING]${NC} $*"; }
error() { echo -e "${RED}[ERROR]${NC} $*" >&2; }

# Defaults
FUCHSIA_DIR="${FUCHSIA_DIR:-$HOME/.fuchsia}"
START_EMU=true

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --help|-h)
            head -27 "$0" | tail -20
            exit 0
            ;;
        --no-start)
            START_EMU=false
            shift
            ;;
        *)
            error "Unknown option: $1"
            exit 1
            ;;
    esac
done

# Detect OS
detect_os() {
    case "$(uname -s)" in
        Darwin) echo "macos" ;;
        Linux)  echo "linux" ;;
        *)      echo "unsupported" ;;
    esac
}

OS=$(detect_os)
if [[ "$OS" == "unsupported" ]]; then
    error "Unsupported operating system. FEMU requires macOS or Linux."
    exit 1
fi

# Check for Fuchsia SDK
check_sdk() {
    info "Checking for Fuchsia SDK..."

    if [[ ! -d "$FUCHSIA_DIR/sdk" ]]; then
        error "Fuchsia SDK not found at $FUCHSIA_DIR/sdk"
        error "Please run: ./scripts/setup-fuchsia-sdk.sh"
        exit 1
    fi

    # Verify ffx is available
    if [[ ! -x "$FUCHSIA_DIR/sdk/tools/ffx" ]] && [[ ! -x "$FUCHSIA_DIR/sdk/tools/x64/ffx" ]]; then
        error "ffx tool not found in SDK"
        exit 1
    fi

    # Determine ffx path
    if [[ -x "$FUCHSIA_DIR/sdk/tools/ffx" ]]; then
        FFX="$FUCHSIA_DIR/sdk/tools/ffx"
    else
        FFX="$FUCHSIA_DIR/sdk/tools/x64/ffx"
    fi

    success "Found Fuchsia SDK with ffx at: $FFX"
}

# Check virtualization support
check_virtualization() {
    info "Checking hardware virtualization support..."

    case "$OS" in
        macos)
            # Check for Hypervisor.framework
            if sysctl -n kern.hv_support 2>/dev/null | grep -q "1"; then
                success "Hypervisor.framework is available"
            else
                warning "Hypervisor.framework may not be available"
                warning "FEMU may run slowly without hardware acceleration"
            fi
            ;;
        linux)
            # Check for KVM
            if [[ -r /dev/kvm ]] && [[ -w /dev/kvm ]]; then
                success "KVM is available and accessible"
            elif [[ -e /dev/kvm ]]; then
                warning "KVM exists but is not accessible"
                warning "Try: sudo usermod -aG kvm $USER"
                warning "Then log out and back in"
            else
                warning "KVM not available - FEMU will be slow"
                warning "Enable VT-x/AMD-V in BIOS if available"
            fi
            ;;
    esac
}

# Download product bundle (contains emulator images)
download_product_bundle() {
    info "Downloading Fuchsia product bundle for emulator..."

    # The product bundle includes:
    # - Fuchsia system images
    # - QEMU/AEMU emulator binaries
    # - Required metadata

    PRODUCT="workbench_eng.x64"  # Development product with full tooling

    # Use ffx to download
    info "Fetching product bundle: $PRODUCT"
    info "This may take a while (several GB download)..."

    if ! "$FFX" product download "$PRODUCT" --repository "https://releases.fuchsia.dev" 2>/dev/null; then
        # Try alternative approach
        warning "Direct download failed, trying alternative method..."

        # List available products
        info "Available product bundles:"
        "$FFX" product list --all 2>/dev/null | head -20 || true

        # Try the core product (smaller)
        PRODUCT="core.x64"
        info "Trying smaller product: $PRODUCT"
        "$FFX" product download "$PRODUCT" 2>/dev/null || {
            error "Failed to download product bundle"
            error "You may need to manually download from:"
            error "  https://fuchsia.dev/fuchsia-src/development/build/emulator"
            return 1
        }
    fi

    success "Product bundle downloaded: $PRODUCT"
    EMU_PRODUCT="$PRODUCT"
}

# Configure FFX for emulator use
configure_ffx() {
    info "Configuring FFX for emulator use..."

    # Set up discovery
    "$FFX" config set discovery.mdns.enabled true 2>/dev/null || true

    # Enable emulator support
    "$FFX" config set emu.enable_graphics true 2>/dev/null || true
    "$FFX" config set emu.headless false 2>/dev/null || true

    success "FFX configured for emulator"
}

# Start emulator
start_emulator() {
    info "Starting Fuchsia emulator..."

    if [[ -z "${EMU_PRODUCT:-}" ]]; then
        # Try to find a downloaded product
        EMU_PRODUCT="workbench_eng.x64"
    fi

    info "Using product: $EMU_PRODUCT"

    # Start with graphics (for UI development)
    # Use --headless for CI/server environments
    "$FFX" emu start "$EMU_PRODUCT" \
        --gpu swiftshader_indirect \
        --net user \
        --startup-timeout 120 \
        2>&1 &

    EMU_PID=$!

    info "Emulator starting in background (PID: $EMU_PID)"
    info "Waiting for emulator to be ready..."

    # Wait for emulator to be discoverable
    local max_wait=120
    local waited=0
    while [[ $waited -lt $max_wait ]]; do
        if "$FFX" target list 2>/dev/null | grep -q "fuchsia-emulator"; then
            success "Emulator is ready!"
            return 0
        fi
        sleep 5
        waited=$((waited + 5))
        echo -n "."
    done
    echo

    warning "Emulator may still be starting. Check with: ffx target list"
    return 0
}

# Show usage instructions
show_usage_instructions() {
    echo
    info "=========================================="
    info "Fuchsia Emulator Setup Complete"
    info "=========================================="
    echo
    info "Quick Start Commands:"
    echo "  # Start emulator (headless)"
    echo "  $FFX emu start ${EMU_PRODUCT:-workbench_eng.x64} --headless"
    echo
    echo "  # Start emulator (with graphics)"
    echo "  $FFX emu start ${EMU_PRODUCT:-workbench_eng.x64}"
    echo
    echo "  # List running emulators/devices"
    echo "  $FFX target list"
    echo
    echo "  # Connect to emulator shell"
    echo "  $FFX target default set fuchsia-emulator"
    echo "  $FFX component run fuchsia-pkg://fuchsia.com/your_package#meta/your_component.cm"
    echo
    echo "  # View logs"
    echo "  $FFX log --filter blinc"
    echo
    echo "  # Stop emulator"
    echo "  $FFX emu stop"
    echo
    info "For VNC access (graphical):"
    echo "  $FFX target vnc"
    echo
    info "Documentation:"
    echo "  https://fuchsia.dev/fuchsia-src/development/build/emulator"
    echo
    info "Building Blinc for Fuchsia:"
    echo "  cargo build --target x86_64-unknown-fuchsia --features fuchsia"
    echo
}

# Main execution
main() {
    echo
    info "=========================================="
    info "Fuchsia Emulator Setup for Blinc"
    info "=========================================="
    echo

    check_sdk
    check_virtualization

    # Download product bundle if not already present
    download_product_bundle || {
        warning "Product bundle download had issues"
        warning "You may need to manually set up the emulator"
    }

    configure_ffx

    if [[ "$START_EMU" == "true" ]]; then
        start_emulator
    fi

    show_usage_instructions

    success "Setup complete!"
}

main "$@"
