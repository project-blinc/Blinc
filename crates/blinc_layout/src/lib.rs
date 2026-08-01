#![allow(unused, dead_code, deprecated, mismatched_lifetime_syntaxes)]
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
pub mod build_once;
pub mod canvas;
pub mod diff;
pub mod div;
pub mod element;
pub mod notch;

// Layout animation systems
pub mod binding;
pub mod element_style;
pub mod event_handler;
pub mod event_router;
pub mod image;
pub mod interactive;
pub mod layout_animation;
pub mod motion;
pub mod motion_texture_cache;
pub mod property;
pub mod render_state;
pub mod renderer;
pub mod rich_text;
pub mod scroll;
pub mod stack;
pub mod state_style_table;
pub mod stateful;
pub mod style;
pub mod styled_text;
pub mod svg;
pub mod syntax;
pub mod text;
pub mod text_measure;
pub mod text_selection;
pub mod tree;
pub mod typography;
pub mod units;
pub mod visual_animation;
pub mod widgets;

// Markdown rendering
pub mod markdown;

// Selector API for programmatic element access
pub mod selector;

// Global overlay state singleton
pub mod overlay_state;

// Continuous pointer query system
pub mod pointer_query;

// Click-outside detection for dropdown dismissal
pub mod click_outside;

// Recorder bridge for event capture (blinc_recorder integration)
#[cfg(feature = "recorder")]
pub mod recorder_bridge;

// CSS calc() expression engine
pub mod calc;

// CSS subset parser for ElementStyle
pub mod css_parser;

// Stable unique key generation for components
pub mod back_handler;
pub mod key;
pub mod window_actions;

// Re-export InstanceKey and reset function at crate root
pub use key::{InstanceKey, reset_call_counters};

// Property channel foundation (reactive architecture v2, Phase 1)
pub use property::{PropertyId, SideEffects, TransformEquivalent};

// Signal-bound property bindings (reactive architecture v2, Phase 2)
pub use binding::{
    BoundValue, IntoReactive, LayoutPendingBinding, PendingBinding, PropertyBindingRegistry,
    Reactive, TypedPendingBinding, register_typed, register_typed_layout,
    unregister_node as unregister_property_bindings, with_registry,
};

// Pre-resolved CSS state-style cascade table (reactive architecture v2, Phase 5)
pub use state_style_table::StateStyleTable;

// Motion-subtree texture bake registry (reactive architecture v2, Phase 4.2)
pub use motion_texture_cache::{
    MotionBakeState, MotionSubtreeBakeRecord, MotionSubtreeBakeRegistry,
};

// Core types
pub use element::{
    BorderBuilder, BorderSide, BorderSides, CursorStyle, DynRenderProps, ElementBounds, FlowRef,
    MotionAnimation, MotionKeyframe, RenderLayer, RenderProps, ResolvedRenderProps,
};

// Diff and reconciliation
pub use diff::{
    ChangeCategory, ChildDiff, DiffResult, DivHash, ReconcileActions, diff, diff_children,
    diff_elements, reconcile,
};
pub use event_handler::{EventCallback, EventContext, EventHandlers, HandlerRegistry};
pub use event_router::{EventRouter, HitTestResult, MouseButton};
pub use interactive::{DirtyTracker, InteractiveContext, NodeState};
pub use style::LayoutStyle;
pub use tree::{LayoutNodeId, LayoutTree, TextMeasureContext};

// Material system
pub use element::{
    GlassMaterial, Material, MaterialShadow, MetallicMaterial, SolidMaterial, WoodMaterial,
};

// Builder API
/// Short alias for [`ElementBuilder`].
pub use div::ElementBuilder as Element;
pub use div::{
    Div, ElementBuilder, ElementTypeId, FontFamily, FontWeight, GenericFont, ImageRenderInfo,
    StyledTextRenderInfo, StyledTextSpanInfo, TextAlign, TextVerticalAlign, div,
};
// Stack container (overlayed children)
pub use stack::{Stack, stack};
// Reference binding
pub use div::{DivRef, ElementRef};
pub use image::{
    Image, ImageFilter, LoadingStrategy, ObjectFit, ObjectPosition, Placeholder, emoji,
    emoji_sized, image, img,
};
pub use rich_text::{RichText, rich_text, rich_text_styled};
pub use svg::{Svg, svg};
pub use text::{Text, text};

// Renderer
pub use renderer::{
    GlassPanel, ImageData, LayoutRenderer, OnReadyCallback, OnReadyEntry, RenderTree,
    RenderTreeDebugStats, StyledTextData, StyledTextSpan, SvgData, TextData, UpdateResult,
};

