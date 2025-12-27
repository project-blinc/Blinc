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

pub mod animated;
pub mod canvas;
pub mod div;
pub mod element;
pub mod element_style;
pub mod event_handler;
pub mod event_router;
pub mod image;
pub mod interactive;
pub mod motion;
pub mod render_state;
pub mod renderer;
pub mod scroll;
pub mod stateful;
pub mod style;
pub mod styled_text;
pub mod svg;
pub mod syntax;
pub mod table;
pub mod text;
pub mod text_measure;
pub mod text_selection;
pub mod tree;
pub mod typography;
pub mod widgets;

// Core types
pub use element::{
    DynRenderProps, ElementBounds, MotionAnimation, MotionKeyframe, RenderLayer, RenderProps,
    ResolvedRenderProps,
};
pub use event_handler::{EventCallback, EventContext, EventHandlers, HandlerRegistry};
pub use event_router::{EventRouter, HitTestResult, MouseButton};
pub use interactive::{DirtyTracker, InteractiveContext, NodeState};
pub use style::LayoutStyle;
pub use tree::{LayoutNodeId, LayoutTree};

// Material system
pub use element::{
    GlassMaterial, Material, MaterialShadow, MetallicMaterial, SolidMaterial, WoodMaterial,
};

// Builder API
pub use div::{
    div, Div, ElementBuilder, ElementTypeId, FontFamily, FontWeight, GenericFont, ImageRenderInfo,
    TextAlign, TextVerticalAlign,
};
// Reference binding
pub use div::{DivRef, ElementRef};
pub use image::{image, img, Image, ImageFilter, ObjectFit, ObjectPosition};
pub use svg::{svg, Svg};
pub use text::{text, Text};

// Renderer
pub use renderer::{
    GlassPanel, ImageData, LayoutRenderer, RenderTree, StyledTextData, StyledTextSpan, SvgData,
    TextData,
};

// Canvas element
pub use canvas::{canvas, Canvas, CanvasBounds, CanvasData, CanvasRenderFn};

// Render state (dynamic properties separate from tree structure)
pub use render_state::{ActiveMotion, MotionState, NodeRenderState, Overlay, RenderState};

// Stateful elements
pub use stateful::{check_stateful_deps, request_redraw, take_needs_redraw, take_pending_prop_updates, take_pending_subtree_rebuilds, PendingSubtreeRebuild, SharedState, StateTransitions, StatefulInner};

// Animation integration
pub use animated::{AnimatedProperties, AnimationBuilder};

// Motion container for entry/exit animations
pub use motion::{
    motion, ElementAnimation, Motion, SlideDirection, StaggerConfig, StaggerDirection,
};

// Text measurement
pub use text_measure::{
    measure_text, measure_text_with_options, set_text_measurer, TextLayoutOptions, TextMeasurer,
    TextMetrics,
};

// Text selection (clipboard support)
pub use text_selection::{
    clear_selection, get_selected_text, global_selection, set_selection, SelectionSource,
    SharedTextSelection, TextSelection,
};

/// Prelude module - import everything commonly needed
pub mod prelude {
    pub use crate::div::{
        div, Div, ElementBuilder, ElementTypeId, FontFamily, FontWeight, GenericFont,
        ImageRenderInfo, TextAlign, TextVerticalAlign,
    };
    // Reference binding for external element access
    pub use crate::div::{DivRef, ElementRef};
    pub use crate::element::{
        DynRenderProps, ElementBounds, RenderLayer, RenderProps, ResolvedRenderProps,
    };
    // Event handlers
    pub use crate::event_handler::{EventCallback, EventContext, EventHandlers, HandlerRegistry};
    // Event routing
    pub use crate::event_router::{EventRouter, HitTestResult, MouseButton};
    // Image element
    pub use crate::image::{image, img, Image, ImageFilter, ObjectFit, ObjectPosition};
    // Interactive state management
    pub use crate::interactive::{DirtyTracker, InteractiveContext, NodeState};
    // Unified element styling
    pub use crate::element_style::{style, ElementStyle};
    // Stateful elements with user-defined state types (core infrastructure)
    pub use crate::stateful::{
        // Internal scroll events for FSM transitions
        scroll_events,
        // Low-level constructor functions for custom styling
        stateful,
        stateful_button,
        stateful_checkbox,
        text_field,
        toggle,
        // Core generic type
        BoundStateful,
        // Type aliases for Stateful<S> - low-level for custom styling
        Button as StatefulButton,
        // Built-in state types (Copy-based for Stateful<S>)
        ButtonState,
        Checkbox as StatefulCheckbox,
        CheckboxState as StatefulCheckboxState,
        ScrollContainer,
        ScrollState,
        SharedState,
        StateTransitions,
        Stateful,
        StatefulInner,
        TextField,
        TextFieldState,
        Toggle,
        ToggleState,
    };

