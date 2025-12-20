#!/bin/bash
# Blinc CLI installer script
# Usage: curl -fsSL https://raw.githubusercontent.com/project-blinc/blinc/main/scripts/install.sh | bash

set -e

REPO="project-blinc/blinc"
INSTALL_DIR="${BLINC_INSTALL_DIR:-/usr/local/bin}"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

info() {
    echo -e "${CYAN}$1${NC}"
}

success() {
    echo -e "${GREEN}$1${NC}"
}

warn() {
    echo -e "${YELLOW}$1${NC}"
}

error() {
    echo -e "${RED}$1${NC}"
    exit 1
}

# Detect OS and architecture
detect_platform() {
    OS="$(uname -s)"
    ARCH="$(uname -m)"

    case "$OS" in
        Linux)
            case "$ARCH" in
                x86_64)
                    PLATFORM="x86_64-unknown-linux-gnu"
                    ;;
                aarch64|arm64)
                    PLATFORM="aarch64-unknown-linux-gnu"
                    ;;
                *)
                    error "Unsupported architecture: $ARCH"
                    ;;
            esac
            ;;
        Darwin)
            case "$ARCH" in
                x86_64)
                    PLATFORM="x86_64-apple-darwin"
                    ;;
                arm64)
                    PLATFORM="aarch64-apple-darwin"
                    ;;
                *)
                    error "Unsupported architecture: $ARCH"
                    ;;
            esac
            ;;
        *)
            error "Unsupported OS: $OS. Use Windows installer or build from source."
            ;;
    esac
}

# Get latest release version
get_latest_version() {
    if command -v curl &> /dev/null; then
        VERSION=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')
    elif command -v wget &> /dev/null; then
        VERSION=$(wget -qO- "https://api.github.com/repos/$REPO/releases/latest" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')
    else
        error "Neither curl nor wget found. Please install one of them."
    fi

    if [ -z "$VERSION" ]; then
        error "Could not determine latest version. Check your internet connection."
    fi
}

# Download and install
install() {
    info "Detected platform: $PLATFORM"
    info "Installing Blinc CLI $VERSION..."

    DOWNLOAD_URL="https://github.com/$REPO/releases/download/$VERSION/blinc-$PLATFORM.tar.gz"
    TMP_DIR=$(mktemp -d)
    trap "rm -rf $TMP_DIR" EXIT

    info "Downloading from $DOWNLOAD_URL..."

    if command -v curl &> /dev/null; then
        curl -fsSL "$DOWNLOAD_URL" -o "$TMP_DIR/blinc.tar.gz"
    else
        wget -q "$DOWNLOAD_URL" -O "$TMP_DIR/blinc.tar.gz"
    fi

    info "Extracting..."
    tar -xzf "$TMP_DIR/blinc.tar.gz" -C "$TMP_DIR"

    info "Installing to $INSTALL_DIR..."
    if [ -w "$INSTALL_DIR" ]; then
        mv "$TMP_DIR/blinc" "$INSTALL_DIR/"
    else
        warn "Need sudo to install to $INSTALL_DIR"
        sudo mv "$TMP_DIR/blinc" "$INSTALL_DIR/"
    fi

    chmod +x "$INSTALL_DIR/blinc"
}

# Verify installation
verify() {
    if command -v blinc &> /dev/null; then
        success ""
        success "✓ Blinc CLI installed successfully!"
        echo ""
        blinc --version
        echo ""
        info "Run 'blinc doctor' to check your development environment."
    else
        warn ""
        warn "Blinc installed to $INSTALL_DIR/blinc"
        warn "Make sure $INSTALL_DIR is in your PATH."
        echo ""
        echo "Add this to your shell profile:"
        echo "  export PATH=\"\$PATH:$INSTALL_DIR\""
    fi
}

main() {
    echo ""
    info "╔════════════════════════════════════════╗"
    info "║         Blinc CLI Installer            ║"
    info "╚════════════════════════════════════════╝"
    echo ""

    detect_platform
    get_latest_version
    install
    verify
}

main
