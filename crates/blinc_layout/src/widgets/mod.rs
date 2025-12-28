//! Ready-to-use widgets with built-in styling and behavior
//!
//! This module provides production-ready widgets that work out of the box
//! in the fluent layout API - no `.build()` required!
//!
//! # Widgets
//!
//! - [`button()`] - Clickable button with hover/press states
//! - [`checkbox()`] - Toggle checkbox with label support
//! - [`text_input()`] - Single-line text input with validation
//! - [`text_area()`] - Multi-line text area
//! - [`scroll()`] - Scrollable container with bounce physics
//! - [`code()`] - Code block with syntax highlighting and line numbers
//!
//! # Example
//!
//! ```ignore
//! use blinc_layout::prelude::*;
//!
//! fn my_form(ctx: &Context) -> impl ElementBuilder {
//!     let username = ctx.use_state_for("username", || text_input_state());
//!     let remember = ctx.use_state_for("remember", || checkbox_state(false));
//!
//!     div().flex_col().gap(16.0)
//!         // Text input - just works!
//!         .child(text_input(&username).placeholder("Username").w(280.0))
//!         // Checkbox - just works!
//!         .child(checkbox(&remember).label("Remember me"))
//!         // Button - just works!
//!         .child(button("Submit").on_click(|_| println!("Submitted!")))
//! }
//! ```

pub mod button;
pub mod checkbox;
pub mod code;
pub mod cursor;
pub mod overlay;
pub mod scroll;
pub mod text_area;
pub mod text_input;

// Re-export button widget
pub use button::{button, button_with, Button, ButtonConfig, ButtonVisualState};

// Re-export checkbox widget
pub use checkbox::{
    checkbox, checkbox_labeled, checkbox_state, Checkbox, CheckboxConfig, CheckboxState,
    SharedCheckboxState,
};

// Re-export text input widget
pub use text_input::{
    // Cursor blink timing utilities
    elapsed_ms,
    has_focused_text_input,
    request_rebuild,
    take_needs_continuous_redraw,
    take_needs_rebuild,
    text_input,
    text_input_state,
    text_input_state_with_placeholder,
    InputType,
    NumberConstraints,
    SharedTextInputState,
    TextInput,
    TextInputConfig,
    TextInputState,
    CURSOR_BLINK_INTERVAL_MS,
};

// Re-export text area widget
pub use text_area::{
    text_area, text_area_state, text_area_state_with_placeholder, SharedTextAreaState, TextArea,
    TextAreaConfig, TextAreaState, TextPosition,
};

// Re-export scroll widget
pub use scroll::{
    scroll, scroll_no_bounce, Scroll, ScrollConfig, ScrollDirection, ScrollPhysics,
    ScrollRenderInfo, SharedScrollPhysics,
};

// Re-export cursor widget (canvas-based smooth cursor)
pub use cursor::{
    cursor_canvas, cursor_canvas_absolute, cursor_state, CursorAnimation, CursorState,
    SharedCursorState,
};

// Re-export code widget
pub use code::{code, pre, Code, CodeConfig};

// Re-export overlay widget
pub use overlay::{
    overlay_events, overlay_manager, BackdropConfig, Corner, ContextMenuBuilder, DialogBuilder,
    DropdownBuilder, ModalBuilder, OverlayAnimation, OverlayConfig, OverlayHandle, OverlayKind,
    OverlayManager, OverlayManagerExt, OverlayPosition, OverlayState, ToastBuilder,
};