// Canvas element
pub use canvas::{Canvas, CanvasBounds, CanvasData, CanvasRenderFn, canvas};

// Render state (dynamic properties separate from tree structure)
pub use render_state::{
    ActiveMotion, CssAnimationStore, MotionState, NodeRenderState, Overlay, RenderState,
    SharedMotionStates, create_shared_motion_states, get_global_scheduler, has_global_scheduler,
    queue_global_motion_exit_cancel, queue_global_motion_exit_start, queue_global_motion_start,
    set_global_scheduler,
};

// Stateful elements
pub use stateful::{
    PartialPropertyUpdate, PendingSubtreeRebuild, RenderPropsWrite, SharedState, StateTransitions,
    StatefulInner, TaffyStyleWrite, check_stateful_animations, check_stateful_deps,
    clear_stateful_animations, clear_stateful_base_updaters, clear_stateful_deps,
    has_animating_statefuls, has_pending_partial_prop_updates, has_pending_subtree_rebuilds,
    has_stateful_base_updater, has_visible_animating_statefuls, peek_needs_redraw,
    queue_layout_update_partial, queue_prop_update, queue_prop_update_partial,
    queue_subtree_rebuild, request_animation_tick, request_redraw, take_animation_tick_request,
    take_needs_redraw, take_pending_partial_prop_updates, take_pending_subtree_rebuilds,
    update_stateful_base_props, use_fsm, use_fsm_keyed, use_shared_state, use_shared_state_with,
    use_state_for, use_state_for_keyed,
};

// Animation integration
pub use animated::{AnimatedProperties, AnimationBuilder};

// Layout animation (FLIP-style bounds animation)
pub use layout_animation::{LayoutAnimation, LayoutAnimationConfig, LayoutAnimationState};

// CSS-like units
pub use units::{Length, Unit, pct, px, sp};

// Motion container for entry/exit animations
pub use motion::{
    ElementAnimation, ExitingChild, Motion, MotionBindings, MotionPresenceState,
    MotionPresenceStore, SharedAnimatedValue, SlideDirection, StaggerConfig, StaggerDirection,
    check_and_clear_exiting, check_ready_for_enter, current_motion_key, is_inside_animating_motion,
    is_inside_motion, motion, motion_derived, motion_events, motion_presence_store,
    query_presence_state, start_exit_for_key, update_presence_state,
};

// Text measurement
pub use text_measure::{
    TextLayoutOptions, TextMeasurer, TextMetrics, measure_text, measure_text_with_options,
    set_text_measurer,
};

// Text selection (clipboard support)
pub use text_selection::{
    SelectionSource, SharedTextSelection, TextSelection, clear_selection, get_selected_text,
    global_selection, set_selection,
};

