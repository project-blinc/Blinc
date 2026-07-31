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
//! - [`scroll()`] - Scrollable container with optional bounce physics
//! - [`code()`] - Code block with syntax highlighting and line numbers
//!
//! # Example
//!
//! ```ignore
//! use blinc_layout::prelude::*;
//!
//! fn my_form(ctx: &Context) -> impl ElementBuilder {
//!     let username = ctx.use_state_for("username", || text_input_state());
//!     let remember = ctx.use_state_keyed("remember", || false);
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

/// Handler for a widget that reports its text as it changes.
///
/// Shared by the text-bearing widgets so the `Arc<dyn Fn>` shape is
/// spelled once.
pub type TextCallback = std::sync::Arc<dyn Fn(&str) + Send + Sync>;

/// Handler that inspects text and reports a verdict, as in a paste
/// override deciding whether it consumed the input.
pub type TextPredicate = std::sync::Arc<dyn Fn(&str) -> bool + Send + Sync>;

pub mod blockquote;
pub mod button;
pub mod checkbox;
pub mod code;
pub mod cursor;
pub mod gesture;
pub mod hr;
pub mod input_otp;
pub mod link;
pub mod list;
#[cfg(feature = "media")]
pub mod media;
pub mod number_input;
pub mod overlay;
pub mod overlay_stack;
pub mod radio;
pub mod rich_text_editor;
pub mod scroll;
pub mod table;
pub mod text_area;
pub mod text_edit;
pub mod text_input;
pub mod toast_tray;
pub mod toggle;
pub mod virtual_list;

// Re-export button widget
pub use button::{Button, ButtonConfig, ButtonVisualState, button, button_with};

// Re-export checkbox widget
pub use checkbox::{Checkbox, CheckboxBuilder, CheckboxConfig, checkbox, checkbox_labeled};

// Re-export radio widget
pub use radio::{RadioGroup, RadioGroupBuilder, RadioGroupConfig, RadioLayout, radio_group};

// Re-export toggle widget
pub use toggle::{Toggle, ToggleBuilder, ToggleConfig, toggle};

// Re-export number_input widget
pub use number_input::{
    NumberInput, NumberInputBuilder, NumberInputConfig, number_input, step_down, step_up,
};

pub use input_otp::{InputOtp, InputOtpBuilder, InputOtpConfig, input_otp};

// Re-export text input widget
pub use text_input::{
    CURSOR_BLINK_INTERVAL_MS,
    InputConstraints,
    InputType,
    SharedTextInputState,
    TextInput,
    TextInputConfig,
    TextInputState,
    // Blur function for click-outside handling
    blur_all_text_inputs,
    // Cursor blink timing utilities
    elapsed_ms,
    // Programmatic focus
    focus_text_input,
    // Deferred focus — enqueue + per-frame drain. See doc on
    // focus_text_input_deferred for the timing rationale.
    focus_text_input_deferred,
    has_focused_text_input,
    process_pending_area_focus,
    process_pending_input_focus,
    request_css_reparse,
    // Rebuild/relayout request functions
    request_full_rebuild,
    request_rebuild,
    // Continuous redraw callback for animation scheduler integration
    set_continuous_redraw_callback,
    take_needs_continuous_redraw,
    take_needs_css_reparse,
    take_needs_rebuild,
    take_needs_relayout,
    text_input,
    text_input_state,
    text_input_state_with_placeholder,
};

// Re-export text area widget
pub use text_area::{
    SharedTextAreaState, TextArea, TextAreaConfig, TextAreaState, TextPosition, focus_text_area,
    focus_text_area_deferred, text_area, text_area_state, text_area_state_with_placeholder,
};

// Re-export scroll widget
pub use scroll::{
    Scroll, ScrollConfig, ScrollDirection, ScrollPhysics, ScrollRenderInfo, ScrollbarConfig,
    ScrollbarRenderInfo, ScrollbarSize, ScrollbarState, ScrollbarVisibility, SharedScrollPhysics,
    scroll, scroll_bouncy, scroll_no_bounce,
};

// Re-export cursor widget (canvas-based smooth cursor)
pub use cursor::{
    CursorAnimation, CursorState, SharedCursorState, cursor_canvas, cursor_canvas_absolute,
    cursor_state,
};

// Re-export code widget
pub use code::{
    Code, CodeConfig, CodeEditor, CodeEditorData, SharedCodeEditorState, code, code_editor,
    code_editor_state, code_minimap, pre,
};

// Re-export overlay widget
pub use overlay::{
    BackdropConfig, ContextMenuBuilder, Corner, DialogBuilder, DropdownBuilder, ModalBuilder,
    OverlayAnimation, OverlayConfig, OverlayHandle, OverlayKind, OverlayManager, OverlayManagerExt,
    OverlayPosition, OverlayState, ToastBuilder, overlay_events, overlay_manager,
};

// Re-export table widget
pub use table::{
    TableBuilder, TableCell, cell, striped_tr, table, tbody, td, td_text, tfoot, th, th_text,
    thead, tr,
};

// Re-export blockquote widget
pub use blockquote::{Blockquote, BlockquoteConfig, blockquote, blockquote_with_config};

// Re-export horizontal rule widget
pub use hr::{HrConfig, hr, hr_color, hr_thick, hr_with_bg, hr_with_config};

// Re-export link widget
pub use link::{Link, LinkConfig, link, open_url};

// Re-export list widgets
pub use list::{
    ListConfig, ListItem, ListMarker, OrderedList, TaskListItem, UnorderedList, li, ol, ol_start,
    ol_start_with_config, ol_with_config, task_item, task_item_with_config, ul, ul_with_config,
};
