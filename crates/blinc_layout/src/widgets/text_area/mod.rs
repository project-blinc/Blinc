//! Ready-to-use TextArea widget
//!
//! Multi-line text area with:
//! - Multi-line text editing
//! - Row/column sizing (like HTML textarea)
//! - Cursor and selection
//! - Visual states: idle, hovered, focused
//! - Built-in styling that just works
//! - Built once: what it shows is bound to its state and patched in place

mod config;
mod content;
mod css;
mod edit;
mod focus;
mod handlers;
mod lines;
mod state;
mod widget;

#[cfg(test)]
mod tests;

pub use config::TextAreaConfig;
pub(crate) use focus::notify_text_area;
pub use focus::{focus_text_area, focus_text_area_deferred, refresh_text_area};
pub use state::{
    SharedTextAreaState, TextAreaState, TextPosition, VisualLine, text_area_state,
    text_area_state_with_placeholder,
};
pub use widget::{TextArea, text_area};

use content::*;
use css::*;
use focus::*;
use lines::*;

// Shared by the submodules, which take them with `use super::*`.
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};

use blinc_core::Color;
use blinc_core::reactive::SignalId;
use blinc_theme::{ColorToken, ThemeState};

use crate::canvas::canvas;
use crate::css_parser::{ElementState, Stylesheet, active_stylesheet};
use crate::div::{Div, ElementBuilder, div};
use crate::element::RenderProps;
use crate::stateful::{StateTransitions, TextFieldState};
use crate::tree::{LayoutNodeId, LayoutTree};
use crate::widgets::cursor::{CursorAnimation, SharedCursorState, cursor_state};
use crate::widgets::scroll::{Scroll, ScrollDirection, ScrollPhysics, SharedScrollPhysics};
use crate::widgets::text_input::{
    elapsed_ms, increment_focus_count, request_continuous_redraw_pub, set_focused_text_area,
};
