#!/usr/bin/env bash
#
# setup-fuchsia-all.sh - Complete Fuchsia Development Environment Setup
#
# This master script runs all Fuchsia setup steps in sequence:
# 1. SDK installation and configuration
# 2. Emulator setup (optional)
# 3. Tool verification
#
# Usage:
#   ./scripts/setup-fuchsia-all.sh [OPTIONS]
#
# Options:
#   --no-emulator    Skip emulator setup (SDK only)
#   --verify-only    Only run verification, skip setup
#   --help           Show this help
#
# After running this script, you can:
#   1. Build Blinc for Fuchsia:
#      cargo build --target x86_64-unknown-fuchsia
#
#   2. Run in emulator:
#      ffx emu start workbench_eng.x64 --headless
#      ffx component run fuchsia-pkg://fuchsia.com/your_app#meta/your_app.cm

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m'

info()    { echo -e "${BLUE}[INFO]${NC} $*"; }
success() { echo -e "${GREEN}[SUCCESS]${NC} $*"; }
warning() { echo -e "${YELLOW}[WARNING]${NC} $*"; }
error()   { echo -e "${RED}[ERROR]${NC} $*" >&2; }
step()    { echo -e "${CYAN}[STEP]${NC} $*"; }

# Options
SETUP_EMULATOR=true
VERIFY_ONLY=false

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --no-emulator)
            SETUP_EMULATOR=false
            shift
            ;;
        --verify-only)
            VERIFY_ONLY=true
            shift
            ;;
        --help|-h)
            head -25 "$0" | tail -18
            exit 0
            ;;
        *)
            error "Unknown option: $1"
            exit 1
            ;;
    esac
done

print_banner() {
    echo ""
    echo -e "${CYAN}╔═══════════════════════════════════════════════════════════╗${NC}"
    echo -e "${CYAN}║       Blinc Fuchsia Development Environment Setup         ║${NC}"
    echo -e "${CYAN}╚═══════════════════════════════════════════════════════════╝${NC}"
    echo ""
}

run_step() {
    local name=$1
    local script=$2
    local args=${3:-}

    echo ""
    echo -e "${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    step "$name"
    echo -e "${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo ""

    if [[ -x "$SCRIPT_DIR/$script" ]]; then
        "$SCRIPT_DIR/$script" $args
    else
        error "Script not found: $SCRIPT_DIR/$script"
        return 1
    fi
}

setup_sdk() {
    run_step "Step 1/3: Installing Fuchsia SDK" "setup-fuchsia-sdk.sh"
}

setup_emulator() {
    if [[ "$SETUP_EMULATOR" == "true" ]]; then
        run_step "Step 2/3: Setting up Fuchsia Emulator" "setup-fuchsia-emulator.sh" "--no-start"
    else
        info "Skipping emulator setup (--no-emulator specified)"
    fi
}

verify_tools() {
    run_step "Step 3/3: Verifying Installation" "verify-fuchsia-tools.sh"
}

print_summary() {
    echo ""
    echo -e "${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${GREEN}Setup Complete!${NC}"
    echo -e "${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo ""
    echo -e "${BLUE}Quick Start:${NC}"
    echo ""
    echo "  1. Reload your shell environment:"
    echo "     source ~/.zshrc  # or ~/.bashrc"
    echo ""
    echo "  2. Build Blinc for Fuchsia:"
    echo "     cargo build --target x86_64-unknown-fuchsia"
    echo ""
    if [[ "$SETUP_EMULATOR" == "true" ]]; then
        echo "  3. Start the Fuchsia emulator:"
        echo "     ffx emu start workbench_eng.x64 --headless"
        echo ""
        echo "  4. Run your app:"
        echo "     ffx component run fuchsia-pkg://fuchsia.com/your_app#meta/your_app.cm"
        echo ""
    fi
    echo -e "${BLUE}Documentation:${NC}"
    echo "  - Fuchsia SDK: https://fuchsia.dev/fuchsia-src/development/sdk"
    echo "  - Blinc Fuchsia: docs/plans/fuchsia-integration-gaps.md"
    echo ""
}

main() {
    print_banner

    if [[ "$VERIFY_ONLY" == "true" ]]; then
        verify_tools
        exit 0
    fi

    local start_time=$(date +%s)

    # Run setup steps
    setup_sdk
    setup_emulator
    verify_tools

    local end_time=$(date +%s)
    local duration=$((end_time - start_time))

    print_summary

    info "Total setup time: ${duration}s"
}

main "$@"
