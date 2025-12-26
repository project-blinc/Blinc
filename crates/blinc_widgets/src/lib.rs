//! Blinc Widget Library
//!
//! Core UI components with FSM-driven interactions and reactive state.
//!
//! # Architecture
//!
//! The widget system is built on three pillars:
//!
//! 1. **FSM-Driven Interactions**: Each widget has a state machine that manages
//!    its interaction states (idle, hovered, pressed, etc.). FSM transitions
//!    trigger visual updates and callbacks.
//!
//! 2. **Reactive Signals**: Widget state is managed through reactive signals
//!    that automatically trigger re-renders when changed.
//!
//! 3. **Dirty Tracking**: Instead of rebuilding the entire UI tree on every
//!    frame, widgets are tracked as "dirty" when their state changes,
//!    enabling incremental updates.
//!
//! # Example
//!
//! ```ignore
//! use blinc_widgets::prelude::*;
//!
//! let mut ctx = WidgetContext::new();
//!
//! // Create a button with FSM-driven interactions
//! let button = button("Click me")
//!     .on_click(|| println!("Button clicked!"))
//!     .build(&mut ctx);
//!
//! // Handle events (from platform)
//! button.handle_event(&mut ctx, &event);
//!
//! // Update animations
//! button.update(&mut ctx, dt);
//!
//! // Build UI only for dirty widgets
//! if ctx.is_dirty(button.id()) {
//!     let ui = button.build(&ctx);
//!     // render ui...
//! }
//! ```

pub mod button;
pub mod checkbox;
pub mod container;
pub mod context;
pub mod text;
pub mod text_area;
pub mod text_input;
pub mod widget;

pub use button::{button, Button, ButtonBuilder, ButtonConfig, ButtonState};
pub use checkbox::{checkbox, Checkbox, CheckboxBuilder, CheckboxConfig, CheckboxState};
pub use context::{DirtyTracker, WidgetContext, WidgetContextExt, WidgetState};
pub use text_area::{
    text_area, TextArea, TextAreaBuilder, TextAreaConfig, TextAreaState, TextPosition,
};
pub use text_input::{
    text_input, InputType, NumberConstraints, TextInput, TextInputBuilder, TextInputConfig,
    TextInputState,
};
pub use widget::{Widget, WidgetId};

/// Prelude for convenient imports
pub mod prelude {
    pub use crate::button::{button, Button, ButtonBuilder, ButtonConfig};
    pub use crate::checkbox::{checkbox, Checkbox, CheckboxBuilder, CheckboxConfig};
    pub use crate::context::{WidgetContext, WidgetContextExt};
    pub use crate::text_area::{text_area, TextArea, TextAreaBuilder, TextAreaConfig};
    pub use crate::text_input::{
        text_input, InputType, NumberConstraints, TextInput, TextInputBuilder, TextInputConfig,
    };
    pub use crate::widget::{Widget, WidgetId};
}
