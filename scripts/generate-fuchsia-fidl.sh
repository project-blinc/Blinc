#!/usr/bin/env bash
#
# generate-fuchsia-fidl.sh - Generate Rust FIDL bindings from Fuchsia SDK
#
# Generates Rust bindings for each FIDL library individually.
#
# Output: extensions/blinc_fuchsia_bindings/src/

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
OUTPUT_DIR="$PROJECT_ROOT/extensions/blinc_fuchsia_bindings/src"

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

# Find SDK
SDK_PATH="$PROJECT_ROOT/vendor/fuchsia-sdk"
if [[ ! -d "$SDK_PATH/tools/x64" ]]; then
    error "Fuchsia SDK not found at vendor/fuchsia-sdk/"
    exit 1
fi

FIDLC="$SDK_PATH/tools/x64/fidlc"
FIDLGEN_RUST="$SDK_PATH/tools/x64/fidlgen_rust"
FIDL_DIR="$SDK_PATH/fidl"

info "Using SDK at: $SDK_PATH"

# Create output directory
mkdir -p "$OUTPUT_DIR"

# Temp directory
TEMP_DIR=$(mktemp -d)
trap "rm -rf $TEMP_DIR" EXIT

# Libraries to generate (standalone ones that don't need complex deps)
LIBS=(
    "fuchsia.math"
    "fuchsia.ui.types"
)

# Track success
declare -a GENERATED=()