    // Ready-to-use widgets (production-ready, work in fluent API without .build())
    pub use crate::widgets::{
        // Button widget - ready-to-use
        button,
        // Checkbox widget - ready-to-use
        checkbox,
        checkbox_labeled,
        checkbox_state,
        // Cursor blink timing (for use by app layer)
        elapsed_ms,
        has_focused_text_input,
        // Text area widget - ready-to-use
        text_area,
        text_area_state,
        text_area_state_with_placeholder,
        // Text input widget - ready-to-use
        text_input,
        text_input_state,
        text_input_state_with_placeholder,
        Button,
        ButtonConfig,
        ButtonVisualState,
        Checkbox,
        CheckboxConfig,
        CheckboxState,
        InputType,
        NumberConstraints,
        SharedCheckboxState,
        SharedTextAreaState,
        SharedTextInputState,
        TextArea,
        TextAreaConfig,
        TextAreaState,
        TextInput,
        TextInputConfig,
        TextInputState,
        TextPosition,
        CURSOR_BLINK_INTERVAL_MS,
    };
    // Material system
    pub use crate::element::{
        GlassMaterial, Material, MaterialShadow, MetallicMaterial, SolidMaterial, WoodMaterial,
    };
    #[allow(deprecated)]
    pub use crate::renderer::{
        GlassPanel, ImageData, LayoutRenderer, RenderTree, SvgData, TextData,
    };
    // Scroll container (ready-to-use widget with Div extension)
    pub use crate::svg::{svg, Svg};
    pub use crate::text::{text, Text};
    pub use crate::tree::{LayoutNodeId, LayoutTree};
    pub use crate::widgets::{
        scroll, scroll_no_bounce, Scroll, ScrollConfig, ScrollDirection, ScrollPhysics,
        ScrollRenderInfo, SharedScrollPhysics,
    };

    // Code block widget with syntax highlighting
    pub use crate::widgets::{code, pre, Code, CodeConfig};

    // Syntax highlighting
    pub use crate::syntax::{
        JsonHighlighter, PlainHighlighter, RustHighlighter, SyntaxConfig, SyntaxHighlighter,
        TokenHit, TokenRule, TokenType,
    };

    // Canvas element
    pub use crate::canvas::{canvas, Canvas, CanvasBounds};

    // Re-export Shadow and Transform from blinc_core for convenience
    pub use blinc_core::{Shadow, Transform};

    // Animation integration
    pub use crate::animated::{AnimatedProperties, AnimationBuilder};

    // Re-export animation types from blinc_animation for convenience
    pub use blinc_animation::{
        AnimatedKeyframe, AnimatedTimeline, AnimatedValue, AnimationPreset, Easing,
        KeyframeProperties, MultiKeyframeAnimation, SchedulerHandle, SpringConfig,
    };

    // Motion container for entry/exit animations
    pub use crate::motion::{
        motion, ElementAnimation, Motion, SlideDirection, StaggerConfig, StaggerDirection,
    };

    // Text selection for clipboard support
    pub use crate::text_selection::{
        clear_selection, get_selected_text, global_selection, set_selection, SelectionSource,
        SharedTextSelection, TextSelection,
    };

    // Render state (dynamic properties separate from tree structure)
    pub use crate::render_state::{
        ActiveMotion, MotionState, NodeRenderState, Overlay, RenderState,
    };

    // Dynamic value system for render-time resolution
    pub use blinc_core::{AnimationAccess, DynFloat, DynValue, ReactiveAccess, ValueContext};

    // Typography helpers (h1-h6, b, span, etc.)
    pub use crate::typography::{
        b, caption, h1, h2, h3, h4, h5, h6, heading, inline_code, label, muted, p, small, span,
        strong,
    };

    // Table elements
    pub use crate::table::{
        cell, striped_tr, table, tbody, td, td_text, tfoot, th, th_text, thead, tr, TableBuilder,
        TableCell,
    };
}
