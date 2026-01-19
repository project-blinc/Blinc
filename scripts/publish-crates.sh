#!/bin/bash
# Script to publish all Blinc crates to crates.io in dependency order
# Usage: ./scripts/publish-crates.sh
#
# Note: crates.io has a rate limit of ~1 new crate per 10 minutes for new publishers
# This script waits between publishes to respect the rate limit

set -e

# Source the cargo registry token
if [ -f ".env.cargo" ]; then
    source .env.cargo
fi

WAIT_TIME=610  # 10 minutes + buffer

# Publish order (respects dependency graph)
# Note: blinc_core has blinc_animation as dev-dep only, so core goes first
PHASE1=(blinc_macros blinc_platform blinc_icons blinc_core)
PHASE2=(blinc_animation blinc_paint blinc_svg blinc_text)
PHASE3=(blinc_theme blinc_image blinc_gpu)
PHASE4=(blinc_layout blinc_cn)
PHASE5=(blinc_platform_desktop blinc_platform_android blinc_platform_ios)
PHASE6=(blinc_app)
PHASE7=(blinc_cli)

publish_crate() {
    local crate=$1
    echo ""
    echo "=========================================="
    echo "Publishing $crate..."
    echo "=========================================="

    if cargo publish -p "$crate" 2>&1; then
        echo "Successfully published $crate"
        return 0
    else
        echo "Failed to publish $crate"
        return 1
    fi
}

wait_for_rate_limit() {
    echo ""
    echo "Waiting $WAIT_TIME seconds for rate limit..."
    sleep $WAIT_TIME
}

publish_phase() {
    local phase_name=$1
    shift
    local crates=("$@")

    echo ""
    echo "############################################"
    echo "# $phase_name"
    echo "############################################"

    for crate in "${crates[@]}"; do
        publish_crate "$crate"
        wait_for_rate_limit
    done
}

echo "Starting Blinc crate publishing..."
echo "This will take approximately $((($WAIT_TIME * 17) / 60)) minutes due to rate limiting."
echo ""

# Check if CARGO_REGISTRY_TOKEN is set
if [ -z "$CARGO_REGISTRY_TOKEN" ]; then
    echo "Error: CARGO_REGISTRY_TOKEN not set"
    echo "Please set it in .env.cargo or export it"
    exit 1
fi

# Start publishing
publish_phase "Phase 1: Foundation crates" "${PHASE1[@]}"
publish_phase "Phase 2: Core systems" "${PHASE2[@]}"
publish_phase "Phase 3: Higher-level systems" "${PHASE3[@]}"
publish_phase "Phase 4: GPU and components" "${PHASE4[@]}"
publish_phase "Phase 5: Platform extensions" "${PHASE5[@]}"
publish_phase "Phase 6: Application framework" "${PHASE6[@]}"
publish_phase "Phase 7: CLI" "${PHASE7[@]}"

echo ""
echo "=============================================="
echo "All crates published successfully!"
echo "=============================================="