# Generate bindings for each library
for lib in "${LIBS[@]}"; do
    lib_dir="$FIDL_DIR/$lib"
    if [[ ! -d "$lib_dir" ]]; then
        warning "Library not found: $lib"
        continue
    fi

    info "Processing $lib..."

    # Find FIDL files
    fidl_files=()
    while IFS= read -r -d '' file; do
        fidl_files+=("$file")
    done < <(find "$lib_dir" -name "*.fidl" -print0 2>/dev/null)

    if [[ ${#fidl_files[@]} -eq 0 ]]; then
        warning "No FIDL files in $lib"
        continue
    fi

    # Compile to JSON IR
    ir_file="$TEMP_DIR/${lib//\./_}.json"
    if ! "$FIDLC" \
        --available fuchsia:HEAD \
        --json "$ir_file" \
        --files "${fidl_files[@]}" \
        2>/dev/null; then
        warning "Failed to compile $lib"
        continue
    fi

    # Generate Rust
    rust_name="fidl_${lib//\./_}"
    rust_file="$OUTPUT_DIR/${rust_name}.rs"

    if ! "$FIDLGEN_RUST" \
        -json "$ir_file" \
        -output-filename "$rust_file" \
        2>/dev/null; then
        warning "Failed to generate Rust for $lib"
        continue
    fi

    success "Generated $rust_name.rs"
    GENERATED+=("$rust_name")
done

# Create lib.rs
cat > "$OUTPUT_DIR/lib.rs" << 'HEADER'
//! Fuchsia FIDL Bindings for Blinc
//!
//! Auto-generated from Fuchsia SDK FIDL definitions.
//! Regenerate with: `./scripts/generate-fuchsia-fidl.sh`
//!
//! Note: The full bindings require Fuchsia-specific crates (fidl, fuchsia-zircon).
//! On non-Fuchsia platforms, stub types are provided.

#![allow(dead_code)]
#![allow(unused_imports)]
#![allow(unused_variables)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(clippy::all)]

HEADER

# Add cfg-gated modules for generated bindings
for mod in "${GENERATED[@]}"; do
    echo "#[cfg(target_os = \"fuchsia\")]" >> "$OUTPUT_DIR/lib.rs"
    echo "pub mod ${mod};" >> "$OUTPUT_DIR/lib.rs"
    echo "" >> "$OUTPUT_DIR/lib.rs"
done

# Add stubs for non-Fuchsia
cat >> "$OUTPUT_DIR/lib.rs" << 'STUBS'
// Stub types for non-Fuchsia platforms
#[cfg(not(target_os = "fuchsia"))]
pub mod fuchsia_math {
    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    pub struct SizeU { pub width: u32, pub height: u32 }

    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    pub struct Size { pub width: i32, pub height: i32 }

    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    pub struct SizeF { pub width: f32, pub height: f32 }

    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    pub struct Vec_ { pub x: i32, pub y: i32 }

    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    pub struct VecF { pub x: f32, pub y: f32 }

    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    pub struct Rect { pub x: i32, pub y: i32, pub width: i32, pub height: i32 }

    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    pub struct RectF { pub x: f32, pub y: f32, pub width: f32, pub height: f32 }

    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    pub struct RectU { pub x: u32, pub y: u32, pub width: u32, pub height: u32 }

    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    pub struct Inset { pub top: i32, pub right: i32, pub bottom: i32, pub left: i32 }

    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    pub struct InsetF { pub top: f32, pub right: f32, pub bottom: f32, pub left: f32 }
}

#[cfg(not(target_os = "fuchsia"))]
pub mod fuchsia_ui_pointer {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum TouchInteractionStatus { Granted, Denied }

    #[derive(Clone, Copy, Debug, Default)]
    pub struct TouchPointerSample {
        pub position_in_viewport: Option<[f32; 2]>,
        pub interaction: Option<TouchInteractionId>,
        pub phase: Option<EventPhase>,
    }

    #[derive(Clone, Copy, Debug, Default)]
    pub struct TouchInteractionId {
        pub device_id: u32,
        pub pointer_id: u32,
        pub interaction_id: u32,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum EventPhase { Add, Change, Remove, Cancel }

    #[derive(Clone, Debug, Default)]
    pub struct TouchEvent {
        pub timestamp: Option<i64>,
        pub pointer_sample: Option<TouchPointerSample>,
        pub interaction_result: Option<TouchInteractionResult>,
    }

    #[derive(Clone, Copy, Debug, Default)]
    pub struct TouchInteractionResult {
        pub interaction: TouchInteractionId,
        pub status: Option<TouchInteractionStatus>,
    }

    #[derive(Clone, Debug, Default)]
    pub struct MouseEvent {
        pub timestamp: Option<i64>,
        pub pointer_sample: Option<MousePointerSample>,
    }

    #[derive(Clone, Copy, Debug, Default)]
    pub struct MousePointerSample {
        pub device_id: Option<u32>,
        pub position_in_viewport: Option<[f32; 2]>,
        pub pressed_buttons: Option<Vec<u8>>,
    }
}

#[cfg(not(target_os = "fuchsia"))]
pub mod fuchsia_ui_input3 {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum KeyEventType { Pressed, Released, Cancel, Sync }

    #[derive(Clone, Debug, Default)]
    pub struct KeyEvent {
        pub timestamp: Option<i64>,
        pub type_: Option<KeyEventType>,
        pub key: Option<Key>,
        pub modifiers: Option<Modifiers>,
    }

    pub type Key = u32;
    pub type Modifiers = u32;

    pub const MODIFIERS_SHIFT: u32 = 1;
    pub const MODIFIERS_CTRL: u32 = 2;
    pub const MODIFIERS_ALT: u32 = 4;
    pub const MODIFIERS_META: u32 = 8;
}

#[cfg(not(target_os = "fuchsia"))]
pub mod fuchsia_ui_views {
    #[derive(Clone, Debug)]
    pub struct ViewRef {
        pub reference: Option<()>,  // Placeholder for zx::EventPair
    }

    #[derive(Clone, Debug)]
    pub struct ViewRefControl {
        pub reference: Option<()>,
    }

    #[derive(Clone, Debug)]
    pub struct ViewIdentityOnCreation {
        pub view_ref: ViewRef,
        pub view_ref_control: ViewRefControl,
    }
}

#[cfg(not(target_os = "fuchsia"))]
pub mod fuchsia_ui_composition {
    use super::fuchsia_math::*;

    pub type ContentId = u64;
    pub type TransformId = u64;

    #[derive(Clone, Debug, Default)]
    pub struct LayoutInfo {
        pub logical_size: Option<SizeU>,
        pub device_pixel_ratio: Option<SizeF>,
    }
}

// Re-export stubs at crate level for convenience
#[cfg(not(target_os = "fuchsia"))]
pub use fuchsia_math::*;
STUBS

echo ""
success "=========================================="
success "Generation Complete!"
success "=========================================="
echo ""
echo "Generated ${#GENERATED[@]} module(s):"
for mod in "${GENERATED[@]}"; do
    echo "  - $mod"
done
echo ""
echo "Stub types provided for non-Fuchsia builds."
echo ""
echo "Next: cargo check -p blinc_fuchsia_bindings"
