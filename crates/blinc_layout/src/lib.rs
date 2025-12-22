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
pub mod renderer;
pub mod style;
pub mod svg;
pub mod text;
pub mod tree;

// Core types
pub use element::{ElementBounds, RenderLayer, RenderProps};
pub use style::LayoutStyle;
pub use tree::{LayoutNodeId, LayoutTree};

// Material system
pub use element::{
    GlassMaterial, Material, MaterialShadow, MetallicMaterial, SolidMaterial, WoodMaterial,
};

// Builder API
pub use div::{div, Div, ElementBuilder, ElementTypeId};
pub use svg::{svg, Svg};
pub use text::{text, Text};

// Renderer
pub use renderer::{GlassPanel, LayoutRenderer, RenderTree, SvgData, TextData};

/// Prelude module - import everything commonly needed
pub mod prelude {
    pub use crate::div::{div, Div, ElementBuilder, ElementTypeId};
    pub use crate::element::{ElementBounds, RenderLayer, RenderProps};
    // Material system
    pub use crate::element::{
        GlassMaterial, Material, MaterialShadow, MetallicMaterial, SolidMaterial, WoodMaterial,
    };
    pub use crate::renderer::{GlassPanel, LayoutRenderer, RenderTree, SvgData, TextData};
    pub use crate::svg::{svg, Svg};
    pub use crate::text::{text, Text};
    pub use crate::tree::{LayoutNodeId, LayoutTree};
}