/// Prelude module - import everything commonly needed
pub mod prelude {
    /// Short alias for [`ElementBuilder`].
    pub use crate::div::ElementBuilder as Element;
    pub use crate::div::{
        Div, ElementBuilder, ElementTypeId, FontFamily, FontWeight, GenericFont, ImageRenderInfo,
        TextAlign, TextVerticalAlign, div,
    };
    // Stack container (overlayed children)
    pub use crate::stack::{Stack, stack};
    // Reference binding for external element access
    pub use crate::div::{DivRef, ElementRef};
    pub use crate::element::{
        CursorStyle, DynRenderProps, ElementBounds, RenderLayer, RenderProps, ResolvedRenderProps,
    };
    // Event handlers
    pub use crate::event_handler::{EventCallback, EventContext, EventHandlers, HandlerRegistry};
    // Event routing
    pub use crate::event_router::{EventRouter, HitTestResult, MouseButton};
    // Image element
    pub use crate::image::{
        Image, ImageFilter, LoadingStrategy, ObjectFit, ObjectPosition, Placeholder, emoji,
        emoji_sized, image, img,
    };
    // Interactive state management
    pub use crate::interactive::{DirtyTracker, InteractiveContext, NodeState};
    // Unified element styling
    pub use crate::element_style::{
        ElementStyle, SpacingRect, StyleAlign, StyleDisplay, StyleFlexDirection, StyleJustify,
        StyleOverflow, StyleVisibility, style,
    };
    // Diff and reconciliation
    pub use crate::diff::{
        ChangeCategory, ChildDiff, DiffResult, DivHash, ReconcileActions, diff, diff_children,
        diff_elements, reconcile,
    };
    // Stateful elements with user-defined state types (core infrastructure)
    pub use crate::stateful::{
        // Core generic type
        BoundStateful,
        // Type aliases for Stateful<S> - low-level for custom styling
        Button as StatefulButton,
        // Built-in state types (Copy-based for Stateful<S>)
        ButtonState,
        Checkbox as StatefulCheckbox,
        CheckboxState as StatefulCheckboxState,
        ChildKeyCounter,
        KeyframeHandle,
        // No-op state for dependency-based refreshing
        NoState,
        ScrollContainer,
        ScrollState,
        SharedAnimatedTimeline,
        SharedAnimatedValue,
        SharedKeyframeTrack,
        SharedState,
        StateContext,
        StateTransitions,
        Stateful,
        StatefulBuilder,
        StatefulInner,
        TextField,
        TextFieldState,
        Toggle,
        ToggleState,
        // Internal scroll events for FSM transitions
        scroll_events,
        // New StateContext API (recommended)
        stateful,
        stateful_button,
        stateful_checkbox,
        // Low-level constructor functions for custom styling (legacy)
        stateful_from_handle,
        stateful_with_key,
        text_field,
        toggle,
        // Bare auto-keyed FSM handle (`SharedState<S>`) — uses
        // `#[track_caller]` to derive a unique key from the source
        // location. The common case for "one Stateful per call
        // site" — no manual key plumbing required.
        use_fsm,
        // Explicit-key variant for loops / reusable component
        // factories where one source line creates multiple
        // instances.
        use_fsm_keyed,
        // Utility functions for persistent shared state
        use_shared_state,
        use_shared_state_with,
        // Deprecated aliases — kept on the prelude during the
        // migration window so existing call sites keep compiling
        // (with a warning) without an extra import.
        use_state_for,
        use_state_for_keyed,
    };

    // Ready-to-use widgets (production-ready, work in fluent API without .build())
    pub use crate::widgets::{
        Button,
        ButtonConfig,
        ButtonVisualState,
        CURSOR_BLINK_INTERVAL_MS,
        Checkbox,
        CheckboxConfig,
        InputConstraints,
        InputType,
        RadioGroup,
        RadioGroupBuilder,
        RadioGroupConfig,
        RadioLayout,
        SharedTextAreaState,
        SharedTextInputState,
        TextArea,
        TextAreaConfig,
        TextAreaState,
        TextInput,
        TextInputConfig,
        TextInputState,
        TextPosition,
        // Button widget - ready-to-use
        button,
        // Checkbox widget - ready-to-use
        checkbox,
        checkbox_labeled,
        // Cursor blink timing (for use by app layer)
        elapsed_ms,
        has_focused_text_input,
        // Radio group widget - ready-to-use
        radio_group,
        // Text area widget - ready-to-use
        text_area,
        text_area_state,
        text_area_state_with_placeholder,
        // Text input widget - ready-to-use
        text_input,
        text_input_state,
        text_input_state_with_placeholder,
    };
    // Material system
    pub use crate::element::{
        GlassMaterial, Material, MaterialShadow, MetallicMaterial, SolidMaterial, WoodMaterial,
    };
    #[allow(deprecated)]
    pub use crate::renderer::{
        GlassPanel, ImageData, LayoutRenderer, OnReadyCallback, RenderTree, SvgData, TextData,
        UpdateResult,
    };
    // Scroll container (ready-to-use widget with Div extension)
    pub use crate::rich_text::{RichText, rich_text, rich_text_styled};
    pub use crate::svg::{Svg, svg};
    pub use crate::text::{Text, text};
    pub use crate::tree::{LayoutNodeId, LayoutTree, StableNodeId};
    pub use crate::widgets::{
        Scroll, ScrollConfig, ScrollDirection, ScrollPhysics, ScrollRenderInfo,
        SharedScrollPhysics, scroll, scroll_bouncy, scroll_no_bounce,
    };

    // Code block widget with syntax highlighting
    pub use crate::widgets::{
        Code, CodeConfig, CodeEditor, CodeEditorData, SharedCodeEditorState, code, code_editor,
        code_editor_state, code_minimap, pre,
    };

    // CSS-like units for layout dimensions
    pub use crate::units::{Length, Unit, pct, px, sp};

    // Syntax highlighting
    pub use crate::syntax::{
        JsonHighlighter, PlainHighlighter, RustHighlighter, SyntaxConfig, SyntaxHighlighter,
        TokenHit, TokenRule, TokenType,
    };

