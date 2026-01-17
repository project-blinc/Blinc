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

#[cfg(target_os = "fuchsia")]
pub mod fidl_fuchsia_math;

#[cfg(target_os = "fuchsia")]
pub mod fidl_fuchsia_ui_types;

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

    #[derive(Clone, Debug, Default)]
    pub struct MousePointerSample {
        pub device_id: Option<u32>,
        pub position_in_viewport: Option<[f32; 2]>,
        pub pressed_buttons: Vec<u8>,
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
