//! Ready-to-use TextInput widget
//!
//! Single-line text input with:
//! - Visual states: idle, hovered, focused
//! - A caret drawn by a canvas that reads the field each paint
//! - Built once: what it shows is bound to a revision signal, so an edit
//!   patches the text, colours and scroll in place
//!
//! # Example
//!
//! ```ignore
//! let input_data = text_input_data_with_placeholder("Enter username");
//! text_input(&input_data)
//!     .w(280.0)
//!     .rounded(12.0)
//! ```

mod config;
mod content;
mod css;
mod data;
mod edit;
mod focus;
mod handlers;
mod long_press;
mod runtime;
mod widget;

#[cfg(test)]
mod tests;

pub use config::*;
use content::*;
use css::*;
pub use data::*;
pub use focus::*;
pub use long_press::*;
pub use runtime::*;
pub use widget::*;

// Shared by the submodules, which take them with `use super::*`.
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, Weak};

use blinc_core::Color;
use blinc_theme::{ColorToken, ThemeState};

use crate::canvas::canvas;
use crate::css_parser::{ElementState, Stylesheet, active_stylesheet};
use crate::div::{Div, ElementBuilder, div};
use crate::element::RenderProps;
use crate::stateful::{StateTransitions, TextFieldState};
use crate::text_selection::{SelectionSource, clear_selection, set_selection};
use crate::tree::{LayoutNodeId, LayoutTree};
use crate::widgets::cursor::{CursorAnimation, SharedCursorState, cursor_state};