    // Canvas element
    pub use crate::canvas::{Canvas, CanvasBounds, canvas};

    // Notch element (shapes with concave curves or sharp steps)
    pub use crate::notch::{CornerConfig, CornerStyle, CornersConfig, Notch, notch};

    // Re-export Shadow, Transform, and layer effect types from blinc_core for convenience
    pub use blinc_core::{BlurQuality, BlurStyle, LayerEffect, Shadow, Transform};

    // Animation integration
    pub use crate::animated::{AnimatedProperties, AnimationBuilder};

    // Layout animation (FLIP-style bounds animation)
    pub use crate::layout_animation::{LayoutAnimation, LayoutAnimationConfig};

    // Re-export animation types from blinc_animation for convenience
    pub use blinc_animation::{
        AnimatedKeyframe, AnimatedTimeline, AnimatedValue, AnimationPreset, Easing,
        KeyframeProperties, MultiKeyframeAnimation, SchedulerHandle, SpringConfig,
    };

    // Motion container for entry/exit animations
    pub use crate::motion::{
        ElementAnimation, Motion, MotionBindings, SlideDirection, StaggerConfig, StaggerDirection,
        current_motion_key, is_inside_animating_motion, is_inside_motion, motion, motion_derived,
    };

    // Text selection for clipboard support
    pub use crate::text_selection::{
        SelectionSource, SharedTextSelection, TextSelection, clear_selection, get_selected_text,
        global_selection, set_selection,
    };

    // Render state (dynamic properties separate from tree structure)
    pub use crate::render_state::{
        ActiveMotion, MotionState, NodeRenderState, Overlay, RenderState,
    };

    // Dynamic value system for render-time resolution
    pub use blinc_core::{AnimationAccess, DynFloat, DynValue, ReactiveAccess, ValueContext};

    // Typography helpers (h1-h6, b, span, etc.)
    pub use crate::typography::{
        b, caption, chained_text, h1, h2, h3, h4, h5, h6, heading, inline_code, label, muted, p,
        small, span, strong,
    };

    // Table elements
    pub use crate::widgets::{
        TableBuilder, TableCell, cell, striped_tr, table, tbody, td, td_text, tfoot, th, th_text,
        thead, tr,
    };

    // Overlay system (modals, dialogs, context menus, toasts)
    pub use crate::widgets::{
        BackdropConfig, ContextMenuBuilder, Corner, DialogBuilder, DropdownBuilder, ModalBuilder,
        OverlayAnimation, OverlayConfig, OverlayHandle, OverlayKind, OverlayManager,
        OverlayManagerExt, OverlayPosition, OverlayState, ToastBuilder, overlay_events,
        overlay_manager,
    };

    // Markdown rendering
    pub use crate::markdown::{
        MarkdownConfig, MarkdownRenderer, markdown, markdown_light, markdown_with_config,
    };

    // Additional markdown widgets
    pub use crate::widgets::{
        Blockquote, BlockquoteConfig, HrConfig, Link, LinkConfig, ListConfig, ListItem, ListMarker,
        OrderedList, TaskListItem, UnorderedList, blockquote, blockquote_with_config, hr, hr_color,
        hr_thick, hr_with_bg, hr_with_config, li, link, ol, ol_start, task_item, ul,
    };

    // Selector API for programmatic element access
    pub use crate::selector::{
        ElementEvent, ElementHandle, ElementRegistry, MotionHandle, ScrollBehavior, ScrollBlock,
        ScrollInline, ScrollOptions, ScrollRef, SharedElementRegistry, query, query_motion,
    };

    // Overlay context singleton
    pub use crate::overlay_state::{OverlayContext, get_overlay_manager};

    // CSS parser for loading stylesheets
    pub use crate::css_parser::{
        AnimationDirection, AnimationFillMode, AnimationTiming, Combinator, ComplexSelector,
        CompoundSelector, CssAnimation, CssKeyframe, CssKeyframes, CssParseResult, CssSelector,
        ElementState as CssElementState, ParseError as CssParseError, SelectorPart,
        Severity as CssSeverity, StructuralPseudo, Stylesheet,
    };

    // Flow DAG types (re-exported from blinc_core)
    pub use blinc_core::{
        FlowError, FlowExpr, FlowFunc, FlowGraph, FlowInput, FlowInputSource, FlowNode, FlowOutput,
        FlowOutputTarget, FlowTarget, FlowType,
    };

    // Stable unique key generation for components
    pub use crate::key::{InstanceKey, reset_call_counters};
}
