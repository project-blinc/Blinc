//! Blinc Layout Engine
//!
//! Flexbox layout powered by Taffy with GPUI-style builder API.
//!
//! # Example
//!
//! ```rust
//! use blinc_layout::prelude::*;
//!
//! let ui = div()
//!     .flex_col()
//!     .w(400.0)
//!     .h(300.0)
//!     .gap(4.0)
//!     .p(4.0)
//!     .child(
//!         div()
//!             .flex_row()
//!             .justify_between()
//!             .child(text("Title").size(24.0))
//!             .child(div().square(32.0).rounded(8.0))
//!     )
//!     .child(
//!         div().flex_grow()
//!     );
//!
//! let mut tree = RenderTree::from_element(&ui);
//! tree.compute_layout(800.0, 600.0);
//! ```

pub mod div;
pub mod element;
pub mod element_style;
pub mod event_router;
pub mod image;
pub mod interactive;
pub mod renderer;
pub mod stateful;
pub mod style;
pub mod svg;
pub mod text;
pub mod tree;

// Core types
pub use element::{ElementBounds, RenderLayer, RenderProps};
pub use event_router::{EventRouter, HitTestResult, MouseButton};
pub use interactive::{DirtyTracker, InteractiveContext, NodeState};
pub use style::LayoutStyle;
pub use tree::{LayoutNodeId, LayoutTree};

// Material system
pub use element::{
    GlassMaterial, Material, MaterialShadow, MetallicMaterial, SolidMaterial, WoodMaterial,
};

// Builder API
pub use div::{div, Div, ElementBuilder, ElementTypeId, FontWeight, ImageRenderInfo, TextAlign};
// Reference binding
pub use div::{DivRef, ElementRef};
pub use image::{image, img, Image, ImageFilter, ObjectFit, ObjectPosition};
pub use svg::{svg, Svg};
pub use text::{text, Text};

// Renderer
pub use renderer::{GlassPanel, ImageData, LayoutRenderer, RenderTree, SvgData, TextData};

/// Prelude module - import everything commonly needed
pub mod prelude {
    pub use crate::div::{
        div, Div, ElementBuilder, ElementTypeId, FontWeight, ImageRenderInfo, TextAlign,
    };
    // Reference binding for external element access
    pub use crate::div::{DivRef, ElementRef};
    pub use crate::element::{ElementBounds, RenderLayer, RenderProps};
    // Event routing
    pub use crate::event_router::{EventRouter, HitTestResult, MouseButton};
    // Image element
    pub use crate::image::{image, img, Image, ImageFilter, ObjectFit, ObjectPosition};
    // Interactive state management
    pub use crate::interactive::{DirtyTracker, InteractiveContext, NodeState};
    // Unified element styling
    pub use crate::element_style::{style, ElementStyle};
    // Stateful elements with user-defined state types
    pub use crate::stateful::{
        // Core generic type
        BoundStateful, Stateful, StateTransitions,
        // Built-in state types
        ButtonState, CheckboxState, TextFieldState, ToggleState,
        // Type aliases
        StatefulButton, StatefulCheckbox, StatefulTextField, StatefulToggle,
        // Constructor functions
        stateful, stateful_button, stateful_checkbox, stateful_text_field, stateful_toggle,
    };
    // Material system
    pub use crate::element::{
        GlassMaterial, Material, MaterialShadow, MetallicMaterial, SolidMaterial, WoodMaterial,
    };
    #[allow(deprecated)]
    pub use crate::renderer::{
        GlassPanel, ImageData, LayoutRenderer, RenderTree, SvgData, TextData,
    };
    pub use crate::svg::{svg, Svg};
    pub use crate::text::{text, Text};
    pub use crate::tree::{LayoutNodeId, LayoutTree};

    // Re-export Shadow and Transform from blinc_core for convenience
    pub use blinc_core::{Shadow, Transform};
}
