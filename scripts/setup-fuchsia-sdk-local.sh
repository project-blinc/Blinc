#!/usr/bin/env bash
#
# setup-fuchsia-sdk-local.sh - Download Fuchsia SDK to project directory
#
# Downloads the Fuchsia SDK Core to vendor/fuchsia-sdk/ for reproducible builds.
# This avoids relying on Bazel cache in temp directories.
#
# Usage:
#   ./scripts/setup-fuchsia-sdk-local.sh
#
# Output:
#   vendor/fuchsia-sdk/  - Contains SDK tools, FIDL definitions, sysroot

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
SDK_DIR="$PROJECT_ROOT/vendor/fuchsia-sdk"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

info()    { echo -e "${BLUE}[INFO]${NC} $*"; }
success() { echo -e "${GREEN}[SUCCESS]${NC} $*"; }
warning() { echo -e "${YELLOW}[WARNING]${NC} $*"; }
error()   { echo -e "${RED}[ERROR]${NC} $*" >&2; }

# Detect platform
detect_platform() {
    local os=""
    local arch=""

    case "$(uname -s)" in
        Darwin) os="mac" ;;
        Linux)  os="linux" ;;
        *)
            error "Unsupported OS: $(uname -s)"
            exit 1
            ;;
    esac

    case "$(uname -m)" in
        x86_64|amd64) arch="amd64" ;;
        arm64|aarch64) arch="arm64" ;;
        *)
            error "Unsupported architecture: $(uname -m)"
            exit 1
            ;;
    esac

    echo "${os}-${arch}"
}

PLATFORM=$(detect_platform)
info "Detected platform: $PLATFORM"

# SDK download URL (CIPD - Chrome Infrastructure Package Deployment)
# Latest stable SDK
SDK_VERSION="latest"
SDK_PACKAGE="fuchsia/sdk/core/${PLATFORM}"

# Check if already downloaded
if [[ -d "$SDK_DIR/tools" ]]; then
    info "SDK already exists at $SDK_DIR"
    read -p "Re-download? [y/N] " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        success "Using existing SDK"
        exit 0
    fi
    rm -rf "$SDK_DIR"
fi

# Create vendor directory
mkdir -p "$PROJECT_ROOT/vendor"

# Method 1: Try using cipd if available
if command -v cipd &> /dev/null; then
    info "Using CIPD to download SDK..."
    cipd install "$SDK_PACKAGE" "$SDK_VERSION" -root "$SDK_DIR" && {
        success "SDK downloaded via CIPD"
        exit 0
    }
fi

# Method 2: Direct download from Chrome Infra
info "Downloading SDK directly..."

# Get the latest version info
CIPD_URL="https://chrome-infra-packages.appspot.com"

# For mac-arm64, we may need mac-amd64 (Rosetta)
DOWNLOAD_PLATFORM="$PLATFORM"
if [[ "$PLATFORM" == "mac-arm64" ]]; then
    warning "Note: Fuchsia SDK for mac-arm64 may use Rosetta for some tools"
    # Try arm64 first, fall back to amd64
fi

# Construct download URL
# Format: https://chrome-infra-packages.appspot.com/dl/fuchsia/sdk/core/mac-amd64/+/latest
DOWNLOAD_URL="${CIPD_URL}/dl/${SDK_PACKAGE}/+/${SDK_VERSION}"

info "Download URL: $DOWNLOAD_URL"

# Create temp file
TEMP_ZIP=$(mktemp)
trap "rm -f $TEMP_ZIP" EXIT

# Download
info "Downloading SDK (this may take a few minutes)..."
if command -v curl &> /dev/null; then
    curl -L -o "$TEMP_ZIP" "$DOWNLOAD_URL" || {
        error "Download failed"
        exit 1
    }
elif command -v wget &> /dev/null; then
    wget -O "$TEMP_ZIP" "$DOWNLOAD_URL" || {
        error "Download failed"
        exit 1
    }
else
    error "curl or wget required"
    exit 1
fi

# Extract
info "Extracting SDK..."
mkdir -p "$SDK_DIR"
unzip -q "$TEMP_ZIP" -d "$SDK_DIR" || {
    error "Extraction failed"
    exit 1
}

# Verify
if [[ -d "$SDK_DIR/tools" ]]; then
    success "SDK extracted to $SDK_DIR"
else
    error "SDK extraction incomplete"
    exit 1
fi

# Make tools executable
chmod +x "$SDK_DIR/tools/"* 2>/dev/null || true
if [[ -d "$SDK_DIR/tools/x64" ]]; then
    chmod +x "$SDK_DIR/tools/x64/"* 2>/dev/null || true
fi
if [[ -d "$SDK_DIR/tools/arm64" ]]; then
    chmod +x "$SDK_DIR/tools/arm64/"* 2>/dev/null || true
fi

# Create .gitignore for vendor
cat > "$PROJECT_ROOT/vendor/.gitignore" << 'EOF'
# Fuchsia SDK is large (~2GB), download via setup script
fuchsia-sdk/
EOF

# Summary
echo ""
success "=========================================="
success "Fuchsia SDK Setup Complete!"
success "=========================================="
echo ""
echo "Location: $SDK_DIR"
echo ""
echo "Tools available:"
ls "$SDK_DIR/tools/x64/" 2>/dev/null | head -10 || ls "$SDK_DIR/tools/" | head -10
echo ""
echo "FIDL libraries:"
ls "$SDK_DIR/fidl/" 2>/dev/null | wc -l | xargs echo "  Count:"
echo ""
echo "Next steps:"
echo "  1. Generate FIDL bindings: ./scripts/generate-fuchsia-fidl.sh"
echo "  2. Build for Fuchsia: cargo build --target x86_64-unknown-fuchsia"
echo ""
