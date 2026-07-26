//! GPUI-style div builder with tailwind-style methods
//!
//! Provides a fluent builder API for creating layout elements:
//! ```rust
//! use blinc_layout::prelude::*;
//! use blinc_core::Color;
//!
//! let ui = div()
//!     .flex_row()
//!     .gap(4.0)
//!     .p(2.0)
//!     .bg(Color::RED)
//!     .child(text("Hello"));
//! ```

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

use blinc_core::{
    BlurQuality, BlurStyle, Brush, ClipPath, Color, CornerRadius, CornerShape, LayerEffect,
    OverflowFade, Shadow, Transform,
};
use blinc_theme::{ShadowTokens, ThemeState};

/// Look up a theme shadow stack and convert it to `Vec<blinc_core::Shadow>`.
fn theme_shadow_stack<F>(pick: F) -> Vec<Shadow>
where
    F: Fn(&ShadowTokens) -> &[blinc_theme::Shadow],
{
    let shadows = ThemeState::get().shadows();
    pick(&shadows).iter().map(Shadow::from).collect()
}
use taffy::Overflow;
use taffy::prelude::*;

use crate::element::{
    ElementBounds, GlassMaterial, Material, MetallicMaterial, RenderLayer, RenderProps,
    WoodMaterial,
};
use crate::element_style::ElementStyle;
use crate::tree::{LayoutNodeId, LayoutTree};

// ============================================================================
// ElementRef - Generic reference binding for external access
// ============================================================================

/// Shared storage for element references
type RefStorage<T> = Arc<Mutex<Option<T>>>;

/// Shared dirty flag for automatic rebuild triggering
type DirtyFlag = Arc<AtomicBool>;

/// A generic reference binding to an element that can be accessed externally
///
/// Similar to React's `useRef`, this allows capturing a reference to an element
/// for external manipulation while maintaining the fluent API flow.
///
/// # Example
///
/// ```ignore
/// use blinc_layout::prelude::*;
///
/// // Create a reference
/// let button_ref = ElementRef::<StatefulButton>::new();
///
/// // Build UI - .bind() works seamlessly in the fluent chain
/// let ui = div()
///     .flex_col()
///     .child(
///         stateful_button()
///             .bind(&button_ref)  // Binds AND continues the chain
///             .on_state(|state, div| { ... })
///     );
///
/// // Later, access the bound element's full API
/// button_ref.with_mut(|btn| {
///     btn.dispatch_state(ButtonState::Pressed);
/// });
/// ```
/// Storage for computed layout bounds
type LayoutBoundsStorage = Arc<Mutex<Option<ElementBounds>>>;

pub struct ElementRef<T> {
    inner: RefStorage<T>,
    /// Shared dirty flag - when set, signals that the UI needs to be rebuilt
    dirty_flag: DirtyFlag,
    /// Computed layout bounds (set after layout is computed)
    layout_bounds: LayoutBoundsStorage,
}

impl<T> Clone for ElementRef<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            dirty_flag: Arc::clone(&self.dirty_flag),
            layout_bounds: Arc::clone(&self.layout_bounds),
        }
    }
}

impl<T> Default for ElementRef<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> ElementRef<T> {
    /// Create a new empty ElementRef
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(None)),
            dirty_flag: Arc::new(AtomicBool::new(false)),
            layout_bounds: Arc::new(Mutex::new(None)),
        }
    }

    /// Create an ElementRef with a shared dirty flag
    ///
    /// This is used internally to share the same dirty flag across
    /// multiple refs, allowing the windowed app to check for changes.
    pub fn with_dirty_flag(dirty_flag: DirtyFlag) -> Self {
        Self {
            inner: Arc::new(Mutex::new(None)),
            dirty_flag,
            layout_bounds: Arc::new(Mutex::new(None)),
        }
    }

    /// Get the dirty flag handle (for sharing with other refs)
    pub fn dirty_flag(&self) -> DirtyFlag {
        Arc::clone(&self.dirty_flag)
    }

    /// Check if the element was modified and clear the flag
    ///
    /// Returns `true` if the element was modified since the last check.
    pub fn take_dirty(&self) -> bool {
        self.dirty_flag.swap(false, Ordering::SeqCst)
    }

    /// Check if the element was modified (without clearing)
    pub fn is_dirty(&self) -> bool {
        self.dirty_flag.load(Ordering::SeqCst)
    }

    /// Mark the element as dirty (needs rebuild)
    pub fn mark_dirty(&self) {
        self.dirty_flag.store(true, Ordering::SeqCst);
    }

    /// Check if an element is bound to this reference
    pub fn is_bound(&self) -> bool {
        self.inner.lock().unwrap().is_some()
    }

    /// Get the internal storage handle for shared access
    ///
    /// This is used by `.bind()` implementations to share storage
    /// between the bound element wrapper and this ref.
    pub fn storage(&self) -> RefStorage<T> {
        Arc::clone(&self.inner)
    }

    /// Set the element in storage (used by bind implementations)
    pub fn set(&self, elem: T) {
        *self.inner.lock().unwrap() = Some(elem);
    }

    /// Access the bound element immutably with a callback
    ///
    /// Returns `Some(result)` if an element is bound, `None` otherwise.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let state = button_ref.with(|btn| *btn.state());
    /// ```
    pub fn with<F, R>(&self, f: F) -> Option<R>
    where
        F: FnOnce(&T) -> R,
    {
        self.inner.lock().unwrap().as_ref().map(f)
    }

    /// Access the bound element mutably with a callback
    ///
    /// Returns `Some(result)` if an element is bound, `None` otherwise.
    /// This is the primary way to call methods on the bound element.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Dispatch state changes
    /// button_ref.with_mut(|btn| {
    ///     btn.dispatch_state(ButtonState::Pressed);
    /// });
    ///
    /// // Modify element styling
    /// div_ref.with_mut(|div| {
    ///     *div = div.swap().bg(Color::RED).rounded(8.0);
    /// });
    /// ```
    ///
    /// **Note:** This automatically marks the element as dirty after the callback,
    /// triggering a UI rebuild.
    pub fn with_mut<F, R>(&self, f: F) -> Option<R>
    where
        F: FnOnce(&mut T) -> R,
    {
        let result = self.inner.lock().unwrap().as_mut().map(f);
        if result.is_some() {
            // Mark dirty after successful mutation
            self.dirty_flag.store(true, Ordering::SeqCst);
        }
        result
    }

    /// Get a clone of the bound element, if any
    pub fn get(&self) -> Option<T>
    where
        T: Clone,
    {
        self.inner.lock().unwrap().clone()
    }

    /// Replace the bound element with a new one, returning the old value
    ///
    /// **Note:** This automatically marks the element as dirty, triggering a UI rebuild.
    pub fn replace(&self, new_elem: T) -> Option<T> {
        let old = self.inner.lock().unwrap().replace(new_elem);
        self.dirty_flag.store(true, Ordering::SeqCst);
        old
    }

    /// Take the bound element out of the reference, leaving None
    pub fn take(&self) -> Option<T> {
        self.inner.lock().unwrap().take()
    }

    /// Borrow the bound element immutably
    ///
    /// Returns a guard that dereferences to &T. Panics if not bound.
    /// For fallible access, use `with()` instead.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let state = button_ref.borrow().state();
    /// ```
    ///
    /// # Panics
    ///
    /// Panics if no element is bound to this reference.
    pub fn borrow(&self) -> ElementRefGuard<'_, T> {
        ElementRefGuard {
            guard: self.inner.lock().unwrap(),
        }
    }

    /// Borrow the bound element mutably
    ///
    /// Returns a guard that dereferences to &mut T. Panics if not bound.
    /// **When the guard is dropped, the element is automatically marked dirty**,
    /// triggering a UI rebuild.
    ///
    /// For fallible access, use `with_mut()` instead.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // This automatically triggers a rebuild when the guard is dropped
    /// button_ref.borrow_mut().dispatch_state(ButtonState::Hovered);
    /// ```
    ///
    /// # Panics
    ///
    /// Panics if no element is bound to this reference.
    pub fn borrow_mut(&self) -> ElementRefGuardMut<'_, T> {
        ElementRefGuardMut {
            guard: self.inner.lock().unwrap(),
            dirty_flag: Arc::clone(&self.dirty_flag),
        }
    }

    // =========================================================================
    // Layout Bounds
    // =========================================================================

    /// Get the computed layout bounds for this element
    ///
    /// Returns `None` if layout hasn't been computed yet or if the element
    /// is not part of the layout tree.
    ///
    /// # Example
    ///
    /// ```ignore
    /// if let Some(bounds) = input_ref.get_layout_bounds() {
    ///     println!("Width: {}, Height: {}", bounds.width, bounds.height);
    /// }
    /// ```
    pub fn get_layout_bounds(&self) -> Option<ElementBounds> {
        *self.layout_bounds.lock().unwrap()
    }

    /// Set the computed layout bounds for this element
    ///
    /// This is called internally after layout is computed to store
    /// the element's position and dimensions.
    pub fn set_layout_bounds(&self, bounds: ElementBounds) {
        *self.layout_bounds.lock().unwrap() = Some(bounds);
    }

    /// Clear the stored layout bounds
    ///
    /// Called when the element is removed from the layout tree or
    /// when layout needs to be recomputed.
    pub fn clear_layout_bounds(&self) {
        *self.layout_bounds.lock().unwrap() = None;
    }

    /// Get the layout bounds storage handle for sharing
    ///
    /// This allows other parts of the system to update the layout bounds
    /// when layout is computed.
    pub fn layout_bounds_storage(&self) -> LayoutBoundsStorage {
        Arc::clone(&self.layout_bounds)
    }
}

/// Guard for immutable access to a bound element
pub struct ElementRefGuard<'a, T> {
    guard: std::sync::MutexGuard<'a, Option<T>>,
}

impl<T> std::ops::Deref for ElementRefGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.guard.as_ref().expect("ElementRef not bound")
    }
}

/// Guard for mutable access to a bound element
///
/// When this guard is dropped, the dirty flag is automatically set,
/// signaling that the UI needs to be rebuilt.
pub struct ElementRefGuardMut<'a, T> {
    guard: std::sync::MutexGuard<'a, Option<T>>,
    dirty_flag: DirtyFlag,
}

impl<T> std::ops::Deref for ElementRefGuardMut<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.guard.as_ref().expect("ElementRef not bound")
    }
}

impl<T> std::ops::DerefMut for ElementRefGuardMut<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.guard.as_mut().expect("ElementRef not bound")
    }
}

impl<T> Drop for ElementRefGuardMut<'_, T> {
    fn drop(&mut self) {
        // Mark dirty when the mutable borrow ends - user modified the element
        self.dirty_flag.store(true, Ordering::SeqCst);
    }
}

/// Type alias for Div references
pub type DivRef = ElementRef<Div>;

/// A div element builder with GPUI/Tailwind-style methods
pub struct Div {
    pub(crate) style: Style,
    pub(crate) children: Vec<Box<dyn ElementBuilder>>,
    pub(crate) background: Option<Brush>,
    pub(crate) border_radius: CornerRadius,
    /// Tracks whether border_radius was explicitly set (distinguishes "set to 0" from "not set" in merges)
    pub(crate) border_radius_explicit: bool,
    pub(crate) corner_shape: CornerShape,
    /// Set by [`Self::lock_corner_shape`]; instructs the paint walker
    /// not to substitute the theme's squircle exponent. Floating
    /// overlay widgets (popovers, dropdowns, select / combobox
    /// menus) use this to keep their chrome circular regardless of
    /// the active theme.
    pub(crate) corner_shape_locked: bool,
    pub(crate) border_color: Option<Color>,
    pub(crate) border_width: f32,
    pub(crate) border_sides: crate::element::BorderSides,
    pub(crate) render_layer: RenderLayer,
    pub(crate) material: Option<Material>,
    pub(crate) shadow: Vec<Shadow>,
    pub(crate) transform: Option<Transform>,
    /// Per-element transform origin in percentages `[x, y]` (0 = top-left,
    /// 100 = bottom-right, 50 = centre on each axis). Default `None`
    /// keeps the renderer's centre-pivot semantics. Set via
    /// [`Self::transform_origin`] or implicitly by helpers like
    /// [`Self::transform_width`] that need a left-edge pivot.
    pub(crate) transform_origin: Option<[f32; 2]>,
    pub(crate) opacity: f32,
    pub(crate) cursor: Option<crate::element::CursorStyle>,
    pub(crate) pointer_events_none: bool,
    /// Layer effects (blur, drop shadow, glow, color matrix) applied to this element
    pub(crate) layer_effects: Vec<LayerEffect>,
    /// Marks this as a stack layer for z-ordering (increments z_layer for interleaved rendering)
    pub(crate) is_stack_layer: bool,
    /// Whether this node is the root of an overlay panel that should route
    /// its SDF + text + SVG dispatches into the dynamic batch. Set by
    /// OverlayStack / ToastTray; the generic `Stack` widget does NOT set
    /// this (it only uses `is_stack_layer` for z-counter bumping).
    pub(crate) is_overlay_root: bool,
    pub(crate) event_handlers: crate::event_handler::EventHandlers,
    /// Element ID for selector API queries
    pub(crate) element_id: Option<String>,
    /// CSS class names for selector matching. Interned `Arc<str>` so
    /// repeated class names share one allocation (see
    /// [`blinc_core::intern`]).
    pub(crate) classes: Vec<std::sync::Arc<str>>,
    // 3D transform properties (stored separately to flow through to render_props)
    pub(crate) rotate_x: Option<f32>,
    pub(crate) rotate_y: Option<f32>,
    pub(crate) perspective_3d: Option<f32>,
    pub(crate) shape_3d: Option<f32>,
    pub(crate) depth: Option<f32>,
    pub(crate) light_direction: Option<[f32; 3]>,
    pub(crate) light_intensity: Option<f32>,
    pub(crate) ambient: Option<f32>,
    pub(crate) specular: Option<f32>,
    pub(crate) translate_z: Option<f32>,
    pub(crate) op_3d: Option<f32>,
    pub(crate) blend_3d: Option<f32>,
    /// Overflow fade distances (smooth alpha fade at clip edges)
    pub(crate) overflow_fade: OverflowFade,
    /// CSS clip-path shape function
    pub(crate) clip_path: Option<ClipPath>,
    /// @flow shader name (references a FlowGraph in the stylesheet)
    pub(crate) flow_name: Option<String>,
    /// Direct @flow graph (from `flow!` macro), bypasses stylesheet lookup
    pub(crate) flow_graph: Option<std::sync::Arc<blinc_core::FlowGraph>>,
    /// Fixed positioning (stays in place when ancestors scroll)
    pub(crate) is_fixed: bool,
    /// Sticky positioning (sticks when scrolled past threshold)
    pub(crate) is_sticky: bool,
    /// Sticky top threshold
    pub(crate) sticky_top: Option<f32>,
    /// Outline width in pixels
    pub(crate) outline_width: f32,
    /// Outline color
    pub(crate) outline_color: Option<Color>,
    /// Outline offset in pixels (gap between border and outline)
    pub(crate) outline_offset: f32,
    /// CSS z-index for stacking order
    pub(crate) z_index: i32,
    /// Scroll physics for overflow:scroll containers
    pub(crate) scroll_physics: Option<crate::scroll::SharedScrollPhysics>,
    /// Layout animation configuration for FLIP-style bounds animation
    pub(crate) layout_animation: Option<crate::layout_animation::LayoutAnimationConfig>,
    /// Visual animation configuration (new FLIP-style system, read-only layout)
    pub(crate) visual_animation: Option<crate::visual_animation::VisualAnimationConfig>,
    /// Ancestor stateful context key for automatic key derivation
    ///
    /// When set, motion containers and layout animations will use this key
    /// as a prefix for auto-generated stable keys.
    pub(crate) stateful_context_key: Option<String>,
    /// Pending signal-bound property bindings. Accumulated as `.bg(&signal)`,
    /// `.opacity(&signal)`, etc. are called; drained in `build()` once
    /// the `LayoutNodeId` is minted. Each entry registers itself against
    /// the new node id in the global property-binding registry.
    /// ([[project-reactive-architecture-v2]] Phase 2.)
    pub(crate) pending_bindings: Vec<Box<dyn crate::binding::PendingBinding>>,
}

impl Default for Div {
    fn default() -> Self {
        Self::new()
    }
}

impl Div {
    /// Create a new div element
    pub fn new() -> Self {
        Self {
            style: Style::default(),
            children: Vec::new(),
            background: None,
            border_radius: CornerRadius::default(),
            border_radius_explicit: false,
            corner_shape: CornerShape::default(),
            corner_shape_locked: false,
            border_color: None,
            border_width: 0.0,
            border_sides: crate::element::BorderSides::default(),
            render_layer: RenderLayer::default(),
            material: None,
            shadow: Vec::new(),
            transform: None,
            transform_origin: None,
            opacity: 1.0,
            cursor: None,
            pointer_events_none: false,
            layer_effects: Vec::new(),
            is_stack_layer: false,
            is_overlay_root: false,
            event_handlers: crate::event_handler::EventHandlers::new(),
            element_id: None,
            classes: Vec::new(),
            rotate_x: None,
            rotate_y: None,
            perspective_3d: None,
            shape_3d: None,
            depth: None,
            light_direction: None,
            light_intensity: None,
            ambient: None,
            specular: None,
            translate_z: None,
            op_3d: None,
            blend_3d: None,
            overflow_fade: OverflowFade::default(),
            clip_path: None,
            flow_name: None,
            flow_graph: None,
            outline_width: 0.0,
            outline_color: None,
            outline_offset: 0.0,
            is_fixed: false,
            is_sticky: false,
            sticky_top: None,
            z_index: 0,
            scroll_physics: None,
            layout_animation: None,
            visual_animation: None,
            stateful_context_key: None,
            pending_bindings: Vec::new(),
        }
    }

    /// Create a new div element with a pre-configured taffy Style
    ///
    /// This is useful for preserving layout properties (width, height, overflow, etc.)
    /// when rebuilding subtrees in Stateful elements.
    pub fn with_style(style: Style) -> Self {
        Self {
            style,
            children: Vec::new(),
            background: None,
            border_radius: CornerRadius::default(),
            border_radius_explicit: false,
            corner_shape: CornerShape::default(),
            corner_shape_locked: false,
            border_color: None,
            border_width: 0.0,
            border_sides: crate::element::BorderSides::default(),
            render_layer: RenderLayer::default(),
            material: None,
            shadow: Vec::new(),
            transform: None,
            transform_origin: None,
            opacity: 1.0,
            cursor: None,
            pointer_events_none: false,
            layer_effects: Vec::new(),
            is_stack_layer: false,
            is_overlay_root: false,
            event_handlers: crate::event_handler::EventHandlers::new(),
            element_id: None,
            classes: Vec::new(),
            rotate_x: None,
            rotate_y: None,
            perspective_3d: None,
            shape_3d: None,
            depth: None,
            light_direction: None,
            light_intensity: None,
            ambient: None,
            specular: None,
            translate_z: None,
            op_3d: None,
            blend_3d: None,
            overflow_fade: OverflowFade::default(),
            clip_path: None,
            flow_name: None,
            flow_graph: None,
            outline_width: 0.0,
            outline_color: None,
            outline_offset: 0.0,
            is_fixed: false,
            is_sticky: false,
            sticky_top: None,
            z_index: 0,
            scroll_physics: None,
            layout_animation: None,
            visual_animation: None,
            stateful_context_key: None,
            pending_bindings: Vec::new(),
        }
    }

    /// Set an element ID for selector API queries
    ///
    /// Elements with IDs can be looked up programmatically:
    /// ```rust,ignore
    /// div().id("my-container").child(...)
    ///
    /// // Later:
    /// if let Some(handle) = ctx.query("my-container") {
    ///     handle.scroll_into_view();
    /// }
    /// ```
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.element_id = Some(id.into());
        self
    }

    /// Get the element ID if set
    pub fn element_id(&self) -> Option<&str> {
        self.element_id.as_deref()
    }

    /// Add a CSS class name for selector matching
    ///
    /// Classes can be used with `.class` selectors in stylesheets.
    /// Multiple classes can be added by chaining `.class()` calls.
    /// Names are interned so a class like `"cn-button"` used by
    /// hundreds of nodes shares a single underlying allocation.
    pub fn class(mut self, name: impl AsRef<str>) -> Self {
        self.classes.push(blinc_core::intern::intern(name.as_ref()));
        self
    }

    /// Get the element's class list
    pub fn classes(&self) -> &[std::sync::Arc<str>] {
        &self.classes
    }

    /// Set the stateful context key for automatic key derivation
    ///
    /// This is typically set automatically by `stateful()` callbacks.
    /// When set, motion containers and layout animations will use this key
    /// as a prefix for auto-generated stable keys.
    pub(crate) fn with_stateful_context(mut self, key: impl Into<String>) -> Self {
        self.stateful_context_key = Some(key.into());
        self
    }

    /// Get the stateful context key if set
    pub fn stateful_context_key(&self) -> Option<&str> {
        self.stateful_context_key.as_deref()
    }

    /// Enable layout animation for this element
    ///
    /// # Deprecated
    ///
    /// Use [`animate_bounds()`](Self::animate_bounds) instead. The new system:
    /// - Never modifies taffy (read-only layout)
    /// - Tracks visual offsets that animate back to zero (FLIP technique)
    /// - Properly propagates parent animation offsets to children
    ///
    /// When enabled, changes to the element's bounds (position/size) after layout
    /// computation will be smoothly animated using spring physics instead of
    /// snapping instantly.
    ///
    /// This is useful for accordion/collapsible content, list reordering,
    /// and any UI where elements change size or position dynamically.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // Animate height changes with default spring
    /// div()
    ///     .animate_layout(LayoutAnimationConfig::height())
    ///     .child(content)
    ///
    /// // Animate all bounds with custom spring
    /// div()
    ///     .animate_layout(LayoutAnimationConfig::all().with_spring(SpringConfig::gentle()))
    ///     .child(content)
    /// ```
    #[deprecated(
        since = "0.3.0",
        note = "Use animate_bounds() instead. The old system modifies taffy which causes parent-child misalignment issues."
    )]
    pub fn animate_layout(
        mut self,
        config: crate::layout_animation::LayoutAnimationConfig,
    ) -> Self {
        // Auto-apply stable key if inside a stateful context
        let config = if let Some(ref ctx_key) = self.stateful_context_key {
            if config.stable_key.is_none() {
                let auto_key = format!("{}:layout_anim", ctx_key);
                config.with_key(auto_key)
            } else {
                config
            }
        } else {
            config
        };
        self.layout_animation = Some(config);
        self
    }

    /// Enable visual bounds animation using the new FLIP-style system
    ///
    /// Unlike `animate_layout()`, this system never modifies the layout tree.
    /// Instead, it tracks visual offsets and animates them back to zero.
    ///
    /// Key differences:
    /// - Layout runs once with final positions (taffy is read-only)
    /// - Animation tracks visual offset from layout position
    /// - Parent offsets propagate to children hierarchically
    /// - Pre-computed bounds used during rendering
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Animate height changes with FLIP-style animation
    /// div()
    ///     .animate_bounds(VisualAnimationConfig::height().with_key("my-content"))
    ///     .overflow_clip()
    ///     .child(content)
    /// ```
    pub fn animate_bounds(
        mut self,
        config: crate::visual_animation::VisualAnimationConfig,
    ) -> Self {
        // Auto-apply stable key if inside a stateful context
        let config = if let Some(ref ctx_key) = self.stateful_context_key {
            if config.key.is_none() {
                let auto_key = format!("{}:visual_anim", ctx_key);
                config.with_key(auto_key)
            } else {
                config
            }
        } else {
            config
        };
        self.visual_animation = Some(config);
        self
    }

    /// Wrap this Div in a Motion container with automatic key derivation
    ///
    /// If this Div is inside a stateful context (has `stateful_context_key` set),
    /// the Motion will use an auto-derived stable key. Otherwise, it falls back
    /// to a location-based key.
    ///
    /// # Example
    ///
    /// ```ignore
    /// stateful::<AccordionState>()
    ///     .on_state(|ctx| {
    ///         // This motion gets an auto-derived key like "stateful:...:motion"
    ///         div()
    ///             .motion()  // Auto-keyed!
    ///             .fade_in(200)
    ///             .child(content)
    ///     })
    /// ```
    #[track_caller]
    pub fn motion(self) -> crate::motion::Motion {
        if let Some(ref ctx_key) = self.stateful_context_key {
            // Auto-derive key from stateful context
            let motion_key = format!("{}:motion", ctx_key);
            crate::motion::motion_derived(&motion_key).child(self)
        } else {
            // Fallback to normal motion with location-based key
            crate::motion::motion().child(self)
        }
    }

    /// Swap this Div with a default, returning the original
    ///
    /// This is a convenience method for use in state callbacks where you need
    /// to consume `self` to chain builder methods, then assign back.
    ///
    /// **Note**: This takes ownership of the current Div and leaves a default in its place.
    /// All properties are preserved in the returned Div. You must assign the result back
    /// to complete the update.
    ///
    /// For updating specific properties without the swap pattern, consider using
    /// the setter methods directly (e.g., `set_bg()`, `set_transform()`).
    ///
    /// # Example
    ///
    /// ```ignore
    /// .on_state(|state, div| match state {
    ///     ButtonState::Idle => {
    ///         *div = div.swap().bg(Color::BLUE).rounded(4.0);
    ///     }
    ///     ButtonState::Hovered => {
    ///         *div = div.swap().bg(Color::CYAN).rounded(8.0);
    ///     }
    /// })
    /// ```
    #[inline]
    pub fn swap(&mut self) -> Self {
        std::mem::take(self)
    }

    /// Conditionally apply modifications based on a boolean condition
    ///
    /// This is useful for conditional styling without verbose if-else blocks.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // Instead of:
    /// let mut d = div();
    /// if is_collapsed {
    ///     d = d.h(0.0);
    /// }
    ///
    /// // Write:
    /// div().when(is_collapsed, |d| d.h(0.0))
    /// ```
    #[inline]
    pub fn when<F>(self, condition: bool, f: F) -> Self
    where
        F: FnOnce(Self) -> Self,
    {
        if condition { f(self) } else { self }
    }

    /// Set the background color/brush without consuming self
    ///
    /// This is useful in state callbacks where you want to update
    /// properties without using the swap pattern.
    #[inline]
    pub fn set_bg(&mut self, color: impl Into<Brush>) {
        self.background = Some(color.into());
    }

    /// Set the corner radius without consuming self
    #[inline]
    pub fn set_rounded(&mut self, radius: f32) {
        self.border_radius = CornerRadius::uniform(radius);
        self.border_radius_explicit = true;
    }

    /// Set the transform without consuming self
    #[inline]
    pub fn set_transform(&mut self, transform: Transform) {
        self.transform = Some(transform);
    }

    /// Set the shadow without consuming self (replaces any existing stack).
    #[inline]
    pub fn set_shadow(&mut self, shadow: Shadow) {
        self.shadow = vec![shadow];
    }

    /// Set a compound shadow stack without consuming self.
    #[inline]
    pub fn set_shadow_stack(&mut self, shadows: Vec<Shadow>) {
        self.shadow = shadows;
    }

    /// Set the opacity without consuming self
    #[inline]
    pub fn set_opacity(&mut self, opacity: f32) {
        self.opacity = opacity;
    }

    /// Set border with width and color without consuming self
    #[inline]
    pub fn set_border(&mut self, width: f32, color: Color) {
        self.border_width = width;
        self.border_color = Some(color);
    }

    /// Set overflow clip without consuming self
    #[inline]
    pub fn set_overflow_clip(&mut self, clip: bool) {
        if clip {
            self.style.overflow.x = taffy::Overflow::Hidden;
            self.style.overflow.y = taffy::Overflow::Hidden;
        } else {
            self.style.overflow.x = taffy::Overflow::Visible;
            self.style.overflow.y = taffy::Overflow::Visible;
        }
    }

    /// Set horizontal padding without consuming self
    #[inline]
    pub fn set_padding_x(&mut self, px: f32) {
        self.style.padding.left = taffy::LengthPercentage::Length(px);
        self.style.padding.right = taffy::LengthPercentage::Length(px);
    }

    /// Set vertical padding without consuming self
    #[inline]
    pub fn set_padding_y(&mut self, px: f32) {
        self.style.padding.top = taffy::LengthPercentage::Length(px);
        self.style.padding.bottom = taffy::LengthPercentage::Length(px);
    }

    /// Clear all children and add a single child
    #[inline]
    pub fn set_child(&mut self, child: impl ElementBuilder + 'static) {
        self.children.clear();
        self.children.push(Box::new(child));
    }

    /// Clear all children
    #[inline]
    pub fn clear_children(&mut self) {
        self.children.clear();
    }

    /// Set width in pixels without consuming self
    ///
    /// This is useful in state callbacks where you want to update
    /// layout properties without using the swap pattern.
    #[inline]
    pub fn set_w(&mut self, px: f32) {
        self.style.size.width = taffy::Dimension::Length(px);
    }

    /// Set height in pixels without consuming self
    #[inline]
    pub fn set_h(&mut self, px: f32) {
        self.style.size.height = taffy::Dimension::Length(px);
    }

    /// Set height to auto without consuming self
    #[inline]
    pub fn set_h_auto(&mut self) {
        self.style.size.height = taffy::Dimension::Auto;
    }

    // =========================================================================
    // ElementStyle Application
    // =========================================================================

    /// Apply an ElementStyle to this div
    ///
    /// Only properties that are set (Some) in the style will be applied.
    /// This allows for style composition and sharing reusable styles.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use blinc_layout::prelude::*;
    ///
    /// // Define a reusable card style
    /// let card_style = ElementStyle::new()
    ///     .bg_surface()
    ///     .rounded_lg()
    ///     .shadow_md();
    ///
    /// // Apply to multiple elements
    /// div().style(&card_style).child(text("Card 1"))
    /// div().style(&card_style).child(text("Card 2"))
    /// ```
    pub fn style(mut self, style: &ElementStyle) -> Self {
        self.set_style(style);
        self
    }

    /// Apply an ElementStyle without consuming self
    ///
    /// This is useful in state callbacks where you want to update
    /// styling without using the swap pattern.
    ///
    /// # Example
    ///
    /// ```ignore
    /// .on_state(|state, div| {
    ///     div.set_style(&hover_style);
    /// })
    /// ```
    #[inline]
    pub fn set_style(&mut self, style: &ElementStyle) {
        use crate::element_style::{
            SpacingRect, StyleAlign, StyleDisplay, StyleFlexDirection, StyleJustify, StyleOverflow,
        };

        // Visual properties
        if let Some(ref bg) = style.background {
            self.background = Some(bg.clone());
        }
        if let Some(radius) = style.corner_radius {
            self.border_radius = radius;
        }
        if let Some(cs) = style.corner_shape {
            self.corner_shape = cs;
        }
        if let Some(fade) = style.overflow_fade {
            self.overflow_fade = fade;
        }
        if !style.shadow.is_empty() {
            self.shadow = style.shadow.clone();
        }
        if let Some(ref transform) = style.transform {
            self.transform = Some(transform.clone());
        }
        if let Some(ref material) = style.material {
            self.material = Some(material.clone());
        }
        if let Some(layer) = style.render_layer {
            self.render_layer = layer;
        }
        if let Some(opacity) = style.opacity {
            self.opacity = opacity;
        }

        // Layout: sizing
        if let Some(w) = style.width {
            match w {
                crate::element_style::StyleDimension::Length(px) => {
                    self.style.size.width = Dimension::Length(px);
                }
                crate::element_style::StyleDimension::Percent(p) => {
                    self.style.size.width = Dimension::Percent(p);
                }
                crate::element_style::StyleDimension::Auto => {
                    self.style.size.width = Dimension::Auto;
                    self.style.flex_basis = Dimension::Auto;
                    self.style.flex_grow = 0.0;
                    self.style.flex_shrink = 0.0;
                }
            }
        }
        if let Some(h) = style.height {
            match h {
                crate::element_style::StyleDimension::Length(px) => {
                    self.style.size.height = Dimension::Length(px);
                }
                crate::element_style::StyleDimension::Percent(p) => {
                    self.style.size.height = Dimension::Percent(p);
                }
                crate::element_style::StyleDimension::Auto => {
                    self.style.size.height = Dimension::Auto;
                    self.style.flex_basis = Dimension::Auto;
                    self.style.flex_grow = 0.0;
                    self.style.flex_shrink = 0.0;
                }
            }
        }
        if let Some(w) = style.min_width {
            self.style.min_size.width = Dimension::Length(w);
        }
        if let Some(h) = style.min_height {
            self.style.min_size.height = Dimension::Length(h);
        }
        if let Some(w) = style.max_width {
            self.style.max_size.width = Dimension::Length(w);
        }
        if let Some(h) = style.max_height {
            self.style.max_size.height = Dimension::Length(h);
        }

        // Layout: display & flex direction
        if let Some(display) = style.display {
            self.style.display = match display {
                StyleDisplay::Flex => Display::Flex,
                StyleDisplay::Block => Display::Block,
                StyleDisplay::None => Display::None,
            };
        }
        if let Some(dir) = style.flex_direction {
            self.style.flex_direction = match dir {
                StyleFlexDirection::Row => FlexDirection::Row,
                StyleFlexDirection::Column => FlexDirection::Column,
                StyleFlexDirection::RowReverse => FlexDirection::RowReverse,
                StyleFlexDirection::ColumnReverse => FlexDirection::ColumnReverse,
            };
        }
        if let Some(wrap) = style.flex_wrap {
            self.style.flex_wrap = if wrap {
                FlexWrap::Wrap
            } else {
                FlexWrap::NoWrap
            };
        }
        if let Some(grow) = style.flex_grow {
            self.style.flex_grow = grow;
        }
        if let Some(shrink) = style.flex_shrink {
            self.style.flex_shrink = shrink;
        }

        // Layout: alignment
        if let Some(align) = style.align_items {
            self.style.align_items = Some(match align {
                StyleAlign::Start => AlignItems::Start,
                StyleAlign::Center => AlignItems::Center,
                StyleAlign::End => AlignItems::End,
                StyleAlign::Stretch => AlignItems::Stretch,
                StyleAlign::Baseline => AlignItems::Baseline,
            });
        }
        if let Some(justify) = style.justify_content {
            self.style.justify_content = Some(match justify {
                StyleJustify::Start => JustifyContent::Start,
                StyleJustify::Center => JustifyContent::Center,
                StyleJustify::End => JustifyContent::End,
                StyleJustify::SpaceBetween => JustifyContent::SpaceBetween,
                StyleJustify::SpaceAround => JustifyContent::SpaceAround,
                StyleJustify::SpaceEvenly => JustifyContent::SpaceEvenly,
            });
        }
        if let Some(align) = style.align_self {
            self.style.align_self = Some(match align {
                StyleAlign::Start => AlignSelf::Start,
                StyleAlign::Center => AlignSelf::Center,
                StyleAlign::End => AlignSelf::End,
                StyleAlign::Stretch => AlignSelf::Stretch,
                StyleAlign::Baseline => AlignSelf::Baseline,
            });
        }

        // Layout: spacing
        if let Some(SpacingRect {
            top,
            right,
            bottom,
            left,
        }) = style.padding
        {
            self.style.padding = Rect {
                top: LengthPercentage::Length(top),
                right: LengthPercentage::Length(right),
                bottom: LengthPercentage::Length(bottom),
                left: LengthPercentage::Length(left),
            };
        }
        if let Some(SpacingRect {
            top,
            right,
            bottom,
            left,
        }) = style.margin
        {
            self.style.margin = Rect {
                top: LengthPercentageAuto::Length(top),
                right: LengthPercentageAuto::Length(right),
                bottom: LengthPercentageAuto::Length(bottom),
                left: LengthPercentageAuto::Length(left),
            };
        }
        if let Some(gap) = style.gap {
            self.style.gap = taffy::Size {
                width: LengthPercentage::Length(gap),
                height: LengthPercentage::Length(gap),
            };
        }

        // Layout: overflow
        if let Some(overflow) = style.overflow {
            let val = match overflow {
                StyleOverflow::Visible => Overflow::Visible,
                StyleOverflow::Clip => Overflow::Clip,
                StyleOverflow::Scroll => Overflow::Scroll,
            };
            self.style.overflow.x = val;
            self.style.overflow.y = val;
            if overflow == StyleOverflow::Scroll {
                self.ensure_scroll_physics(crate::scroll::ScrollDirection::Both);
            }
        }
        // Per-axis overflow
        if let Some(ox) = style.overflow_x {
            let val = match ox {
                StyleOverflow::Visible => Overflow::Visible,
                StyleOverflow::Clip => Overflow::Clip,
                StyleOverflow::Scroll => Overflow::Scroll,
            };
            self.style.overflow.x = val;
            if ox == StyleOverflow::Scroll {
                self.ensure_scroll_physics(crate::scroll::ScrollDirection::Horizontal);
            }
        }
        if let Some(oy) = style.overflow_y {
            let val = match oy {
                StyleOverflow::Visible => Overflow::Visible,
                StyleOverflow::Clip => Overflow::Clip,
                StyleOverflow::Scroll => Overflow::Scroll,
            };
            self.style.overflow.y = val;
            if oy == StyleOverflow::Scroll {
                self.ensure_scroll_physics(crate::scroll::ScrollDirection::Vertical);
            }
        }

        // Layout: border
        if let Some(width) = style.border_width {
            self.border_width = width;
        }
        if let Some(color) = style.border_color {
            self.border_color = Some(color);
        }

        // 3D properties
        if let Some(v) = style.rotate_x {
            self.rotate_x = Some(v);
        }
        if let Some(v) = style.rotate_y {
            self.rotate_y = Some(v);
        }
        if let Some(v) = style.perspective {
            self.perspective_3d = Some(v);
        }
        if let Some(ref s) = style.shape_3d {
            self.shape_3d = Some(crate::css_parser::shape_3d_to_float(s));
        }
        if let Some(v) = style.depth {
            self.depth = Some(v);
        }
        if let Some(dir) = style.light_direction {
            self.light_direction = Some(dir);
        }
        if let Some(v) = style.light_intensity {
            self.light_intensity = Some(v);
        }
        if let Some(v) = style.ambient {
            self.ambient = Some(v);
        }
        if let Some(v) = style.specular {
            self.specular = Some(v);
        }
        if let Some(v) = style.translate_z {
            self.translate_z = Some(v);
        }
        if let Some(ref op) = style.op_3d {
            self.op_3d = Some(crate::css_parser::op_3d_to_float(op));
        }
        if let Some(v) = style.blend_3d {
            self.blend_3d = Some(v);
        }
        if let Some(ref cp) = style.clip_path {
            self.clip_path = Some(cp.clone());
        }
        if let Some(ref f) = style.flow {
            self.flow_name = Some(f.clone());
        }
    }

    /// Merge properties from another Div into this one
    ///
    /// This applies the other Div's non-default properties on top of this one.
    /// Useful in `on_state` callbacks to apply changes without reassignment:
    ///
    /// ```ignore
    /// .on_state(|state, div| {
    ///     div.merge(div().bg(color).child(label));
    /// })
    /// ```
    #[inline]
    pub fn merge(&mut self, other: Div) {
        // Create a default for comparison
        let default = Div::new();

        // Merge style if it differs from default
        // Style is complex, so we merge it field by field via taffy
        self.merge_style(&other.style, &default.style);

        // Merge render properties - take other's value if non-default
        if other.background.is_some() {
            self.background = other.background;
        }
        if other.border_radius_explicit || other.border_radius != default.border_radius {
            self.border_radius = other.border_radius;
            self.border_radius_explicit = true;
        }
        if other.border_color.is_some() {
            self.border_color = other.border_color;
        }
        if other.border_width != default.border_width {
            self.border_width = other.border_width;
        }
        if other.render_layer != default.render_layer {
            self.render_layer = other.render_layer;
        }
        if other.material.is_some() {
            self.material = other.material;
        }
        if !other.shadow.is_empty() {
            self.shadow = other.shadow.clone();
        }
        if other.transform.is_some() {
            self.transform = other.transform;
        }
        if other.opacity != default.opacity {
            self.opacity = other.opacity;
        }
        if other.cursor.is_some() {
            self.cursor = other.cursor;
        }

        // Merge outline (used by focus rings on inputs / textareas, and
        // anywhere `outline_*` builders are chained on a Div that then
        // gets `merge()`d into a parent — e.g. TextInput's on_state
        // callback builds an `inner` div with the focus outline and
        // merges it onto the Stateful container). Without these, the
        // outline silently vanishes mid-merge.
        if other.outline_width != default.outline_width {
            self.outline_width = other.outline_width;
        }
        if other.outline_color.is_some() {
            self.outline_color = other.outline_color;
        }
        if other.outline_offset != default.outline_offset {
            self.outline_offset = other.outline_offset;
        }

        // Merge element identity (ID and CSS classes)
        // These are critical for event routing (click-outside detection) and
        // CSS selector matching. Without this, Stateful containers lose the
        // element_id and classes set on the Div returned from on_state.
        if other.element_id.is_some() {
            self.element_id = other.element_id;
        }
        if !other.classes.is_empty() {
            self.classes = other.classes;
        }

        // Merge children - if other has children, replace ours
        if !other.children.is_empty() {
            self.children = other.children;
        }

        // Merge stateful context key - take other's if set
        if other.stateful_context_key.is_some() {
            self.stateful_context_key = other.stateful_context_key;
        }

        // Merge visual animation config - take other's if set
        if other.visual_animation.is_some() {
            self.visual_animation = other.visual_animation;
        }

        // Merge layout animation config (deprecated) - take other's if set
        if other.layout_animation.is_some() {
            self.layout_animation = other.layout_animation;
        }

        // Merge 3D properties
        if other.rotate_x.is_some() {
            self.rotate_x = other.rotate_x;
        }
        if other.rotate_y.is_some() {
            self.rotate_y = other.rotate_y;
        }
        if other.perspective_3d.is_some() {
            self.perspective_3d = other.perspective_3d;
        }
        if other.shape_3d.is_some() {
            self.shape_3d = other.shape_3d;
        }
        if other.depth.is_some() {
            self.depth = other.depth;
        }
        if other.light_direction.is_some() {
            self.light_direction = other.light_direction;
        }
        if other.light_intensity.is_some() {
            self.light_intensity = other.light_intensity;
        }
        if other.ambient.is_some() {
            self.ambient = other.ambient;
        }
        if other.specular.is_some() {
            self.specular = other.specular;
        }
        if other.translate_z.is_some() {
            self.translate_z = other.translate_z;
        }
        if other.op_3d.is_some() {
            self.op_3d = other.op_3d;
        }
        if other.blend_3d.is_some() {
            self.blend_3d = other.blend_3d;
        }
        if other.clip_path.is_some() {
            self.clip_path = other.clip_path;
        }
        if other.flow_name.is_some() {
            self.flow_name = other.flow_name;
            self.flow_graph = other.flow_graph;
        }

        // Merge scroll physics - take other's if set
        if other.scroll_physics.is_some() {
            self.scroll_physics = other.scroll_physics;
        }

        // Merge event handlers - combine both sets so scroll handlers etc. survive
        if !other.event_handlers.is_empty() {
            self.event_handlers.merge(other.event_handlers);
        }

        // Append signal-bound property bindings. Both sides keep their
        // subscriptions; merge concatenates rather than replaces so a
        // cn widget that mixes its own bindings with the user's
        // `.bg(&signal)` doesn't drop either set.
        self.pending_bindings.extend(other.pending_bindings);
    }

    /// Merge taffy Style fields from other if they differ from default
    fn merge_style(&mut self, other: &Style, default: &Style) {
        // Display & position
        if other.display != default.display {
            self.style.display = other.display;
        }
        if other.position != default.position {
            self.style.position = other.position;
        }
        if other.overflow != default.overflow {
            self.style.overflow = other.overflow;
        }

        // Flex container properties
        if other.flex_direction != default.flex_direction {
            self.style.flex_direction = other.flex_direction;
        }
        if other.flex_wrap != default.flex_wrap {
            self.style.flex_wrap = other.flex_wrap;
        }
        if other.justify_content != default.justify_content {
            self.style.justify_content = other.justify_content;
        }
        if other.align_items != default.align_items {
            self.style.align_items = other.align_items;
        }
        if other.align_content != default.align_content {
            self.style.align_content = other.align_content;
        }
        // Gap - merge per axis
        if other.gap.width != default.gap.width {
            self.style.gap.width = other.gap.width;
        }
        if other.gap.height != default.gap.height {
            self.style.gap.height = other.gap.height;
        }

        // Flex item properties
        if other.flex_grow != default.flex_grow {
            self.style.flex_grow = other.flex_grow;
        }
        if other.flex_shrink != default.flex_shrink {
            self.style.flex_shrink = other.flex_shrink;
        }
        if other.flex_basis != default.flex_basis {
            self.style.flex_basis = other.flex_basis;
        }
        if other.align_self != default.align_self {
            self.style.align_self = other.align_self;
        }

        // Size constraints - merge per dimension to allow w() then h() separately
        if other.size.width != default.size.width {
            self.style.size.width = other.size.width;
        }
        if other.size.height != default.size.height {
            self.style.size.height = other.size.height;
        }
        if other.min_size.width != default.min_size.width {
            self.style.min_size.width = other.min_size.width;
        }
        if other.min_size.height != default.min_size.height {
            self.style.min_size.height = other.min_size.height;
        }
        if other.max_size.width != default.max_size.width {
            self.style.max_size.width = other.max_size.width;
        }
        if other.max_size.height != default.max_size.height {
            self.style.max_size.height = other.max_size.height;
        }
        if other.aspect_ratio != default.aspect_ratio {
            self.style.aspect_ratio = other.aspect_ratio;
        }

        // Spacing - merge per side to allow partial updates (e.g., px then py)
        // Margin
        if other.margin.left != default.margin.left {
            self.style.margin.left = other.margin.left;
        }
        if other.margin.right != default.margin.right {
            self.style.margin.right = other.margin.right;
        }
        if other.margin.top != default.margin.top {
            self.style.margin.top = other.margin.top;
        }
        if other.margin.bottom != default.margin.bottom {
            self.style.margin.bottom = other.margin.bottom;
        }
        // Padding
        if other.padding.left != default.padding.left {
            self.style.padding.left = other.padding.left;
        }
        if other.padding.right != default.padding.right {
            self.style.padding.right = other.padding.right;
        }
        if other.padding.top != default.padding.top {
            self.style.padding.top = other.padding.top;
        }
        if other.padding.bottom != default.padding.bottom {
            self.style.padding.bottom = other.padding.bottom;
        }
        // Border
        if other.border.left != default.border.left {
            self.style.border.left = other.border.left;
        }
        if other.border.right != default.border.right {
            self.style.border.right = other.border.right;
        }
        if other.border.top != default.border.top {
            self.style.border.top = other.border.top;
        }
        if other.border.bottom != default.border.bottom {
            self.style.border.bottom = other.border.bottom;
        }

        // Inset (for absolute positioning) - merge per side
        if other.inset.left != default.inset.left {
            self.style.inset.left = other.inset.left;
        }
        if other.inset.right != default.inset.right {
            self.style.inset.right = other.inset.right;
        }
        if other.inset.top != default.inset.top {
            self.style.inset.top = other.inset.top;
        }
        if other.inset.bottom != default.inset.bottom {
            self.style.inset.bottom = other.inset.bottom;
        }
    }

    // =========================================================================
    // Display & Flex Direction
    // =========================================================================

    /// Set display to flex (default)
    pub fn flex(mut self) -> Self {
        self.style.display = Display::Flex;
        self
    }

    /// Set display to block
    pub fn block(mut self) -> Self {
        self.style.display = Display::Block;
        self
    }

    /// Set display to grid
    pub fn grid(mut self) -> Self {
        self.style.display = Display::Grid;
        self
    }

    /// Set display to none
    pub fn hidden(mut self) -> Self {
        self.style.display = Display::None;
        self
    }

    /// Set flex direction to row (horizontal)
    pub fn flex_row(mut self) -> Self {
        self.style.display = Display::Flex;
        self.style.flex_direction = FlexDirection::Row;
        self
    }

    /// Set flex direction to column (vertical)
    pub fn flex_col(mut self) -> Self {
        self.style.display = Display::Flex;
        self.style.flex_direction = FlexDirection::Column;
        self
    }

    /// Set flex direction to row-reverse
    pub fn flex_row_reverse(mut self) -> Self {
        self.style.display = Display::Flex;
        self.style.flex_direction = FlexDirection::RowReverse;
        self
    }

    /// Set flex direction to column-reverse
    pub fn flex_col_reverse(mut self) -> Self {
        self.style.display = Display::Flex;
        self.style.flex_direction = FlexDirection::ColumnReverse;
        self
    }

    // =========================================================================
    // Flex Properties
    // =========================================================================

    /// Set flex-grow to 1 (element will grow to fill space)
    pub fn flex_grow(mut self) -> Self {
        self.style.flex_grow = 1.0;
        self
    }

    /// Set flex-grow to a specific value
    ///
    /// Use this for proportional sizing. For example, an element with flex_grow_value(2.0)
    /// will grow to twice the size of an element with flex_grow_value(1.0).
    pub fn flex_grow_value(mut self, value: f32) -> Self {
        self.style.flex_grow = value;
        self
    }

    /// Set flex-shrink to 1 (element will shrink if needed)
    pub fn flex_shrink(mut self) -> Self {
        self.style.flex_shrink = 1.0;
        self
    }

    /// Set flex-shrink to 0 (element won't shrink)
    pub fn flex_shrink_0(mut self) -> Self {
        self.style.flex_shrink = 0.0;
        self
    }

    /// Set flex-basis to auto
    pub fn flex_auto(mut self) -> Self {
        self.style.flex_grow = 1.0;
        self.style.flex_shrink = 1.0;
        self.style.flex_basis = Dimension::Auto;
        self
    }

    /// Set flex: 1 1 0% (grow, shrink, basis 0)
    pub fn flex_1(mut self) -> Self {
        self.style.flex_grow = 1.0;
        self.style.flex_shrink = 1.0;
        self.style.flex_basis = Dimension::Length(0.0);
        self
    }

    /// Allow wrapping
    pub fn flex_wrap(mut self) -> Self {
        self.style.flex_wrap = FlexWrap::Wrap;
        self
    }

    // =========================================================================
    // Alignment & Justification
    // =========================================================================

    /// Center items both horizontally and vertically
    pub fn items_center(mut self) -> Self {
        self.style.align_items = Some(AlignItems::Center);
        self
    }

    /// Align items to start
    pub fn items_start(mut self) -> Self {
        self.style.align_items = Some(AlignItems::Start);
        self
    }

    /// Align items to end
    pub fn items_end(mut self) -> Self {
        self.style.align_items = Some(AlignItems::End);
        self
    }

    /// Stretch items to fill (default)
    pub fn items_stretch(mut self) -> Self {
        self.style.align_items = Some(AlignItems::Stretch);
        self
    }

    /// Align items to baseline
    pub fn items_baseline(mut self) -> Self {
        self.style.align_items = Some(AlignItems::Baseline);
        self
    }

    /// Align self to start (overrides parent's align_items for this element)
    pub fn align_self_start(mut self) -> Self {
        self.style.align_self = Some(AlignSelf::Start);
        self
    }

    /// Align self to center (overrides parent's align_items for this element)
    pub fn align_self_center(mut self) -> Self {
        self.style.align_self = Some(AlignSelf::Center);
        self
    }

    /// Align self to end (overrides parent's align_items for this element)
    pub fn align_self_end(mut self) -> Self {
        self.style.align_self = Some(AlignSelf::End);
        self
    }

    /// Stretch self to fill (overrides parent's align_items for this element)
    pub fn align_self_stretch(mut self) -> Self {
        self.style.align_self = Some(AlignSelf::Stretch);
        self
    }

    /// Justify content to start
    pub fn justify_start(mut self) -> Self {
        self.style.justify_content = Some(JustifyContent::Start);
        self
    }

    /// Justify content to center
    pub fn justify_center(mut self) -> Self {
        self.style.justify_content = Some(JustifyContent::Center);
        self
    }

    /// Justify content to end
    pub fn justify_end(mut self) -> Self {
        self.style.justify_content = Some(JustifyContent::End);
        self
    }

    /// Space between items
    pub fn justify_between(mut self) -> Self {
        self.style.justify_content = Some(JustifyContent::SpaceBetween);
        self
    }

    /// Space around items
    pub fn justify_around(mut self) -> Self {
        self.style.justify_content = Some(JustifyContent::SpaceAround);
        self
    }

    /// Space evenly between items
    pub fn justify_evenly(mut self) -> Self {
        self.style.justify_content = Some(JustifyContent::SpaceEvenly);
        self
    }

    /// Align content to start (for multi-line flex containers)
    pub fn content_start(mut self) -> Self {
        self.style.align_content = Some(taffy::AlignContent::Start);
        self
    }

    /// Align content to center (for multi-line flex containers)
    pub fn content_center(mut self) -> Self {
        self.style.align_content = Some(taffy::AlignContent::Center);
        self
    }

    /// Align content to end (for multi-line flex containers)
    pub fn content_end(mut self) -> Self {
        self.style.align_content = Some(taffy::AlignContent::End);
        self
    }

    /// Align this element to start on cross-axis (overrides parent's align-items)
    ///
    /// In a flex-row parent, this prevents height stretching.
    /// In a flex-col parent, this prevents width stretching.
    pub fn self_start(mut self) -> Self {
        self.style.align_self = Some(AlignSelf::FlexStart);
        self
    }

    /// Align this element to center on cross-axis (overrides parent's align-items)
    pub fn self_center(mut self) -> Self {
        self.style.align_self = Some(AlignSelf::Center);
        self
    }

    /// Align this element to end on cross-axis (overrides parent's align-items)
    pub fn self_end(mut self) -> Self {
        self.style.align_self = Some(AlignSelf::FlexEnd);
        self
    }

    /// Stretch this element on cross-axis (overrides parent's align-items)
    pub fn self_stretch(mut self) -> Self {
        self.style.align_self = Some(AlignSelf::Stretch);
        self
    }

    // =========================================================================
    // Sizing (pixel values)
    // =========================================================================

    /// Set width in pixels.
    ///
    /// Accepts either an eager `f32` or a `&State<f32>` / `State<f32>`
    /// for signal-bound reactivity. Signal updates patch the taffy
    /// `Style.size.width` directly via the property channel and trigger
    /// `compute_layout` next frame — no `Stateful` wrap, no subtree
    /// rebuild for visual-only ancestors.
    /// ([[project-reactive-architecture-v2]] Phase 2.4.)
    pub fn w(mut self, value: impl crate::binding::IntoReactive<f32>) -> Self {
        use crate::binding::{LayoutPendingBinding, Reactive};
        match value.into_reactive() {
            Reactive::Const(px) => {
                self.style.size.width = Dimension::Length(px);
            }
            Reactive::Bound(state) => {
                if let Some(px) = state.try_get() {
                    self.style.size.width = Dimension::Length(px);
                }
                self.pending_bindings
                    .push(Box::new(LayoutPendingBinding::new(
                        state,
                        crate::property::PropertyId::Width,
                        |style, px: f32| {
                            style.size.width = Dimension::Length(px);
                        },
                    )));
            }
            Reactive::Computed(computed) => {
                if let Some(px) = computed.try_get() {
                    self.style.size.width = Dimension::Length(px);
                }
                self.pending_bindings
                    .push(Box::new(LayoutPendingBinding::from_computed(
                        computed,
                        crate::property::PropertyId::Width,
                        |style, px: f32| {
                            style.size.width = Dimension::Length(px);
                        },
                    )));
            }
        }
        self
    }

    /// Set width to 100%
    pub fn w_full(mut self) -> Self {
        self.style.size.width = Dimension::Percent(1.0);
        self
    }

    /// Set width to auto
    pub fn w_auto(mut self) -> Self {
        self.style.size.width = Dimension::Auto;
        self
    }

    /// Set width to fit content (shrink-wrap to children).
    ///
    /// Sets `size.width = Auto`, `flex_grow = 0`, `flex_shrink = 0` so the
    /// element doesn't grow or shrink from its intrinsic width along a
    /// flex-row parent's main axis. Also forces `align_self = Start` so
    /// that inside a flex-column parent — where width is the *cross*
    /// axis — the parent's default `align_items: Stretch` doesn't
    /// stretch the child back out to the container's width. Does NOT
    /// touch `flex_basis`: that field is per-axis (main axis only) and
    /// setting it here would clobber sibling-axis sizing when parent
    /// direction and fit axis disagree.
    pub fn w_fit(mut self) -> Self {
        self.style.size.width = Dimension::Auto;
        self.style.flex_grow = 0.0;
        self.style.flex_shrink = 0.0;
        self.style.align_self = Some(AlignSelf::Start);
        self
    }

    /// Set height in pixels.
    ///
    /// Accepts either an eager `f32` or a `&State<f32>` / `State<f32>`
    /// for signal-bound reactivity. Same shape as [`Self::w`].
    pub fn h(mut self, value: impl crate::binding::IntoReactive<f32>) -> Self {
        use crate::binding::{LayoutPendingBinding, Reactive};
        match value.into_reactive() {
            Reactive::Const(px) => {
                self.style.size.height = Dimension::Length(px);
            }
            Reactive::Bound(state) => {
                if let Some(px) = state.try_get() {
                    self.style.size.height = Dimension::Length(px);
                }
                self.pending_bindings
                    .push(Box::new(LayoutPendingBinding::new(
                        state,
                        crate::property::PropertyId::Height,
                        |style, px: f32| {
                            style.size.height = Dimension::Length(px);
                        },
                    )));
            }
            Reactive::Computed(computed) => {
                if let Some(px) = computed.try_get() {
                    self.style.size.height = Dimension::Length(px);
                }
                self.pending_bindings
                    .push(Box::new(LayoutPendingBinding::from_computed(
                        computed,
                        crate::property::PropertyId::Height,
                        |style, px: f32| {
                            style.size.height = Dimension::Length(px);
                        },
                    )));
            }
        }
        self
    }

    /// Set height to 100%
    pub fn h_full(mut self) -> Self {
        self.style.size.height = Dimension::Percent(1.0);
        self
    }

    /// Set height to auto
    pub fn h_auto(mut self) -> Self {
        self.style.size.height = Dimension::Auto;
        self
    }

    /// Set height to fit content (shrink-wrap to children).
    ///
    /// Sets `size.height = Auto`, `flex_grow = 0`, `flex_shrink = 0` so the
    /// element doesn't grow or shrink from its intrinsic height along a
    /// flex-column parent's main axis. Also forces `align_self = Start`
    /// so that inside a flex-row parent — where height is the *cross*
    /// axis — the parent's default `align_items: Stretch` doesn't
    /// stretch the child back out to the container's height. Does NOT
    /// touch `flex_basis`: that field is per-axis (main axis only) and
    /// setting it here would clobber sibling-axis sizing when parent
    /// direction and fit axis disagree.
    pub fn h_fit(mut self) -> Self {
        self.style.size.height = Dimension::Auto;
        self.style.flex_grow = 0.0;
        self.style.flex_shrink = 0.0;
        self.style.align_self = Some(AlignSelf::Start);
        self
    }

    /// Set both width and height to fit content.
    ///
    /// Shrink-wraps in both dimensions regardless of parent flex
    /// direction. See [`Self::w_fit`] / [`Self::h_fit`] for the
    /// rationale on `align_self` and the `flex_basis` omission.
    pub fn size_fit(mut self) -> Self {
        self.style.size.width = Dimension::Auto;
        self.style.size.height = Dimension::Auto;
        self.style.flex_grow = 0.0;
        self.style.flex_shrink = 0.0;
        self.style.align_self = Some(AlignSelf::Start);
        self
    }

    /// Set both width and height in pixels
    pub fn size(mut self, w: f32, h: f32) -> Self {
        self.style.size.width = Dimension::Length(w);
        self.style.size.height = Dimension::Length(h);
        self
    }

    /// Set square size (width and height equal)
    pub fn square(mut self, size: f32) -> Self {
        self.style.size.width = Dimension::Length(size);
        self.style.size.height = Dimension::Length(size);
        self
    }

    /// Set min-width in pixels
    pub fn min_w(mut self, px: f32) -> Self {
        self.style.min_size.width = Dimension::Length(px);
        self
    }

    /// Set min-height in pixels
    pub fn min_h(mut self, px: f32) -> Self {
        self.style.min_size.height = Dimension::Length(px);
        self
    }

    /// Set max-width in pixels
    pub fn max_w(mut self, px: f32) -> Self {
        self.style.max_size.width = Dimension::Length(px);
        self
    }

    /// Set max-height in pixels
    pub fn max_h(mut self, px: f32) -> Self {
        self.style.max_size.height = Dimension::Length(px);
        self
    }

    // =========================================================================
    // Spacing (4px base unit like Tailwind)
    // =========================================================================

    /// Set gap between children (in 4px units).
    /// `gap(4)` = 16 px.
    ///
    /// Accepts either an eager `f32` or a `&State<f32>` / `State<f32>`.
    /// Signal updates patch the taffy `Style.gap` and trigger relayout.
    pub fn gap(mut self, value: impl crate::binding::IntoReactive<f32>) -> Self {
        use crate::binding::{LayoutPendingBinding, Reactive};
        match value.into_reactive() {
            Reactive::Const(units) => {
                let px = units * 4.0;
                self.style.gap = taffy::Size {
                    width: LengthPercentage::Length(px),
                    height: LengthPercentage::Length(px),
                };
            }
            Reactive::Bound(state) => {
                if let Some(units) = state.try_get() {
                    let px = units * 4.0;
                    self.style.gap = taffy::Size {
                        width: LengthPercentage::Length(px),
                        height: LengthPercentage::Length(px),
                    };
                }
                self.pending_bindings
                    .push(Box::new(LayoutPendingBinding::new(
                        state,
                        crate::property::PropertyId::Gap,
                        |style, units: f32| {
                            let px = units * 4.0;
                            style.gap = taffy::Size {
                                width: LengthPercentage::Length(px),
                                height: LengthPercentage::Length(px),
                            };
                        },
                    )));
            }
            Reactive::Computed(computed) => {
                if let Some(units) = computed.try_get() {
                    let px = units * 4.0;
                    self.style.gap = taffy::Size {
                        width: LengthPercentage::Length(px),
                        height: LengthPercentage::Length(px),
                    };
                }
                self.pending_bindings
                    .push(Box::new(LayoutPendingBinding::from_computed(
                        computed,
                        crate::property::PropertyId::Gap,
                        |style, units: f32| {
                            let px = units * 4.0;
                            style.gap = taffy::Size {
                                width: LengthPercentage::Length(px),
                                height: LengthPercentage::Length(px),
                            };
                        },
                    )));
            }
        }
        self
    }

    /// Set gap in pixels directly
    pub fn gap_px(mut self, px: f32) -> Self {
        self.style.gap = taffy::Size {
            width: LengthPercentage::Length(px),
            height: LengthPercentage::Length(px),
        };
        self
    }

    /// Set column gap (horizontal spacing between items)
    pub fn gap_x(mut self, units: f32) -> Self {
        self.style.gap.width = LengthPercentage::Length(units * 4.0);
        self
    }

    /// Set row gap (vertical spacing between items)
    pub fn gap_y(mut self, units: f32) -> Self {
        self.style.gap.height = LengthPercentage::Length(units * 4.0);
        self
    }

    // -------------------------------------------------------------------------
    // Theme-based gap spacing
    // -------------------------------------------------------------------------

    /// Set gap using theme spacing scale (space_1 = 4px)
    pub fn gap_1(self) -> Self {
        self.gap_px(ThemeState::get().spacing().space_1)
    }

    /// Set gap using theme spacing scale (space_2 = 8px)
    pub fn gap_2(self) -> Self {
        self.gap_px(ThemeState::get().spacing().space_2)
    }

    /// Set gap using theme spacing scale (space_3 = 12px)
    pub fn gap_3(self) -> Self {
        self.gap_px(ThemeState::get().spacing().space_3)
    }

    /// Set gap using theme spacing scale (space_4 = 16px)
    pub fn gap_4(self) -> Self {
        self.gap_px(ThemeState::get().spacing().space_4)
    }

    /// Set gap using theme spacing scale (space_5 = 20px)
    pub fn gap_5(self) -> Self {
        self.gap_px(ThemeState::get().spacing().space_5)
    }

    /// Set gap using theme spacing scale (space_6 = 24px)
    pub fn gap_6(self) -> Self {
        self.gap_px(ThemeState::get().spacing().space_6)
    }

    /// Set gap using theme spacing scale (space_8 = 32px)
    pub fn gap_8(self) -> Self {
        self.gap_px(ThemeState::get().spacing().space_8)
    }

    /// Set gap using theme spacing scale (space_10 = 40px)
    pub fn gap_10(self) -> Self {
        self.gap_px(ThemeState::get().spacing().space_10)
    }

    /// Set gap using theme spacing scale (space_12 = 48px)
    pub fn gap_12(self) -> Self {
        self.gap_px(ThemeState::get().spacing().space_12)
    }

    // -------------------------------------------------------------------------
    // Padding
    // -------------------------------------------------------------------------

    /// Set padding on all sides (in 4px units).
    /// `p(4)` = 16 px padding.
    ///
    /// Accepts either an eager `f32` or a `&State<f32>` / `State<f32>`.
    /// Signal updates patch the taffy `Style.padding` and trigger
    /// relayout.
    pub fn p(mut self, value: impl crate::binding::IntoReactive<f32>) -> Self {
        use crate::binding::{LayoutPendingBinding, Reactive};
        match value.into_reactive() {
            Reactive::Const(units) => {
                let px = LengthPercentage::Length(units * 4.0);
                self.style.padding = Rect {
                    left: px,
                    right: px,
                    top: px,
                    bottom: px,
                };
            }
            Reactive::Bound(state) => {
                if let Some(units) = state.try_get() {
                    let px = LengthPercentage::Length(units * 4.0);
                    self.style.padding = Rect {
                        left: px,
                        right: px,
                        top: px,
                        bottom: px,
                    };
                }
                self.pending_bindings
                    .push(Box::new(LayoutPendingBinding::new(
                        state,
                        crate::property::PropertyId::Padding,
                        |style, units: f32| {
                            let px = LengthPercentage::Length(units * 4.0);
                            style.padding = Rect {
                                left: px,
                                right: px,
                                top: px,
                                bottom: px,
                            };
                        },
                    )));
            }
            Reactive::Computed(computed) => {
                if let Some(units) = computed.try_get() {
                    let px = LengthPercentage::Length(units * 4.0);
                    self.style.padding = Rect {
                        left: px,
                        right: px,
                        top: px,
                        bottom: px,
                    };
                }
                self.pending_bindings
                    .push(Box::new(LayoutPendingBinding::from_computed(
                        computed,
                        crate::property::PropertyId::Padding,
                        |style, units: f32| {
                            let px = LengthPercentage::Length(units * 4.0);
                            style.padding = Rect {
                                left: px,
                                right: px,
                                top: px,
                                bottom: px,
                            };
                        },
                    )));
            }
        }
        self
    }

    /// Set padding in pixels
    pub fn p_px(mut self, px: f32) -> Self {
        let val = LengthPercentage::Length(px);
        self.style.padding = Rect {
            left: val,
            right: val,
            top: val,
            bottom: val,
        };
        self
    }

    /// Set horizontal padding (in 4px units)
    pub fn px(mut self, units: f32) -> Self {
        let px = LengthPercentage::Length(units * 4.0);
        self.style.padding.left = px;
        self.style.padding.right = px;
        self
    }

    /// Set vertical padding (in 4px units)
    pub fn py(mut self, units: f32) -> Self {
        let px = LengthPercentage::Length(units * 4.0);
        self.style.padding.top = px;
        self.style.padding.bottom = px;
        self
    }

    /// Set left padding (in 4px units)
    pub fn pl(mut self, units: f32) -> Self {
        self.style.padding.left = LengthPercentage::Length(units * 4.0);
        self
    }

    /// Set right padding (in 4px units)
    pub fn pr(mut self, units: f32) -> Self {
        self.style.padding.right = LengthPercentage::Length(units * 4.0);
        self
    }

    /// Set top padding (in 4px units)
    pub fn pt(mut self, units: f32) -> Self {
        self.style.padding.top = LengthPercentage::Length(units * 4.0);
        self
    }

    /// Set bottom padding (in 4px units)
    pub fn pb(mut self, units: f32) -> Self {
        self.style.padding.bottom = LengthPercentage::Length(units * 4.0);
        self
    }

    // -------------------------------------------------------------------------
    // Theme-based padding
    // -------------------------------------------------------------------------

    /// Set padding using theme spacing scale (space_1 = 4px)
    pub fn p_1(self) -> Self {
        let px = ThemeState::get().spacing().space_1;
        self.padding(crate::units::Length::Px(px))
    }

    /// Set padding using theme spacing scale (space_2 = 8px)
    pub fn p_2(self) -> Self {
        let px = ThemeState::get().spacing().space_2;
        self.padding(crate::units::Length::Px(px))
    }

    /// Set padding using theme spacing scale (space_3 = 12px)
    pub fn p_3(self) -> Self {
        let px = ThemeState::get().spacing().space_3;
        self.padding(crate::units::Length::Px(px))
    }

    /// Set padding using theme spacing scale (space_4 = 16px)
    pub fn p_4(self) -> Self {
        let px = ThemeState::get().spacing().space_4;
        self.padding(crate::units::Length::Px(px))
    }

    /// Set padding using theme spacing scale (space_5 = 20px)
    pub fn p_5(self) -> Self {
        let px = ThemeState::get().spacing().space_5;
        self.padding(crate::units::Length::Px(px))
    }

    /// Set padding using theme spacing scale (space_6 = 24px)
    pub fn p_6(self) -> Self {
        let px = ThemeState::get().spacing().space_6;
        self.padding(crate::units::Length::Px(px))
    }

    /// Set padding using theme spacing scale (space_8 = 32px)
    pub fn p_8(self) -> Self {
        let px = ThemeState::get().spacing().space_8;
        self.padding(crate::units::Length::Px(px))
    }

    /// Set padding using theme spacing scale (space_10 = 40px)
    pub fn p_10(self) -> Self {
        let px = ThemeState::get().spacing().space_10;
        self.padding(crate::units::Length::Px(px))
    }

    /// Set padding using theme spacing scale (space_12 = 48px)
    pub fn p_12(self) -> Self {
        let px = ThemeState::get().spacing().space_12;
        self.padding(crate::units::Length::Px(px))
    }

    // =========================================================================
    // Raw Pixel Padding (for internal/widget use)
    // =========================================================================

    /// Set horizontal padding in raw pixels (no unit conversion)
    pub fn padding_x_px(mut self, pixels: f32) -> Self {
        let px = LengthPercentage::Length(pixels);
        self.style.padding.left = px;
        self.style.padding.right = px;
        self
    }

    /// Set vertical padding in raw pixels (no unit conversion)
    pub fn padding_y_px(mut self, pixels: f32) -> Self {
        let px = LengthPercentage::Length(pixels);
        self.style.padding.top = px;
        self.style.padding.bottom = px;
        self
    }

    // =========================================================================
    // Semantic Length-based API (uses Length type for clarity)
    // =========================================================================

    /// Set padding using a semantic Length value
    ///
    /// # Examples
    /// ```rust,ignore
    /// use blinc_layout::prelude::*;
    ///
    /// div().padding(px(16.0))  // 16 raw pixels
    /// div().padding(sp(4.0))   // 4 spacing units = 16px
    /// ```
    pub fn padding(mut self, len: crate::units::Length) -> Self {
        let val: LengthPercentage = len.into();
        self.style.padding = Rect {
            left: val,
            right: val,
            top: val,
            bottom: val,
        };
        self
    }

    /// Set horizontal padding using a semantic Length value
    pub fn padding_x(mut self, len: crate::units::Length) -> Self {
        let val: LengthPercentage = len.into();
        self.style.padding.left = val;
        self.style.padding.right = val;
        self
    }

    /// Set vertical padding using a semantic Length value
    pub fn padding_y(mut self, len: crate::units::Length) -> Self {
        let val: LengthPercentage = len.into();
        self.style.padding.top = val;
        self.style.padding.bottom = val;
        self
    }

    /// Set margin on all sides (in 4px units)
    pub fn m(mut self, units: f32) -> Self {
        let px = LengthPercentageAuto::Length(units * 4.0);
        self.style.margin = Rect {
            left: px,
            right: px,
            top: px,
            bottom: px,
        };
        self
    }

    /// Set margin in pixels
    pub fn m_px(mut self, px: f32) -> Self {
        let val = LengthPercentageAuto::Length(px);
        self.style.margin = Rect {
            left: val,
            right: val,
            top: val,
            bottom: val,
        };
        self
    }

    /// Set horizontal margin (in 4px units)
    pub fn mx(mut self, units: f32) -> Self {
        let px = LengthPercentageAuto::Length(units * 4.0);
        self.style.margin.left = px;
        self.style.margin.right = px;
        self
    }

    /// Set vertical margin (in 4px units)
    pub fn my(mut self, units: f32) -> Self {
        let px = LengthPercentageAuto::Length(units * 4.0);
        self.style.margin.top = px;
        self.style.margin.bottom = px;
        self
    }

    /// Set auto horizontal margin (centering)
    pub fn mx_auto(mut self) -> Self {
        self.style.margin.left = LengthPercentageAuto::Auto;
        self.style.margin.right = LengthPercentageAuto::Auto;
        self
    }

    /// Set left margin (in 4px units)
    pub fn ml(mut self, units: f32) -> Self {
        self.style.margin.left = LengthPercentageAuto::Length(units * 4.0);
        self
    }

    /// Set right margin (in 4px units)
    pub fn mr(mut self, units: f32) -> Self {
        self.style.margin.right = LengthPercentageAuto::Length(units * 4.0);
        self
    }

    /// Set top margin (in 4px units)
    pub fn mt(mut self, units: f32) -> Self {
        self.style.margin.top = LengthPercentageAuto::Length(units * 4.0);
        self
    }

    /// Set bottom margin (in 4px units)
    pub fn mb(mut self, units: f32) -> Self {
        self.style.margin.bottom = LengthPercentageAuto::Length(units * 4.0);
        self
    }

    // -------------------------------------------------------------------------
    // Theme-based margin
    // -------------------------------------------------------------------------

    /// Set margin using theme spacing scale (space_1 = 4px)
    pub fn m_1(self) -> Self {
        self.m_px(ThemeState::get().spacing().space_1)
    }

    /// Set margin using theme spacing scale (space_2 = 8px)
    pub fn m_2(self) -> Self {
        self.m_px(ThemeState::get().spacing().space_2)
    }

    /// Set margin using theme spacing scale (space_3 = 12px)
    pub fn m_3(self) -> Self {
        self.m_px(ThemeState::get().spacing().space_3)
    }

    /// Set margin using theme spacing scale (space_4 = 16px)
    pub fn m_4(self) -> Self {
        self.m_px(ThemeState::get().spacing().space_4)
    }

    /// Set margin using theme spacing scale (space_5 = 20px)
    pub fn m_5(self) -> Self {
        self.m_px(ThemeState::get().spacing().space_5)
    }

    /// Set margin using theme spacing scale (space_6 = 24px)
    pub fn m_6(self) -> Self {
        self.m_px(ThemeState::get().spacing().space_6)
    }

    /// Set margin using theme spacing scale (space_8 = 32px)
    pub fn m_8(self) -> Self {
        self.m_px(ThemeState::get().spacing().space_8)
    }

    // =========================================================================
    // Position
    // =========================================================================

    /// Set position to absolute
    pub fn absolute(mut self) -> Self {
        self.style.position = Position::Absolute;
        self
    }

    /// Set position to relative (default)
    pub fn relative(mut self) -> Self {
        self.style.position = Position::Relative;
        self
    }

    /// Set position to fixed (stays in place when ancestors scroll)
    pub fn fixed(mut self) -> Self {
        self.style.position = Position::Absolute;
        self.is_fixed = true;
        self.is_sticky = false;
        self
    }

    /// Set position to sticky with a top threshold
    pub fn sticky(mut self, top: f32) -> Self {
        self.style.position = Position::Relative;
        self.is_sticky = true;
        self.is_fixed = false;
        self.sticky_top = Some(top);
        self
    }

    /// Set inset (position from all edges)
    pub fn inset(mut self, px: f32) -> Self {
        let val = LengthPercentageAuto::Length(px);
        self.style.inset = Rect {
            left: val,
            right: val,
            top: val,
            bottom: val,
        };
        self
    }

    /// Set top position
    pub fn top(mut self, px: f32) -> Self {
        self.style.inset.top = LengthPercentageAuto::Length(px);
        self
    }

    /// Set bottom position
    pub fn bottom(mut self, px: f32) -> Self {
        self.style.inset.bottom = LengthPercentageAuto::Length(px);
        self
    }

    /// Set left position
    pub fn left(mut self, px: f32) -> Self {
        self.style.inset.left = LengthPercentageAuto::Length(px);
        self
    }

    /// Set right position
    pub fn right(mut self, px: f32) -> Self {
        self.style.inset.right = LengthPercentageAuto::Length(px);
        self
    }

    // =========================================================================
    // Overflow
    // =========================================================================

    /// Set overflow to hidden (clip content)
    ///
    /// Content that extends beyond the element's bounds will be clipped.
    /// This is essential for scroll containers.
    pub fn overflow_clip(mut self) -> Self {
        self.style.overflow.x = Overflow::Clip;
        self.style.overflow.y = Overflow::Clip;
        self
    }

    /// Set the CSS `clip-path`. Restricts the element's painted
    /// pixels to the supplied shape (polygon, circle, ellipse,
    /// inset, rect, xywh, or SVG path).
    pub fn clip_path(mut self, path: ClipPath) -> Self {
        self.clip_path = Some(path);
        self
    }

    /// Set overflow to visible (default, content can extend beyond bounds)
    pub fn overflow_visible(mut self) -> Self {
        self.style.overflow.x = Overflow::Visible;
        self.style.overflow.y = Overflow::Visible;
        self
    }

    /// Set overflow to scroll (enable scrolling with full physics)
    ///
    /// Creates a scroll container with momentum scrolling and a scrollbar.
    /// Bounce is disabled by default; use an explicit scroll config/widget
    /// when rubber-band edges are desired.
    /// This is equivalent to using the `scroll()` widget but configured via the builder API.
    pub fn overflow_scroll(mut self) -> Self {
        self.style.overflow.x = Overflow::Scroll;
        self.style.overflow.y = Overflow::Scroll;
        self.ensure_scroll_physics(crate::scroll::ScrollDirection::Both);
        self
    }

    /// Set overflow to scroll (taffy style only, no physics)
    ///
    /// Used internally by the `Scroll` widget which manages its own physics.
    pub(crate) fn overflow_scroll_style_only(mut self) -> Self {
        self.style.overflow.x = Overflow::Scroll;
        self.style.overflow.y = Overflow::Scroll;
        self
    }

    /// Set horizontal overflow only (X-axis)
    pub fn overflow_x(mut self, overflow: Overflow) -> Self {
        self.style.overflow.x = overflow;
        if overflow == Overflow::Scroll {
            self.ensure_scroll_physics(crate::scroll::ScrollDirection::Horizontal);
        }
        self
    }

    /// Set vertical overflow only (Y-axis)
    pub fn overflow_y(mut self, overflow: Overflow) -> Self {
        self.style.overflow.y = overflow;
        if overflow == Overflow::Scroll {
            self.ensure_scroll_physics(crate::scroll::ScrollDirection::Vertical);
        }
        self
    }

    /// Set overflow to scroll on X-axis only
    pub fn overflow_x_scroll(mut self) -> Self {
        self.style.overflow.x = Overflow::Scroll;
        self.style.overflow.y = Overflow::Clip;
        self.ensure_scroll_physics(crate::scroll::ScrollDirection::Horizontal);
        self
    }

    /// Set overflow to scroll on Y-axis only
    pub fn overflow_y_scroll(mut self) -> Self {
        self.style.overflow.x = Overflow::Clip;
        self.style.overflow.y = Overflow::Scroll;
        self.ensure_scroll_physics(crate::scroll::ScrollDirection::Vertical);
        self
    }

    /// Ensure scroll physics are created for this div
    fn ensure_scroll_physics(&mut self, direction: crate::scroll::ScrollDirection) {
        if self.scroll_physics.is_none() {
            let config = crate::scroll::ScrollConfig {
                direction,
                ..Default::default()
            };
            let physics = std::sync::Arc::new(std::sync::Mutex::new(
                crate::scroll::ScrollPhysics::new(config),
            ));
            let handlers =
                crate::scroll::Scroll::create_internal_handlers(std::sync::Arc::clone(&physics));
            // Merge scroll handlers into existing event handlers
            self.event_handlers.merge(handlers);
            self.scroll_physics = Some(physics);
        }
    }

    // =========================================================================
    // Visual Properties
    // =========================================================================

    /// Set background colour.
    ///
    /// Accepts either an eager `Color` or a `&State<Color>` /
    /// `State<Color>` for signal-bound reactivity. When passed a
    /// signal, changes to the state fire a property-channel patch via
    /// the global binding registry — no `Stateful` wrap, no closure
    /// re-run, no subtree rebuild.
    /// ([[project-reactive-architecture-v2]] Phase 2.)
    ///
    /// ```ignore
    /// // Eager (compiles unchanged from pre-Phase-2 code)
    /// div().bg(Color::RED)
    ///
    /// // Signal-bound — `.set()` triggers a direct field patch
    /// let bg = State::new(...);
    /// div().bg(&bg)
    /// ```
    pub fn bg(mut self, value: impl crate::binding::IntoReactive<Color>) -> Self {
        use crate::binding::{Reactive, TypedPendingBinding};
        match value.into_reactive() {
            Reactive::Const(color) => {
                self.background = Some(Brush::Solid(color));
            }
            Reactive::Bound(state) => {
                // Seed the initial value so the first paint matches.
                if let Some(c) = state.try_get() {
                    self.background = Some(Brush::Solid(c));
                }
                self.pending_bindings
                    .push(Box::new(TypedPendingBinding::new(
                        state,
                        crate::property::PropertyId::Background,
                        |props, color: Color| {
                            props.background = Some(Brush::Solid(color));
                        },
                    )));
            }
            Reactive::Computed(computed) => {
                if let Some(c) = computed.try_get() {
                    self.background = Some(Brush::Solid(c));
                }
                self.pending_bindings
                    .push(Box::new(TypedPendingBinding::from_computed(
                        computed,
                        crate::property::PropertyId::Background,
                        |props, color: Color| {
                            props.background = Some(Brush::Solid(color));
                        },
                    )));
            }
        }
        self
    }

    /// Set background brush (for gradients)
    pub fn background(mut self, brush: impl Into<Brush>) -> Self {
        self.background = Some(brush.into());
        self
    }

    // -------------------------------------------------------------------------
    // Backdrop Blur (CSS backdrop-filter: blur())
    // -------------------------------------------------------------------------

    /// Apply backdrop blur effect to content behind this element
    ///
    /// This blurs whatever is rendered behind this element, similar to
    /// CSS `backdrop-filter: blur()`. For a full frosted glass effect with
    /// tinting and noise, use `.glass()` instead.
    ///
    /// # Example
    ///
    /// ```ignore
    /// div()
    ///     .w(200.0).h(100.0)
    ///     .backdrop_blur(10.0) // 10px blur radius
    ///     .bg(Color::rgba(255, 255, 255, 128)) // Semi-transparent overlay
    /// ```
    pub fn backdrop_blur(self, radius: f32) -> Self {
        self.background(Brush::Blur(BlurStyle::with_radius(radius)))
    }

    /// Apply backdrop blur with full style control
    ///
    /// # Example
    ///
    /// ```ignore
    /// div()
    ///     .backdrop_blur_style(
    ///         BlurStyle::new()
    ///             .radius(20.0)
    ///             .quality(BlurQuality::High)
    ///             .tint(Color::rgba(255, 255, 255, 50))
    ///     )
    /// ```
    pub fn backdrop_blur_style(self, style: BlurStyle) -> Self {
        self.background(Brush::Blur(style))
    }

    /// Light backdrop blur (5px, subtle)
    pub fn backdrop_blur_light(self) -> Self {
        self.background(Brush::Blur(BlurStyle::light()))
    }

    /// Medium backdrop blur (10px, default)
    pub fn backdrop_blur_medium(self) -> Self {
        self.background(Brush::Blur(BlurStyle::medium()))
    }

    /// Heavy backdrop blur (20px, strong)
    pub fn backdrop_blur_heavy(self) -> Self {
        self.background(Brush::Blur(BlurStyle::heavy()))
    }

    /// Maximum backdrop blur (50px)
    pub fn backdrop_blur_max(self) -> Self {
        self.background(Brush::Blur(BlurStyle::max()))
    }

    // -------------------------------------------------------------------------
    // Theme-based background colors
    // -------------------------------------------------------------------------

    /// Set background to theme primary color
    pub fn bg_primary(self) -> Self {
        self.bg(ThemeState::get().color(blinc_theme::ColorToken::Primary))
    }

    /// Set background to theme secondary color
    pub fn bg_secondary(self) -> Self {
        self.bg(ThemeState::get().color(blinc_theme::ColorToken::Secondary))
    }

    /// Set background to theme background color
    pub fn bg_background(self) -> Self {
        self.bg(ThemeState::get().color(blinc_theme::ColorToken::Background))
    }

    /// Set background to theme surface color
    pub fn bg_surface(self) -> Self {
        self.bg(ThemeState::get().color(blinc_theme::ColorToken::Surface))
    }

    /// Set background to theme elevated surface color
    pub fn bg_surface_elevated(self) -> Self {
        self.bg(ThemeState::get().color(blinc_theme::ColorToken::SurfaceElevated))
    }

    /// Set background to theme success color
    pub fn bg_success(self) -> Self {
        self.bg(ThemeState::get().color(blinc_theme::ColorToken::SuccessBg))
    }

    /// Set background to theme warning color
    pub fn bg_warning(self) -> Self {
        self.bg(ThemeState::get().color(blinc_theme::ColorToken::WarningBg))
    }

    /// Set background to theme error color
    pub fn bg_error(self) -> Self {
        self.bg(ThemeState::get().color(blinc_theme::ColorToken::ErrorBg))
    }

    /// Set background to theme info color
    pub fn bg_info(self) -> Self {
        self.bg(ThemeState::get().color(blinc_theme::ColorToken::InfoBg))
    }

    /// Set background to theme accent color
    pub fn bg_accent(self) -> Self {
        self.bg(ThemeState::get().color(blinc_theme::ColorToken::Accent))
    }

    // -------------------------------------------------------------------------
    // Corner Radius
    // -------------------------------------------------------------------------

    /// Set corner radius (all corners).
    ///
    /// Accepts either an eager `f32` or a `&State<f32>` / `State<f32>` for
    /// signal-bound reactivity. Signal updates patch the corner radius
    /// directly via the property channel — no `Stateful` wrap.
    /// ([[project-reactive-architecture-v2]] Phase 2.)
    pub fn rounded(mut self, value: impl crate::binding::IntoReactive<f32>) -> Self {
        use crate::binding::{Reactive, TypedPendingBinding};
        match value.into_reactive() {
            Reactive::Const(radius) => {
                self.border_radius = CornerRadius::uniform(radius);
                self.border_radius_explicit = true;
            }
            Reactive::Bound(state) => {
                if let Some(r) = state.try_get() {
                    self.border_radius = CornerRadius::uniform(r);
                    self.border_radius_explicit = true;
                }
                self.pending_bindings
                    .push(Box::new(TypedPendingBinding::new(
                        state,
                        crate::property::PropertyId::CornerRadius,
                        |props, r: f32| {
                            props.border_radius = CornerRadius::uniform(r);
                            props.border_radius_explicit = true;
                        },
                    )));
            }
            Reactive::Computed(computed) => {
                if let Some(r) = computed.try_get() {
                    self.border_radius = CornerRadius::uniform(r);
                    self.border_radius_explicit = true;
                }
                self.pending_bindings
                    .push(Box::new(TypedPendingBinding::from_computed(
                        computed,
                        crate::property::PropertyId::CornerRadius,
                        |props, r: f32| {
                            props.border_radius = CornerRadius::uniform(r);
                            props.border_radius_explicit = true;
                        },
                    )));
            }
        }
        self
    }

    /// Set corner radius with full pill shape (radius = min(w,h)/2)
    pub fn rounded_full(mut self) -> Self {
        // Use a large value; actual pill shape depends on element size
        self.border_radius = CornerRadius::uniform(9999.0);
        self.border_radius_explicit = true;
        self
    }

    /// Set individual corner radii
    pub fn rounded_corners(mut self, tl: f32, tr: f32, br: f32, bl: f32) -> Self {
        self.border_radius = CornerRadius::new(tl, tr, br, bl);
        self.border_radius_explicit = true;
        self
    }

    // -------------------------------------------------------------------------
    // Theme-based corner radii
    // -------------------------------------------------------------------------

    /// Set corner radius to theme's small radius
    pub fn rounded_sm(self) -> Self {
        self.rounded(ThemeState::get().radii().radius_sm)
    }

    /// Set corner radius to theme's default radius
    pub fn rounded_default(self) -> Self {
        self.rounded(ThemeState::get().radii().radius_default)
    }

    /// Set corner radius to theme's medium radius
    pub fn rounded_md(self) -> Self {
        self.rounded(ThemeState::get().radii().radius_md)
    }

    /// Set corner radius to theme's large radius
    pub fn rounded_lg(self) -> Self {
        self.rounded(ThemeState::get().radii().radius_lg)
    }

    /// Set corner radius to theme's extra large radius
    pub fn rounded_xl(self) -> Self {
        self.rounded(ThemeState::get().radii().radius_xl)
    }

    /// Set corner radius to theme's 2xl radius
    pub fn rounded_2xl(self) -> Self {
        self.rounded(ThemeState::get().radii().radius_2xl)
    }

    /// Set corner radius to theme's 3xl radius
    pub fn rounded_3xl(self) -> Self {
        self.rounded(ThemeState::get().radii().radius_3xl)
    }

    /// Set corner radius to none (0)
    pub fn rounded_none(self) -> Self {
        self.rounded(0.0)
    }

    // =========================================================================
    // Corner Shape (superellipse)
    // =========================================================================

    /// Set uniform corner shape (superellipse exponent)
    ///
    /// Values: 0.0 = bevel, 1.0 = round (default), 2.0 = squircle, -1.0 = scoop
    pub fn corner_shape(mut self, n: f32) -> Self {
        self.corner_shape = CornerShape::uniform(n);
        self
    }

    /// Set per-corner shape values (top-left, top-right, bottom-right, bottom-left)
    pub fn corner_shapes(mut self, tl: f32, tr: f32, br: f32, bl: f32) -> Self {
        self.corner_shape = CornerShape::new(tl, tr, br, bl);
        self
    }

    /// Set corners to bevel (diamond/L1 norm)
    pub fn corner_bevel(self) -> Self {
        self.corner_shape(0.0)
    }

    /// Set corners to squircle (L4 superellipse)
    pub fn corner_squircle(self) -> Self {
        self.corner_shape(2.0)
    }

    /// Set corners to scoop (concave)
    pub fn corner_scoop(self) -> Self {
        self.corner_shape(-1.0)
    }

    /// Opt out of theme-driven squircle substitution.
    ///
    /// When the active theme advertises a non-default
    /// [`ShapeTokens`](blinc_theme::ShapeTokens) (e.g. any of the
    /// Universal HID variants), the paint walker substitutes the
    /// theme's effective `n` onto rounded corners that pass the
    /// threshold check. Calling `lock_corner_shape()` tells the
    /// walker to leave this element's corners alone — useful for
    /// floating overlay widgets (popovers, dropdown panels, select
    /// menus) whose chrome should remain circular regardless of the
    /// theme.
    ///
    /// Has no effect on themes that already use circular corners
    /// (every existing platform bundle + Catppuccin BlincTheme) —
    /// they short-circuit the substitution before this flag is
    /// consulted.
    pub fn lock_corner_shape(mut self) -> Self {
        self.corner_shape_locked = true;
        self
    }

    // =========================================================================
    // Overflow Fade
    // =========================================================================

    /// Set uniform overflow fade distance on all edges (in CSS pixels)
    pub fn overflow_fade(mut self, distance: f32) -> Self {
        self.overflow_fade = OverflowFade::uniform(distance);
        self
    }

    /// Set horizontal overflow fade (left and right edges)
    pub fn overflow_fade_x(mut self, distance: f32) -> Self {
        self.overflow_fade = OverflowFade::horizontal(distance);
        self
    }

    /// Set vertical overflow fade (top and bottom edges)
    pub fn overflow_fade_y(mut self, distance: f32) -> Self {
        self.overflow_fade = OverflowFade::vertical(distance);
        self
    }

    /// Set per-edge overflow fade distances (top, right, bottom, left)
    pub fn overflow_fade_edges(mut self, top: f32, right: f32, bottom: f32, left: f32) -> Self {
        self.overflow_fade = OverflowFade::new(top, right, bottom, left);
        self
    }

    // =========================================================================
    // Border
    // =========================================================================

    /// Set border with color and width
    pub fn border(mut self, width: f32, color: Color) -> Self {
        self.border_width = width;
        self.border_color = Some(color);
        self
    }

    /// Set border colour only.
    ///
    /// Accepts either an eager `Color` or a `&State<Color>` /
    /// `State<Color>` for signal-bound reactivity. Same pattern as
    /// [`Self::bg`].
    pub fn border_color(mut self, value: impl crate::binding::IntoReactive<Color>) -> Self {
        use crate::binding::{Reactive, TypedPendingBinding};
        match value.into_reactive() {
            Reactive::Const(color) => {
                self.border_color = Some(color);
            }
            Reactive::Bound(state) => {
                if let Some(c) = state.try_get() {
                    self.border_color = Some(c);
                }
                self.pending_bindings
                    .push(Box::new(TypedPendingBinding::new(
                        state,
                        crate::property::PropertyId::BorderColor,
                        |props, c: Color| {
                            props.border_color = Some(c);
                        },
                    )));
            }
            Reactive::Computed(computed) => {
                if let Some(c) = computed.try_get() {
                    self.border_color = Some(c);
                }
                self.pending_bindings
                    .push(Box::new(TypedPendingBinding::from_computed(
                        computed,
                        crate::property::PropertyId::BorderColor,
                        |props, c: Color| {
                            props.border_color = Some(c);
                        },
                    )));
            }
        }
        self
    }

    /// Set border width only
    pub fn border_width(mut self, width: f32) -> Self {
        self.border_width = width;
        self
    }

    /// Set outline width in pixels
    pub fn outline_width(mut self, width: f32) -> Self {
        self.outline_width = width;
        self
    }

    /// Set outline color
    pub fn outline_color(mut self, color: Color) -> Self {
        self.outline_color = Some(color);
        self
    }

    /// Set outline offset in pixels (gap between border and outline)
    pub fn outline_offset(mut self, offset: f32) -> Self {
        self.outline_offset = offset;
        self
    }

    /// Set left border only (useful for blockquotes)
    ///
    /// If a uniform border was previously set, other sides will inherit from it.
    ///
    /// # Example
    /// ```ignore
    /// div().border_left(4.0, Color::BLUE).pl(16.0)
    /// ```
    pub fn border_left(mut self, width: f32, color: Color) -> Self {
        self.inherit_uniform_border_to_sides();
        self.border_sides.left = Some(crate::element::BorderSide::new(width, color));
        self
    }

    /// Set right border only
    ///
    /// If a uniform border was previously set, other sides will inherit from it.
    pub fn border_right(mut self, width: f32, color: Color) -> Self {
        self.inherit_uniform_border_to_sides();
        self.border_sides.right = Some(crate::element::BorderSide::new(width, color));
        self
    }

    /// Set top border only
    ///
    /// If a uniform border was previously set, other sides will inherit from it.
    pub fn border_top(mut self, width: f32, color: Color) -> Self {
        self.inherit_uniform_border_to_sides();
        self.border_sides.top = Some(crate::element::BorderSide::new(width, color));
        self
    }

    /// Set bottom border only
    ///
    /// If a uniform border was previously set, other sides will inherit from it.
    pub fn border_bottom(mut self, width: f32, color: Color) -> Self {
        self.inherit_uniform_border_to_sides();
        self.border_sides.bottom = Some(crate::element::BorderSide::new(width, color));
        self
    }

    /// Set horizontal borders (left and right)
    ///
    /// If a uniform border was previously set, vertical sides will inherit from it.
    pub fn border_x(mut self, width: f32, color: Color) -> Self {
        self.inherit_uniform_border_to_sides();
        let side = crate::element::BorderSide::new(width, color);
        self.border_sides.left = Some(side);
        self.border_sides.right = Some(side);
        self
    }

    /// Set vertical borders (top and bottom)
    ///
    /// If a uniform border was previously set, horizontal sides will inherit from it.
    pub fn border_y(mut self, width: f32, color: Color) -> Self {
        self.inherit_uniform_border_to_sides();
        let side = crate::element::BorderSide::new(width, color);
        self.border_sides.top = Some(side);
        self.border_sides.bottom = Some(side);
        self
    }

    /// Helper to inherit uniform border to per-side borders before modifying a specific side
    ///
    /// This ensures that when you do `.border(1.0, color).border_bottom(3.0, color)`,
    /// the other three sides get the uniform border instead of being empty.
    fn inherit_uniform_border_to_sides(&mut self) {
        // Only inherit if we have a uniform border and no per-side borders yet
        if self.border_width > 0.0 && !self.border_sides.has_any() {
            if let Some(color) = self.border_color {
                let side = crate::element::BorderSide::new(self.border_width, color);
                self.border_sides.top = Some(side);
                self.border_sides.right = Some(side);
                self.border_sides.bottom = Some(side);
                self.border_sides.left = Some(side);
            }
        }
    }

    /// Configure borders using a builder pattern
    ///
    /// This provides a fluent API for setting multiple border sides at once
    /// without accidentally overriding previously set sides.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Set left and bottom borders
    /// div().borders(|b| b.left(4.0, Color::BLUE).bottom(1.0, Color::gray(0.3)))
    ///
    /// // Set all borders, then override one
    /// div().borders(|b| b.all(1.0, Color::gray(0.2)).left(4.0, Color::BLUE))
    ///
    /// // Set horizontal borders only
    /// div().borders(|b| b.x(1.0, Color::RED))
    /// ```
    pub fn borders<F>(mut self, f: F) -> Self
    where
        F: FnOnce(crate::element::BorderBuilder) -> crate::element::BorderBuilder,
    {
        let builder = f(crate::element::BorderBuilder::new());
        self.border_sides = builder.build();
        self
    }

    // =========================================================================
    // Layer (for rendering order)
    // =========================================================================

    /// Set the render layer
    pub fn layer(mut self, layer: RenderLayer) -> Self {
        self.render_layer = layer;
        self
    }

    /// Render in foreground (on top of glass)
    pub fn foreground(self) -> Self {
        self.layer(RenderLayer::Foreground)
    }

    // =========================================================================
    // Material System
    // =========================================================================

    /// Apply a material to this element
    pub fn material(mut self, material: Material) -> Self {
        // Glass materials also set the render layer to Glass
        if matches!(material, Material::Glass(_)) {
            self.render_layer = RenderLayer::Glass;
        }
        self.material = Some(material);
        self
    }

    /// Apply a visual effect to this element
    ///
    /// Effects include glass (blur), metallic (reflection), wood (texture), etc.
    /// This is the general-purpose method for applying any material effect.
    ///
    /// Example:
    /// ```ignore
    /// // Glass effect
    /// div().effect(GlassMaterial::thick().tint_rgba(1.0, 0.9, 0.9, 0.5))
    ///
    /// // Metallic effect
    /// div().effect(MetallicMaterial::chrome())
    /// ```
    pub fn effect(self, effect: impl Into<Material>) -> Self {
        self.material(effect.into())
    }

    /// Apply glass material with default settings (shorthand for common case)
    ///
    /// Creates a frosted glass effect that blurs content behind the element.
    pub fn glass(self) -> Self {
        self.material(Material::Glass(GlassMaterial::new()))
    }

    /// Apply metallic material with default settings
    pub fn metallic(self) -> Self {
        self.material(Material::Metallic(MetallicMaterial::new()))
    }

    /// Apply chrome metallic preset
    pub fn chrome(self) -> Self {
        self.material(Material::Metallic(MetallicMaterial::chrome()))
    }

    /// Apply gold metallic preset
    pub fn gold(self) -> Self {
        self.material(Material::Metallic(MetallicMaterial::gold()))
    }

    /// Apply wood material with default settings
    pub fn wood(self) -> Self {
        self.material(Material::Wood(WoodMaterial::new()))
    }

    // =========================================================================
    // Shadow
    // =========================================================================

    /// Apply a single drop shadow to this element (replaces any existing stack).
    ///
    /// Accepts either an eager `Shadow` or a `&State<Shadow>` /
    /// `State<Shadow>` for signal-bound reactivity.
    pub fn shadow(mut self, value: impl crate::binding::IntoReactive<Shadow>) -> Self {
        use crate::binding::{Reactive, TypedPendingBinding};
        match value.into_reactive() {
            Reactive::Const(shadow) => {
                self.shadow = vec![shadow];
            }
            Reactive::Bound(state) => {
                if let Some(s) = state.try_get() {
                    self.shadow = vec![s];
                }
                self.pending_bindings
                    .push(Box::new(TypedPendingBinding::new(
                        state,
                        crate::property::PropertyId::Shadow,
                        |props, s: Shadow| {
                            props.shadow = vec![s];
                        },
                    )));
            }
            Reactive::Computed(computed) => {
                if let Some(s) = computed.try_get() {
                    self.shadow = vec![s];
                }
                self.pending_bindings
                    .push(Box::new(TypedPendingBinding::from_computed(
                        computed,
                        crate::property::PropertyId::Shadow,
                        |props, s: Shadow| {
                            props.shadow = vec![s];
                        },
                    )));
            }
        }
        self
    }

    /// Apply a compound drop shadow stack (multiple layered shadows).
    pub fn shadow_stack(mut self, shadows: Vec<Shadow>) -> Self {
        self.shadow = shadows;
        self
    }

    /// Apply a drop shadow with the given parameters
    pub fn shadow_params(self, offset_x: f32, offset_y: f32, blur: f32, color: Color) -> Self {
        self.shadow(Shadow::new(offset_x, offset_y, blur, color))
    }

    /// Apply a small drop shadow using theme colors
    pub fn shadow_sm(self) -> Self {
        self.shadow_stack(theme_shadow_stack(|s| &s.shadow_sm))
    }

    /// Apply a medium drop shadow using theme colors
    pub fn shadow_md(self) -> Self {
        self.shadow_stack(theme_shadow_stack(|s| &s.shadow_md))
    }

    /// Apply a large drop shadow using theme colors
    pub fn shadow_lg(self) -> Self {
        self.shadow_stack(theme_shadow_stack(|s| &s.shadow_lg))
    }

    /// Apply an extra large drop shadow using theme colors
    pub fn shadow_xl(self) -> Self {
        self.shadow_stack(theme_shadow_stack(|s| &s.shadow_xl))
    }

    // =========================================================================
    // Transform
    // =========================================================================

    /// Apply a transform to this element.
    ///
    /// Accepts either an eager `Transform` or a `&State<Transform>` /
    /// `State<Transform>` for signal-bound reactivity. Animated
    /// translate / rotate / scale should use this path — patches the
    /// transform cell per frame via the property channel without
    /// rebuilding the subtree.
    pub fn transform(mut self, value: impl crate::binding::IntoReactive<Transform>) -> Self {
        use crate::binding::{Reactive, TypedPendingBinding};
        match value.into_reactive() {
            Reactive::Const(transform) => {
                self.transform = Some(transform);
            }
            Reactive::Bound(state) => {
                if let Some(t) = state.try_get() {
                    self.transform = Some(t);
                }
                self.pending_bindings
                    .push(Box::new(TypedPendingBinding::new(
                        state,
                        crate::property::PropertyId::Transform,
                        |props, t: Transform| {
                            props.transform = Some(t);
                        },
                    )));
            }
            Reactive::Computed(computed) => {
                if let Some(t) = computed.try_get() {
                    self.transform = Some(t);
                }
                self.pending_bindings
                    .push(Box::new(TypedPendingBinding::from_computed(
                        computed,
                        crate::property::PropertyId::Transform,
                        |props, t: Transform| {
                            props.transform = Some(t);
                        },
                    )));
            }
        }
        self
    }

    /// Set the per-element transform origin in percentages — `(0.0, 0.0)`
    /// is the top-left corner, `(100.0, 100.0)` is the bottom-right,
    /// `(50.0, 50.0)` is the centre (the renderer's default when no
    /// origin is set explicitly).
    ///
    /// Phase 8.1 of the unified property channel
    /// ([[project-reactive-architecture-v2]]) added this builder so
    /// [`Self::transform_width`] can pivot a scale transform
    /// at the left edge (origin `(0.0, 50.0)`); useful independently
    /// for any caller that needs non-centre transforms.
    pub fn transform_origin(mut self, x_percent: f32, y_percent: f32) -> Self {
        self.transform_origin = Some([x_percent, y_percent]);
        self
    }

    /// Bind a transform that's derived from an arbitrary signal type
    /// via a caller-supplied mapper. Phase 8.1 of the unified
    /// property channel ([[project-reactive-architecture-v2]]).
    ///
    /// The general primitive that [`Self::transform_width`]
    /// sugars over. Useful when the source signal's value type isn't
    /// `Transform` but a derivation produces one: e.g. a `State<f32>`
    /// in `0..=100` mapped to `Transform::scale((v/100).clamp(0,1), 1.0)`,
    /// or a `State<Pose>` mapped to a translate + rotate compound.
    ///
    /// Seeds the initial value via `mapper(state.try_get().unwrap_or_default())`
    /// and subscribes to the signal so future writes patch
    /// `props.transform` through the unified property channel —
    /// identical wiring to [`Self::transform`] but with the inline
    /// derivation closure.
    pub fn bind_transform_from<T>(
        mut self,
        source: impl crate::binding::IntoReactive<T>,
        mapper: impl Fn(T) -> Transform + Send + Sync + 'static,
    ) -> Self
    where
        T: Clone + Default + Send + Sync + 'static,
    {
        use crate::binding::{Reactive, TypedPendingBinding};
        let mapper = std::sync::Arc::new(mapper);
        match source.into_reactive() {
            Reactive::Const(v) => {
                self.transform = Some(mapper(v));
            }
            Reactive::Bound(state) => {
                let initial = state.try_get().unwrap_or_default();
                self.transform = Some(mapper(initial));
                self.pending_bindings
                    .push(Box::new(TypedPendingBinding::new(
                        state,
                        crate::property::PropertyId::Transform,
                        move |props, v: T| {
                            props.transform = Some(mapper(v));
                        },
                    )));
            }
            Reactive::Computed(computed) => {
                let initial = computed.try_get().unwrap_or_default();
                self.transform = Some(mapper(initial));
                self.pending_bindings
                    .push(Box::new(TypedPendingBinding::from_computed(
                        computed,
                        crate::property::PropertyId::Transform,
                        move |props, v: T| {
                            props.transform = Some(mapper(v));
                        },
                    )));
            }
        }
        self
    }

    /// Animate the visual width of this element by scaling along the
    /// X axis from the left edge. Phase 8.1 of the unified property
    /// channel ([[project-reactive-architecture-v2]]).
    ///
    /// The element's layout slot stays full-size — only the GPU
    /// scale transform changes per frame as the bound signal moves.
    /// Skipping `compute_layout()` is the win: width-bound animations
    /// previously had to either rebuild the subtree (`Stateful`) or
    /// route through the taffy style path (`.w(&signal)` — Phase 2.4
    /// works but triggers a relayout per signal-set). The scale-x
    /// path is GPU-only.
    ///
    /// `fraction` is a 0.0..=1.0 multiplier on the element's layout
    /// width: `0.0` collapses the element to invisible, `1.0` is the
    /// full width, `0.5` is half. Values outside `[0.0, 1.0]` are
    /// accepted (over-shooting / negative scales are legitimate for
    /// spring overshoot or mirroring).
    ///
    /// Sets the transform origin to `(0.0, 50.0)` so the scale grows
    /// from the left edge instead of the centre. If you've also set
    /// a custom [`Self::transform_origin`], call
    /// `.transform_origin(...)` AFTER this method to keep your value.
    ///
    /// Hit-testing is unaffected: the element still claims its full
    /// layout-width bounding box for pointer events. This matches
    /// the slider/progress-fill use case where the fill is a
    /// non-interactive visual indicator clipped by a parent track.
    pub fn transform_width(mut self, fraction: impl crate::binding::IntoReactive<f32>) -> Self {
        use crate::binding::{Reactive, TypedPendingBinding};
        // Pin the pivot to the left edge so the scale visibly grows /
        // shrinks from the start of the element rather than its centre.
        // Caller can override afterwards via `.transform_origin(...)`.
        self.transform_origin = Some([0.0, 50.0]);
        match fraction.into_reactive() {
            Reactive::Const(f) => {
                self.transform = Some(Transform::scale(f, 1.0));
            }
            Reactive::Bound(state) => {
                if let Some(f) = state.try_get() {
                    self.transform = Some(Transform::scale(f, 1.0));
                }
                self.pending_bindings
                    .push(Box::new(TypedPendingBinding::new(
                        state,
                        crate::property::PropertyId::Transform,
                        |props, f: f32| {
                            props.transform = Some(Transform::scale(f, 1.0));
                        },
                    )));
            }
            Reactive::Computed(computed) => {
                if let Some(f) = computed.try_get() {
                    self.transform = Some(Transform::scale(f, 1.0));
                }
                self.pending_bindings
                    .push(Box::new(TypedPendingBinding::from_computed(
                        computed,
                        crate::property::PropertyId::Transform,
                        |props, f: f32| {
                            props.transform = Some(Transform::scale(f, 1.0));
                        },
                    )));
            }
        }
        self
    }

    /// Translate this element by the given x and y offset.
    ///
    /// Eager-only — multi-axis composition with separate reactive
    /// signals would need a read-modify-write binding shape that
    /// hasn't landed yet. For animated translation, build a
    /// `State<Transform>` and bind via [`Self::transform`].
    pub fn translate(self, x: f32, y: f32) -> Self {
        self.transform(Transform::translate(x, y))
    }

    /// Scale this element uniformly.
    ///
    /// Accepts an eager `f32` or a `&State<f32>` / `State<f32>` for
    /// signal-bound reactivity — signal updates patch
    /// `props.transform` via the unified property channel per set.
    pub fn scale(self, factor: impl crate::binding::IntoReactive<f32>) -> Self {
        use crate::binding::Reactive;
        match factor.into_reactive() {
            Reactive::Const(f) => self.transform(Transform::scale(f, f)),
            Reactive::Bound(state) => {
                self.bind_transform_from(state, |f: f32| Transform::scale(f, f))
            }
            Reactive::Computed(computed) => {
                self.bind_transform_from(computed, |f: f32| Transform::scale(f, f))
            }
        }
    }

    /// Scale this element with different x and y factors.
    ///
    /// Eager-only — see [`Self::translate`] for the multi-axis
    /// composition rationale.
    pub fn scale_xy(self, sx: f32, sy: f32) -> Self {
        self.transform(Transform::scale(sx, sy))
    }

    /// Rotate this element by the given angle in radians.
    ///
    /// Accepts an eager `f32` or a `&State<f32>` / `State<f32>` for
    /// signal-bound reactivity.
    pub fn rotate(self, angle: impl crate::binding::IntoReactive<f32>) -> Self {
        use crate::binding::Reactive;
        match angle.into_reactive() {
            Reactive::Const(a) => self.transform(Transform::rotate(a)),
            Reactive::Bound(state) => {
                self.bind_transform_from(state, |a: f32| Transform::rotate(a))
            }
            Reactive::Computed(computed) => {
                self.bind_transform_from(computed, |a: f32| Transform::rotate(a))
            }
        }
    }

    /// Rotate this element by the given angle in degrees.
    ///
    /// Accepts an eager `f32` or a `&State<f32>` / `State<f32>` for
    /// signal-bound reactivity (the value is multiplied by π/180 on
    /// every fire to produce the radians the underlying transform
    /// uses).
    pub fn rotate_deg(self, degrees: impl crate::binding::IntoReactive<f32>) -> Self {
        use crate::binding::Reactive;
        const DEG_TO_RAD: f32 = std::f32::consts::PI / 180.0;
        match degrees.into_reactive() {
            Reactive::Const(d) => self.transform(Transform::rotate(d * DEG_TO_RAD)),
            Reactive::Bound(state) => {
                self.bind_transform_from(state, |d: f32| Transform::rotate(d * DEG_TO_RAD))
            }
            Reactive::Computed(computed) => {
                self.bind_transform_from(computed, |d: f32| Transform::rotate(d * DEG_TO_RAD))
            }
        }
    }

    // =========================================================================
    // Opacity
    // =========================================================================

    /// Set opacity (0.0 = transparent, 1.0 = opaque).
    ///
    /// Accepts either an eager `f32` or a `&State<f32>` / `State<f32>`
    /// for signal-bound reactivity. Same mechanism as [`Self::bg`].
    pub fn opacity(mut self, value: impl crate::binding::IntoReactive<f32>) -> Self {
        use crate::binding::{Reactive, TypedPendingBinding};
        match value.into_reactive() {
            Reactive::Const(o) => {
                self.opacity = o.clamp(0.0, 1.0);
            }
            Reactive::Bound(state) => {
                if let Some(o) = state.try_get() {
                    self.opacity = o.clamp(0.0, 1.0);
                }
                self.pending_bindings
                    .push(Box::new(TypedPendingBinding::new(
                        state,
                        crate::property::PropertyId::Opacity,
                        |props, o: f32| {
                            props.opacity = o.clamp(0.0, 1.0);
                        },
                    )));
            }
            Reactive::Computed(computed) => {
                if let Some(o) = computed.try_get() {
                    self.opacity = o.clamp(0.0, 1.0);
                }
                self.pending_bindings
                    .push(Box::new(TypedPendingBinding::from_computed(
                        computed,
                        crate::property::PropertyId::Opacity,
                        |props, o: f32| {
                            props.opacity = o.clamp(0.0, 1.0);
                        },
                    )));
            }
        }
        self
    }

    /// Fully opaque (opacity = 1.0)
    pub fn opaque(self) -> Self {
        self.opacity(1.0)
    }

    /// Set a @flow shader on this element.
    ///
    /// Accepts either a `FlowGraph` (from the `flow!` macro) or a `&str` name
    /// referencing a flow defined in a CSS stylesheet.
    ///
    /// ```rust,ignore
    /// // Direct flow graph
    /// div().flow(flow!(ripple, fragment, { ... }))
    ///
    /// // Name reference to CSS-defined flow
    /// div().flow("ripple")
    /// ```
    pub fn flow(mut self, f: impl Into<crate::element::FlowRef>) -> Self {
        match f.into() {
            crate::element::FlowRef::Name(name) => {
                self.flow_name = Some(name);
            }
            crate::element::FlowRef::Graph(g) => {
                self.flow_name = Some(g.name.clone());
                self.flow_graph = Some(g);
            }
        }
        self
    }

    /// Semi-transparent (opacity = 0.5)
    pub fn translucent(self) -> Self {
        self.opacity(0.5)
    }

    /// Invisible (opacity = 0.0)
    pub fn invisible(self) -> Self {
        self.opacity(0.0)
    }

    // =========================================================================
    // Layer Effects (Post-Processing)
    // =========================================================================

    /// Add a layer effect to this element
    ///
    /// Layer effects are applied during layer composition and include:
    /// - Blur (Gaussian blur)
    /// - DropShadow (offset shadow behind element)
    /// - Glow (outer glow)
    /// - ColorMatrix (color transformation)
    ///
    /// # Example
    ///
    /// ```ignore
    /// div()
    ///     .w(100.0).h(100.0)
    ///     .bg(Color::BLUE)
    ///     .layer_effect(LayerEffect::blur(8.0))
    /// ```
    pub fn layer_effect(mut self, effect: LayerEffect) -> Self {
        self.layer_effects.push(effect);
        self
    }

    /// Add a blur effect
    ///
    /// # Example
    ///
    /// ```ignore
    /// div()
    ///     .w(100.0).h(100.0)
    ///     .bg(Color::BLUE)
    ///     .blur(8.0) // 8px blur radius
    /// ```
    pub fn blur(self, radius: f32) -> Self {
        self.layer_effect(LayerEffect::blur(radius))
    }

    /// Add a blur effect with specific quality
    ///
    /// Quality options:
    /// - `BlurQuality::Low` - Single-pass box blur (fastest)
    /// - `BlurQuality::Medium` - Two-pass separable Gaussian (balanced)
    /// - `BlurQuality::High` - Multi-pass Kawase blur (highest quality)
    pub fn blur_with_quality(self, radius: f32, quality: BlurQuality) -> Self {
        self.layer_effect(LayerEffect::blur_with_quality(radius, quality))
    }

    /// Add a drop shadow effect
    ///
    /// This is a layer-level shadow effect (post-processing), different from
    /// the element shadow set via `.shadow()` which is rendered inline.
    ///
    /// # Example
    ///
    /// ```ignore
    /// div()
    ///     .w(100.0).h(100.0)
    ///     .bg(Color::WHITE)
    ///     .drop_shadow_effect(4.0, 4.0, 8.0, Color::rgba(0, 0, 0, 128))
    /// ```
    pub fn drop_shadow_effect(self, offset_x: f32, offset_y: f32, blur: f32, color: Color) -> Self {
        self.layer_effect(LayerEffect::drop_shadow(offset_x, offset_y, blur, color))
    }

    /// Add a glow effect
    ///
    /// Creates an outer glow around the element.
    ///
    /// # Example
    ///
    /// ```ignore
    /// div()
    ///     .w(100.0).h(100.0)
    ///     .bg(Color::BLUE)
    ///     .glow_effect(Color::CYAN, 8.0, 4.0, 0.8)
    /// ```
    ///
    /// ## Parameters
    /// - `color`: Glow color
    /// - `blur`: Blur softness (higher = softer edges), typically 4-24
    /// - `range`: How far the glow extends from the element, typically 0-20
    /// - `opacity`: Glow visibility (0.0 to 1.0)
    pub fn glow_effect(self, color: Color, blur: f32, range: f32, opacity: f32) -> Self {
        self.layer_effect(LayerEffect::glow(color, blur, range, opacity))
    }

    /// Apply grayscale filter
    pub fn grayscale(self) -> Self {
        self.layer_effect(LayerEffect::grayscale())
    }

    /// Apply sepia filter
    pub fn sepia(self) -> Self {
        self.layer_effect(LayerEffect::sepia())
    }

    /// Adjust brightness (1.0 = normal, <1.0 = darker, >1.0 = brighter)
    pub fn brightness(self, factor: f32) -> Self {
        self.layer_effect(LayerEffect::brightness(factor))
    }

    /// Adjust contrast (1.0 = normal, <1.0 = less contrast, >1.0 = more contrast)
    pub fn contrast(self, factor: f32) -> Self {
        self.layer_effect(LayerEffect::contrast(factor))
    }

    /// Adjust saturation (0.0 = grayscale, 1.0 = normal, >1.0 = oversaturated)
    pub fn saturation(self, factor: f32) -> Self {
        self.layer_effect(LayerEffect::saturation(factor))
    }

    // =========================================================================
    // Cursor Style
    // =========================================================================

    /// Set the cursor style when hovering over this element
    ///
    /// # Example
    ///
    /// ```ignore
    /// div()
    ///     .w(100.0).h(50.0)
    ///     .bg(Color::BLUE)
    ///     .cursor(CursorStyle::Pointer) // Hand cursor for clickable elements
    ///     .on_click(|_| println!("Clicked!"))
    /// ```
    pub fn cursor(mut self, cursor: crate::element::CursorStyle) -> Self {
        self.cursor = Some(cursor);
        self
    }

    /// Set cursor to pointer (hand) - convenience for clickable elements
    pub fn cursor_pointer(self) -> Self {
        self.cursor(crate::element::CursorStyle::Pointer)
    }

    /// Set cursor to text (I-beam) - for text input areas
    pub fn cursor_text(self) -> Self {
        self.cursor(crate::element::CursorStyle::Text)
    }

    /// Set cursor to move - for draggable elements
    pub fn cursor_move(self) -> Self {
        self.cursor(crate::element::CursorStyle::Move)
    }

    /// Set cursor to grab (open hand) - for grabbable elements
    pub fn cursor_grab(self) -> Self {
        self.cursor(crate::element::CursorStyle::Grab)
    }

    /// Set cursor to grabbing (closed hand) - while dragging
    pub fn cursor_grabbing(self) -> Self {
        self.cursor(crate::element::CursorStyle::Grabbing)
    }

    /// Set cursor to not-allowed - for disabled elements
    pub fn cursor_not_allowed(self) -> Self {
        self.cursor(crate::element::CursorStyle::NotAllowed)
    }

    /// Make this element transparent to hit-testing (pointer-events: none)
    ///
    /// When set, this element will not capture mouse clicks or hover events.
    /// Only its children can receive events. Useful for wrapper elements that
    /// should not block interaction with elements behind them.
    pub fn pointer_events_none(mut self) -> Self {
        self.pointer_events_none = true;
        self
    }

    /// Mark this element as a stack layer for z-ordering
    ///
    /// When set, entering this element increments the z_layer counter,
    /// ensuring its content renders above previous siblings in the
    /// interleaved primitive/text rendering. This is used internally
    /// by Stack and can be used for overlay containers that need to
    /// render above other content.
    pub fn stack_layer(mut self) -> Self {
        self.is_stack_layer = true;
        self
    }

    /// Mark this node as the root of an overlay panel — used by
    /// `OverlayStack` / `ToastTray` so their content routes through the
    /// dynamic batch (and lands in `composite_frame`'s overlay pass after
    /// the static cache + static-SVG dispatch). The generic `Stack`
    /// widget should NOT call this — its `is_stack_layer` is purely for
    /// z-counter bumping within the static cache.
    pub fn overlay_root(mut self) -> Self {
        self.is_overlay_root = true;
        self
    }

    /// Set CSS z-index for stacking order (higher values render on top)
    pub fn z_index(mut self, z: i32) -> Self {
        self.z_index = z;
        self
    }

    // =========================================================================
    // Children
    // =========================================================================

    /// Add a child element
    pub fn child(mut self, child: impl ElementBuilder + 'static) -> Self {
        self.children.push(Box::new(child));
        self
    }

    /// Add a boxed child element (for dynamic element types)
    pub fn child_box(mut self, child: Box<dyn ElementBuilder>) -> Self {
        self.children.push(child);
        self
    }

    /// Add multiple children
    pub fn children<I>(mut self, children: I) -> Self
    where
        I: IntoIterator,
        I::Item: ElementBuilder + 'static,
    {
        for child in children {
            self.children.push(Box::new(child));
        }
        self
    }

    /// Get direct access to the taffy style for advanced configuration
    pub fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }

    // =========================================================================
    // Event Handlers
    // =========================================================================

    /// Get a reference to the event handlers
    pub fn event_handlers(&self) -> &crate::event_handler::EventHandlers {
        &self.event_handlers
    }

    /// Get a mutable reference to the event handlers
    pub fn event_handlers_mut(&mut self) -> &mut crate::event_handler::EventHandlers {
        &mut self.event_handlers
    }

    /// Register a click handler (fired on POINTER_UP)
    ///
    /// # Example
    ///
    /// ```ignore
    /// div()
    ///     .w(100.0).h(50.0)
    ///     .bg(Color::BLUE)
    ///     .on_click(|ctx| {
    ///         println!("Clicked at ({}, {})", ctx.local_x, ctx.local_y);
    ///     })
    /// ```
    pub fn on_click<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + 'static,
    {
        self.event_handlers.on_click(handler);
        self
    }

    /// Register a mouse down handler
    pub fn on_mouse_down<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + 'static,
    {
        self.event_handlers.on_mouse_down(handler);
        self
    }

    /// Register a mouse up handler
    pub fn on_mouse_up<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + 'static,
    {
        self.event_handlers.on_mouse_up(handler);
        self
    }

    /// Register a right-click handler (mouse button 2)
    ///
    /// The handler fires on POINTER_DOWN with the right mouse button.
    /// Use this for context menus, secondary actions, etc.
    pub fn on_right_click<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + 'static,
    {
        self.event_handlers.on_mouse_down(move |ctx| {
            if ctx.mouse_button == 2 {
                handler(ctx);
            }
        });
        self
    }

    /// Alias for `on_right_click` — register a context menu handler
    pub fn on_context_menu<F>(self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + 'static,
    {
        self.on_right_click(handler)
    }

    /// Register a hover enter handler
    ///
    /// # Example
    ///
    /// ```ignore
    /// div()
    ///     .on_hover_enter(|_| println!("Mouse entered!"))
    ///     .on_hover_leave(|_| println!("Mouse left!"))
    /// ```
    pub fn on_hover_enter<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + 'static,
    {
        self.event_handlers.on_hover_enter(handler);
        self
    }

    /// Register a hover leave handler
    pub fn on_hover_leave<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + 'static,
    {
        self.event_handlers.on_hover_leave(handler);
        self
    }

    /// Register a focus handler
    pub fn on_focus<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + 'static,
    {
        self.event_handlers.on_focus(handler);
        self
    }

    /// Register a blur handler
    pub fn on_blur<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + 'static,
    {
        self.event_handlers.on_blur(handler);
        self
    }

    /// Register a mount handler (element added to tree)
    pub fn on_mount<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + 'static,
    {
        self.event_handlers.on_mount(handler);
        self
    }

    /// Register an unmount handler (element removed from tree)
    pub fn on_unmount<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + 'static,
    {
        self.event_handlers.on_unmount(handler);
        self
    }

    /// Register a key down handler (requires focus)
    pub fn on_key_down<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + 'static,
    {
        self.event_handlers.on_key_down(handler);
        self
    }

    /// Register a key up handler (requires focus)
    pub fn on_key_up<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + 'static,
    {
        self.event_handlers.on_key_up(handler);
        self
    }

    /// Register a scroll handler
    pub fn on_scroll<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + 'static,
    {
        self.event_handlers.on_scroll(handler);
        self
    }

    /// Register a mouse move handler (pointer movement over this element)
    pub fn on_mouse_move<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + 'static,
    {
        self.event_handlers
            .on(blinc_core::events::event_types::POINTER_MOVE, handler);
        self
    }

    /// Mark this element as a window drag region (custom title bar).
    ///
    /// When the user presses the mouse on this element, the OS window drag
    /// operation starts — the window follows the cursor until release.
    /// Use with `WindowConfig { decorations: false, .. }` for frameless windows.
    ///
    /// # Example
    /// ```ignore
    /// // Custom title bar
    /// div()
    ///     .w_full().h(32.0)
    ///     .bg(Color::rgba(0.1, 0.1, 0.1, 1.0))
    ///     .drag_region()
    ///     .child(text("My App").color(Color::WHITE))
    /// ```
    pub fn drag_region(self) -> Self {
        self.on_mouse_down(|_ctx| {
            crate::window_actions::drag_window();
        })
    }

    /// Register a drag handler (pointer movement while button is pressed)
    pub fn on_drag<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + 'static,
    {
        self.event_handlers
            .on(blinc_core::events::event_types::DRAG, handler);
        self
    }

    /// Register a drag end handler (pointer movement while button is pressed)
    pub fn on_drag_end<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + 'static,
    {
        self.event_handlers
            .on(blinc_core::events::event_types::DRAG_END, handler);
        self
    }

    /// Register a file drop handler (fires when files are dropped onto this element)
    pub fn on_file_drop<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + 'static,
    {
        self.event_handlers
            .on(blinc_core::events::event_types::FILE_DROP, handler);
        self
    }

    /// Register a file drag-over handler (fires when files hover over this element)
    pub fn on_file_drag_over<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + 'static,
    {
        self.event_handlers
            .on(blinc_core::events::event_types::FILE_DRAG_OVER, handler);
        self
    }

    /// Register a file drag-leave handler (fires when file drag exits this element)
    pub fn on_file_drag_leave<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + 'static,
    {
        self.event_handlers
            .on(blinc_core::events::event_types::FILE_DRAG_LEAVE, handler);
        self
    }

    /// Register a pinch zoom gesture handler.
    /// `ctx.pinch_scale` contains the scale delta (1.0 = no change).
    pub fn on_pinch<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + 'static,
    {
        self.event_handlers
            .on(blinc_core::events::event_types::PINCH, handler);
        self
    }

    /// Register a rotation gesture handler.
    /// `ctx.rotation_delta` contains the angle delta in radians.
    pub fn on_rotate<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + 'static,
    {
        self.event_handlers
            .on(blinc_core::events::event_types::ROTATE, handler);
        self
    }

    /// Register a text input handler (receives character input when focused)
    pub fn on_text_input<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + 'static,
    {
        self.event_handlers.on_text_input(handler);
        self
    }

    /// Register a resize handler
    pub fn on_resize<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + 'static,
    {
        self.event_handlers.on_resize(handler);
        self
    }

    /// Register a handler for a specific event type
    ///
    /// This is the low-level method for registering handlers for any event type.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use blinc_core::events::event_types;
    ///
    /// div().on_event(event_types::POINTER_MOVE, |ctx| {
    ///     println!("Mouse moved to ({}, {})", ctx.mouse_x, ctx.mouse_y);
    /// })
    /// ```
    pub fn on_event<F>(mut self, event_type: blinc_core::events::EventType, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + 'static,
    {
        self.event_handlers.on(event_type, handler);
        self
    }
}

/// Element type identifier for downcasting
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ElementTypeId {
    Div,
    Text,
    /// Styled text with inline formatting spans
    StyledText,
    Svg,
    Image,
    Canvas,
    /// Motion container (transparent wrapper for animations)
    Motion,
}

/// Text alignment options
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextAlign {
    /// Align text to the left (default)
    #[default]
    Left,
    /// Center text
    Center,
    /// Align text to the right
    Right,
}

/// Font weight options
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FontWeight {
    /// Thin (100)
    Thin,
    /// Extra Light (200)
    ExtraLight,
    /// Light (300)
    Light,
    /// Normal/Regular (400)
    #[default]
    Normal,
    /// Medium (500)
    Medium,
    /// Semi Bold (600)
    SemiBold,
    /// Bold (700)
    Bold,
    /// Extra Bold (800)
    ExtraBold,
    /// Black (900)
    Black,
}

impl FontWeight {
    /// Get the numeric weight value (100-900)
    pub fn weight(&self) -> u16 {
        match self {
            FontWeight::Thin => 100,
            FontWeight::ExtraLight => 200,
            FontWeight::Light => 300,
            FontWeight::Normal => 400,
            FontWeight::Medium => 500,
            FontWeight::SemiBold => 600,
            FontWeight::Bold => 700,
            FontWeight::ExtraBold => 800,
            FontWeight::Black => 900,
        }
    }
}

/// Vertical alignment for text within its bounding box
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextVerticalAlign {
    /// Text is positioned at the top of its bounding box (default for multi-line text).
    /// Glyphs are centered within the line height for proper vertical centering.
    #[default]
    Top,
    /// Text is optically centered within its bounding box (for single-line centered text).
    /// Uses cap-height based centering for better visual appearance.
    Center,
    /// Text is positioned by its baseline. Use this for inline text that should
    /// align by baseline with other text elements (e.g., mixing fonts in a paragraph).
    /// The baseline position is at a standardized offset from the top of the bounding box.
    Baseline,
}

/// Generic font category for fallback when a named font isn't available
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GenericFont {
    /// Default system UI font
    #[default]
    System,
    /// Monospace font for code (Menlo, Consolas, Monaco, etc.)
    Monospace,
    /// Serif font (Times, Georgia, etc.)
    Serif,
    /// Sans-serif font (Helvetica, Arial, etc.)
    SansSerif,
}

/// Font family specification
///
/// Allows specifying either a named font (e.g., "Fira Code", "Inter") or
/// a generic category. When a named font is specified, the generic category
/// serves as a fallback if the font isn't available.
///
/// # Example
///
/// ```ignore
/// // Use a specific font with monospace fallback
/// text("code").font("Fira Code")
///
/// // Use system monospace
/// text("code").monospace()
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FontFamily {
    /// Specific font name (e.g., "Fira Code", "Inter", "SF Pro")
    pub name: Option<String>,
    /// Generic fallback category
    pub generic: GenericFont,
}

impl FontFamily {
    /// Create a font family with just a generic category
    pub fn generic(generic: GenericFont) -> Self {
        Self {
            name: None,
            generic,
        }
    }

    /// Create a font family with a specific font name
    pub fn named(name: impl Into<String>) -> Self {
        Self {
            name: Some(name.into()),
            generic: GenericFont::System,
        }
    }

    /// Create a font family with a specific name and fallback category
    pub fn named_with_fallback(name: impl Into<String>, generic: GenericFont) -> Self {
        Self {
            name: Some(name.into()),
            generic,
        }
    }

    /// System UI font
    pub fn system() -> Self {
        Self::generic(GenericFont::System)
    }

    /// Monospace font
    pub fn monospace() -> Self {
        Self::generic(GenericFont::Monospace)
    }

    /// Serif font
    pub fn serif() -> Self {
        Self::generic(GenericFont::Serif)
    }

    /// Sans-serif font
    pub fn sans_serif() -> Self {
        Self::generic(GenericFont::SansSerif)
    }
}

/// Text render data extracted from element
#[derive(Clone)]
pub struct TextRenderInfo {
    pub content: String,
    pub font_size: f32,
    pub color: [f32; 4],
    pub align: TextAlign,
    pub weight: FontWeight,
    /// Whether to use italic style
    pub italic: bool,
    pub v_align: TextVerticalAlign,
    /// Whether to wrap text at container bounds (default: true for text())
    pub wrap: bool,
    /// Line height multiplier (default: 1.2)
    pub line_height: f32,
    /// Measured width of the text (before any layout constraints)
    /// Used to determine if wrapping is actually needed at render time
    pub measured_width: f32,
    /// Font family category
    pub font_family: FontFamily,
    /// Word spacing in pixels (0.0 = normal)
    pub word_spacing: f32,
    /// Letter spacing in pixels (0.0 = normal)
    pub letter_spacing: f32,
    /// Font ascender in pixels (distance from baseline to top)
    /// Used for accurate baseline alignment across different fonts
    pub ascender: f32,
    /// Whether text has strikethrough decoration
    pub strikethrough: bool,
    /// Whether text has underline decoration
    pub underline: bool,
}

/// A span within styled text (for rich_text element)
#[derive(Clone)]
pub struct StyledTextSpanInfo {
    /// Start byte index
    pub start: usize,
    /// End byte index (exclusive)
    pub end: usize,
    /// RGBA color
    pub color: [f32; 4],
    /// Whether span is bold
    pub bold: bool,
    /// Whether span is italic
    pub italic: bool,
    /// Whether span has underline
    pub underline: bool,
    /// Whether span has strikethrough
    pub strikethrough: bool,
    /// Optional link URL
    pub link_url: Option<String>,
}

/// Styled text render data (for rich_text element with inline formatting)
#[derive(Clone)]
pub struct StyledTextRenderInfo {
    /// Plain text content (tags stripped)
    pub content: String,
    /// Style spans
    pub spans: Vec<StyledTextSpanInfo>,
    /// Font size
    pub font_size: f32,
    /// Default color for unspanned regions
    pub default_color: [f32; 4],
    /// Text alignment
    pub align: TextAlign,
    /// Vertical alignment
    pub v_align: TextVerticalAlign,
    /// Font family
    pub font_family: FontFamily,
    /// Line height multiplier
    pub line_height: f32,
    /// Default font weight (for unspanned regions or spans without explicit weight)
    pub weight: FontWeight,
    /// Default italic style (for unspanned regions or spans without explicit italic)
    pub italic: bool,
    /// Measured ascender from font metrics (for consistent baseline alignment)
    pub ascender: f32,
}

/// SVG render data extracted from element
#[derive(Clone)]
pub struct SvgRenderInfo {
    pub source: Arc<str>,
    pub tint: Option<blinc_core::Color>,
    pub fill: Option<blinc_core::Color>,
    pub stroke: Option<blinc_core::Color>,
    pub stroke_width: Option<f32>,
}

/// Image render data extracted from element
#[derive(Clone)]
pub struct ImageRenderInfo {
    /// Image source (file path, URL, or base64 data)
    pub source: String,
    /// Object-fit mode (cover, contain, fill, scale-down, none)
    pub object_fit: u8,
    /// Object position (x: 0.0-1.0, y: 0.0-1.0)
    pub object_position: [f32; 2],
    /// Opacity (0.0 - 1.0)
    pub opacity: f32,
    /// Border radius for rounded corners
    pub border_radius: f32,
    /// Tint color [r, g, b, a]
    pub tint: [f32; 4],
    /// Filter: [grayscale, sepia, brightness, contrast, saturate, hue_rotate, invert, blur]
    pub filter: [f32; 8],
    /// Loading strategy: 0 = Eager (default), 1 = Lazy
    pub loading_strategy: u8,
    /// Placeholder type: 0 = None, 1 = Color, 2 = Image, 3 = Skeleton
    pub placeholder_type: u8,
    /// Placeholder color (only used when placeholder_type == 1)
    pub placeholder_color: [f32; 4],
    /// Placeholder image source (only used when placeholder_type == 2)
    pub placeholder_image: Option<String>,
    /// Fade-in duration in milliseconds
    pub fade_duration_ms: u32,
}

impl Default for ImageRenderInfo {
    fn default() -> Self {
        Self {
            source: String::new(),
            object_fit: 0,               // Cover
            object_position: [0.5, 0.5], // Center
            opacity: 1.0,
            border_radius: 0.0,
            tint: [1.0, 1.0, 1.0, 1.0],
            filter: [0.0, 0.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0], // identity filter
            loading_strategy: 0,                              // Eager
            placeholder_type: 1,                              // Color (default)
            placeholder_color: [0.15, 0.15, 0.15, 0.5],       // Default gray
            placeholder_image: None,
            fade_duration_ms: 200,
        }
    }
}

/// Trait for types that can build into layout elements
///
/// Note: No Send/Sync requirement - UI is single-threaded.
pub trait ElementBuilder {
    /// Build this element into a layout tree, returning the node ID
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId;

    /// Get the render properties for this element
    fn render_props(&self) -> RenderProps;

    /// Get children builders (for recursive traversal)
    fn children_builders(&self) -> &[Box<dyn ElementBuilder>];

    /// Get the element type identifier
    fn element_type_id(&self) -> ElementTypeId {
        ElementTypeId::Div
    }

    /// Get text render info if this is a text element
    fn text_render_info(&self) -> Option<TextRenderInfo> {
        None
    }

    /// Get styled text render info if this is a styled text element (rich_text)
    fn styled_text_render_info(&self) -> Option<StyledTextRenderInfo> {
        None
    }

    /// Get SVG render info if this is an SVG element
    fn svg_render_info(&self) -> Option<SvgRenderInfo> {
        None
    }

    /// Get image render info if this is an image element
    fn image_render_info(&self) -> Option<ImageRenderInfo> {
        None
    }

    /// Get canvas render info if this is a canvas element
    fn canvas_render_info(&self) -> Option<crate::canvas::CanvasRenderFn> {
        None
    }

    /// Whether this canvas's `render_fn` produces deterministic output
    /// for a given bounds + closure captures, with no per-frame state
    /// (animations, timers, live signal reads). Static canvases get
    /// emitted to the bg batch normally but skip the per-frame
    /// overlay re-paint that would otherwise overdraw any children
    /// the walker layered on top of them in the static cache. They
    /// also don't flip the `had_canvas_painted` gate, so the
    /// compositor fast path can still engage on frames where the
    /// only canvases are static.
    ///
    /// Default `false` — applies only when `element_type_id` is
    /// [`ElementTypeId::Canvas`]; ignored otherwise.
    fn is_static_canvas(&self) -> bool {
        false
    }

    /// The taffy `Style` that this element actually installs on its
    /// layout node — i.e. the style after any widget-internal
    /// adjustments `build()` would apply. The default delegates to
    /// `layout_style().cloned()`, which is correct for elements
    /// whose `build()` just forwards `self.style` verbatim. Override
    /// when `build()` mutates the style before handing it to taffy
    /// (e.g. `Notch::build` adds `scoop.depth` to padding so children
    /// don't render over the carved-out region).
    ///
    /// The layout-prop fast path in `process_pending_subtree_rebuilds`
    /// reads this via `layout_tree.set_style(node, effective)` to
    /// reflow a Stateful subtree without rebuilding it. If the
    /// element's effective style diverges from `layout_style()`, the
    /// fast path patches the wrong style onto taffy and the layout
    /// silently shifts (Notch's bottom-dock icons jumping to y=0 was
    /// exactly this — the scoop padding was missing from the patched
    /// style).
    fn effective_layout_style(&self) -> Option<taffy::Style> {
        self.layout_style().cloned()
    }

    /// Get event handlers for this element
    ///
    /// Returns a reference to the element's event handlers for registration
    /// with the handler registry during tree building.
    fn event_handlers(&self) -> Option<&crate::event_handler::EventHandlers> {
        None
    }

    /// Get scroll render info if this is a scroll element
    fn scroll_info(&self) -> Option<crate::scroll::ScrollRenderInfo> {
        None
    }

    /// Get scroll physics handle if this is a scroll element
    fn scroll_physics(&self) -> Option<crate::scroll::SharedScrollPhysics> {
        None
    }

    /// Whether this element opts in to viewport culling for its
    /// descendants. The renderer skips painting any descendant whose
    /// post-scroll bounds don't intersect the visible viewport (plus a
    /// small overscan buffer). Only meaningful on scroll containers.
    fn viewport_cull(&self) -> bool {
        false
    }

    /// Get motion animation config for a child at given index
    ///
    /// This is only implemented by Motion containers. The index corresponds
    /// to the order of children as returned by children_builders().
    fn motion_animation_for_child(
        &self,
        _child_index: usize,
    ) -> Option<crate::element::MotionAnimation> {
        None
    }

    /// Get motion bindings for continuous animation
    ///
    /// If this element has bound animated values (e.g., translate_y, opacity),
    /// they are returned here so the RenderTree can sample them each frame.
    fn motion_bindings(&self) -> Option<crate::motion::MotionBindings> {
        None
    }

    /// Get stable ID for motion animation tracking
    ///
    /// When set, the motion animation state is tracked by this stable key
    /// instead of node ID, allowing animations to persist across tree rebuilds.
    /// This is essential for overlays which are rebuilt every frame.
    fn motion_stable_id(&self) -> Option<&str> {
        None
    }

    /// Check if this motion should replay its animation
    ///
    /// When true, the animation restarts from the beginning even if
    /// the motion already exists with the same stable key.
    fn motion_should_replay(&self) -> bool {
        false
    }

    /// Check if this motion should start in suspended state
    ///
    /// When true, the motion starts with opacity 0 and waits for
    /// `query_motion(key).start()` to trigger the enter animation.
    fn motion_is_suspended(&self) -> bool {
        false
    }

    /// DEPRECATED: Check if this motion should start its exit animation
    ///
    /// This method is deprecated. Motion exit is now triggered explicitly via
    /// `MotionHandle.exit()` / `query_motion(key).exit()` instead of capturing
    /// the is_overlay_closing() flag at build time.
    ///
    /// The old mechanism was flawed because the flag reset after build_content(),
    /// breaking multi-frame exit animations.
    #[deprecated(
        since = "0.1.0",
        note = "Use query_motion(key).exit() to explicitly trigger motion exit"
    )]
    fn motion_is_exiting(&self) -> bool {
        false
    }

    /// Get the layout style for this element
    ///
    /// This is used for hashing layout-affecting properties like size,
    /// padding, margin, flex properties, etc.
    fn layout_style(&self) -> Option<&taffy::Style> {
        None
    }

    /// Get layout bounds storage for this element
    ///
    /// If this element wants to be notified of its computed layout bounds
    /// after layout is calculated, it returns a storage that will be updated.
    fn layout_bounds_storage(&self) -> Option<crate::renderer::LayoutBoundsStorage> {
        None
    }

    /// Get layout bounds change callback for this element
    ///
    /// If this element needs to react when its layout bounds change (e.g., TextInput
    /// needs to recalculate scroll when width changes), it returns a callback that
    /// will be invoked when the bounds differ from the previous layout.
    fn layout_bounds_callback(&self) -> Option<crate::renderer::LayoutBoundsCallback> {
        None
    }

    /// Semantic HTML-like tag name for CSS type selectors (e.g., "button", "a", "ul")
    ///
    /// Widgets override this to declare their semantic type, enabling global CSS
    /// rules like `button { background: blue; }` to match all instances.
    fn semantic_type_name(&self) -> Option<&'static str> {
        None
    }

    /// Get the element ID for selector API queries
    ///
    /// Elements with IDs can be looked up programmatically via `ctx.query("id")`.
    fn element_id(&self) -> Option<&str> {
        None
    }

    /// Set an auto-generated element ID (used by stateful containers for stable child IDs).
    ///
    /// Only sets the ID if one hasn't been explicitly assigned by the user.
    /// Returns true if the ID was set, false if an explicit ID already existed.
    fn set_auto_id(&mut self, _id: String) -> bool {
        false
    }

    /// Get mutable access to children for auto-ID assignment
    fn children_builders_mut(&mut self) -> &mut [Box<dyn ElementBuilder>] {
        &mut []
    }

    /// Get the element's CSS class list for selector matching.
    ///
    /// Returns interned `Arc<str>` rather than owned `String` so that
    /// repeated class names (e.g. `"cn-button"` used by hundreds of
    /// nodes) share a single allocation. See [`blinc_core::intern`].
    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        &[]
    }

    /// Get the bound ScrollRef for programmatic scroll control
    ///
    /// Only scroll containers return a ScrollRef. This is used by the renderer
    /// to bind the ScrollRef to the node and process pending scroll commands.
    fn bound_scroll_ref(&self) -> Option<&crate::selector::ScrollRef> {
        None
    }

    /// Get the on_ready callback for this motion container
    ///
    /// Motion containers can register a callback that fires once after the
    /// motion is laid out. Used with suspended animations to start the
    /// animation after content is mounted.
    fn motion_on_ready_callback(
        &self,
    ) -> Option<std::sync::Arc<dyn Fn(crate::element::ElementBounds) + Send + Sync>> {
        None
    }

    /// Get layout animation configuration for this element
    ///
    /// If this element should animate its layout bounds changes (position, size),
    /// it returns a configuration specifying which properties to animate
    /// and the spring physics settings.
    ///
    /// Layout animation uses FLIP-style animation: layout settles instantly
    /// while the visual rendering interpolates from old bounds to new bounds.
    fn layout_animation_config(&self) -> Option<crate::layout_animation::LayoutAnimationConfig> {
        None
    }

    /// Get visual animation config for new FLIP-style animations (read-only layout)
    ///
    /// Unlike layout_animation_config, this system never modifies taffy.
    /// The animation tracks visual offsets that get animated back to zero.
    fn visual_animation_config(&self) -> Option<crate::visual_animation::VisualAnimationConfig> {
        None
    }
}

/// Forwarding impl so `Box<dyn ElementBuilder>` itself satisfies
/// `ElementBuilder`. Important for the edition-2024 HRTB workaround
/// documented on [`crate::renderer::RenderTree::from_element`] (and on
/// `WindowedApp::run` / `run_with_theme`): downstream crates whose
/// `fn build_ui(ctx) -> impl Element` lacks `+ use<>` cannot satisfy
/// the higher-ranked `FnMut` bound that runs the UI builder. The
/// type-erased escape hatch — wrapping the call site in
/// `|cx| -> Box<dyn ElementBuilder> { Box::new(build_ui(cx)) }` —
/// only works if the boxed trait object itself implements
/// `ElementBuilder`, hence this blanket. Forwards every method (incl.
/// the two `&mut self` ones) through the box's auto-deref.
///
/// See [[gotcha-run-with-theme-hrtb-fnmut]] for the full reasoning.
impl<T: ?Sized + ElementBuilder> ElementBuilder for Box<T> {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        (**self).build(tree)
    }
    fn render_props(&self) -> RenderProps {
        (**self).render_props()
    }
    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        (**self).children_builders()
    }
    fn element_type_id(&self) -> ElementTypeId {
        (**self).element_type_id()
    }
    fn text_render_info(&self) -> Option<TextRenderInfo> {
        (**self).text_render_info()
    }
    fn styled_text_render_info(&self) -> Option<StyledTextRenderInfo> {
        (**self).styled_text_render_info()
    }
    fn svg_render_info(&self) -> Option<SvgRenderInfo> {
        (**self).svg_render_info()
    }
    fn image_render_info(&self) -> Option<ImageRenderInfo> {
        (**self).image_render_info()
    }
    fn canvas_render_info(&self) -> Option<crate::canvas::CanvasRenderFn> {
        (**self).canvas_render_info()
    }
    fn is_static_canvas(&self) -> bool {
        (**self).is_static_canvas()
    }
    fn effective_layout_style(&self) -> Option<taffy::Style> {
        (**self).effective_layout_style()
    }
    fn event_handlers(&self) -> Option<&crate::event_handler::EventHandlers> {
        (**self).event_handlers()
    }
    fn scroll_info(&self) -> Option<crate::scroll::ScrollRenderInfo> {
        (**self).scroll_info()
    }
    fn scroll_physics(&self) -> Option<crate::scroll::SharedScrollPhysics> {
        (**self).scroll_physics()
    }
    fn viewport_cull(&self) -> bool {
        (**self).viewport_cull()
    }
    fn motion_animation_for_child(
        &self,
        child_index: usize,
    ) -> Option<crate::element::MotionAnimation> {
        (**self).motion_animation_for_child(child_index)
    }
    fn motion_bindings(&self) -> Option<crate::motion::MotionBindings> {
        (**self).motion_bindings()
    }
    fn motion_stable_id(&self) -> Option<&str> {
        (**self).motion_stable_id()
    }
    fn motion_should_replay(&self) -> bool {
        (**self).motion_should_replay()
    }
    fn motion_is_suspended(&self) -> bool {
        (**self).motion_is_suspended()
    }
    fn motion_is_exiting(&self) -> bool {
        (**self).motion_is_exiting()
    }
    fn layout_style(&self) -> Option<&taffy::Style> {
        (**self).layout_style()
    }
    fn layout_bounds_storage(&self) -> Option<crate::renderer::LayoutBoundsStorage> {
        (**self).layout_bounds_storage()
    }
    fn layout_bounds_callback(&self) -> Option<crate::renderer::LayoutBoundsCallback> {
        (**self).layout_bounds_callback()
    }
    fn semantic_type_name(&self) -> Option<&'static str> {
        (**self).semantic_type_name()
    }
    fn element_id(&self) -> Option<&str> {
        (**self).element_id()
    }
    fn set_auto_id(&mut self, id: String) -> bool {
        (**self).set_auto_id(id)
    }
    fn children_builders_mut(&mut self) -> &mut [Box<dyn ElementBuilder>] {
        (**self).children_builders_mut()
    }
    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        (**self).element_classes()
    }
    fn bound_scroll_ref(&self) -> Option<&crate::selector::ScrollRef> {
        (**self).bound_scroll_ref()
    }
    fn motion_on_ready_callback(
        &self,
    ) -> Option<std::sync::Arc<dyn Fn(crate::element::ElementBounds) + Send + Sync>> {
        (**self).motion_on_ready_callback()
    }
    fn layout_animation_config(&self) -> Option<crate::layout_animation::LayoutAnimationConfig> {
        (**self).layout_animation_config()
    }
    fn visual_animation_config(&self) -> Option<crate::visual_animation::VisualAnimationConfig> {
        (**self).visual_animation_config()
    }
}

impl ElementBuilder for Div {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        let node = tree.create_node(self.style.clone());

        // Build and add children
        for child in &self.children {
            let child_node = child.build(tree);
            tree.add_child(node, child_node);
        }

        // Register signal-bound property bindings against the freshly
        // minted node id. Each pending binding installs a subscription
        // in the global property-binding registry; subsequent
        // `State<T>::set` calls fire writers that queue partial updates
        // through the unified property channel.
        // ([[project-reactive-architecture-v2]] Phase 2.)
        for binding in &self.pending_bindings {
            binding.register(node);
        }

        node
    }

    #[allow(deprecated)]
    fn render_props(&self) -> RenderProps {
        // Check if overflow is set to clip content (Clip or Scroll)
        // Overflow::Visible is the only mode that doesn't clip
        let clips_content = !matches!(self.style.overflow.x, Overflow::Visible)
            || !matches!(self.style.overflow.y, Overflow::Visible);

        RenderProps {
            background: self.background.clone(),
            border_radius: self.border_radius,
            border_radius_explicit: self.border_radius_explicit,
            corner_shape: self.corner_shape,
            corner_shape_locked: self.corner_shape_locked,
            border_color: self.border_color,
            border_width: self.border_width,
            border_sides: self.border_sides,
            layer: self.render_layer,
            material: self.material.clone(),
            shadow: self.shadow.clone(),
            transform: self.transform.clone(),
            transform_origin: self.transform_origin,
            opacity: self.opacity,
            clips_content,
            is_stack_layer: self.is_stack_layer,
            is_overlay_root: self.is_overlay_root,
            pointer_events_none: self.pointer_events_none,
            cursor: self.cursor,
            layer_effects: self.layer_effects.clone(),
            rotate_x: self.rotate_x,
            rotate_y: self.rotate_y,
            perspective: self.perspective_3d,
            shape_3d: self.shape_3d,
            depth: self.depth,
            light_direction: self.light_direction,
            light_intensity: self.light_intensity,
            ambient: self.ambient,
            specular: self.specular,
            translate_z: self.translate_z,
            op_3d: self.op_3d,
            blend_3d: self.blend_3d,
            clip_path: self.clip_path.clone(),
            overflow_fade: self.overflow_fade,
            flow: self.flow_name.clone(),
            flow_graph: self.flow_graph.clone(),
            outline_color: self.outline_color,
            outline_width: self.outline_width,
            outline_offset: self.outline_offset,
            is_fixed: self.is_fixed,
            is_sticky: self.is_sticky,
            sticky_top: self.sticky_top,
            z_index: self.z_index,
            ..Default::default()
        }
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        &self.children
    }

    fn event_handlers(&self) -> Option<&crate::event_handler::EventHandlers> {
        if self.event_handlers.is_empty() {
            None
        } else {
            Some(&self.event_handlers)
        }
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        Some(&self.style)
    }

    fn element_id(&self) -> Option<&str> {
        self.element_id.as_deref()
    }

    fn set_auto_id(&mut self, id: String) -> bool {
        if self.element_id.is_none() {
            self.element_id = Some(id);
            true
        } else {
            false
        }
    }

    fn children_builders_mut(&mut self) -> &mut [Box<dyn ElementBuilder>] {
        &mut self.children
    }

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        &self.classes
    }

    fn layout_animation_config(&self) -> Option<crate::layout_animation::LayoutAnimationConfig> {
        self.layout_animation.clone()
    }

    fn visual_animation_config(&self) -> Option<crate::visual_animation::VisualAnimationConfig> {
        self.visual_animation.clone()
    }

    fn scroll_physics(&self) -> Option<crate::scroll::SharedScrollPhysics> {
        self.scroll_physics.clone()
    }
}

/// Convenience function to create a new div
pub fn div() -> Div {
    Div::new()
}

// Stack has been moved to stack.rs

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::RenderTree;

    #[test]
    fn test_div_builder() {
        let d = div().w(100.0).h(50.0).flex_row().gap(2.0).p(4.0);

        assert!(matches!(d.style.display, Display::Flex));
        assert!(matches!(d.style.flex_direction, FlexDirection::Row));
    }

    #[test]
    fn test_div_with_children() {
        let parent = div().flex_col().child(div().h(20.0)).child(div().h(30.0));

        assert_eq!(parent.children.len(), 2);
    }

    #[test]
    fn test_build_tree() {
        let ui = div().flex_col().child(div().h(20.0)).child(div().h(30.0));

        let mut tree = LayoutTree::new();
        let root = ui.build(&mut tree);

        assert_eq!(tree.len(), 3);
        assert_eq!(tree.children(root).len(), 2);
    }

    #[test]
    fn test_layout_flex_row_with_fixed_children() {
        // Three fixed-width children in a row
        let ui = div()
            .w(300.0)
            .h(100.0)
            .flex_row()
            .child(div().w(50.0).h(100.0))
            .child(div().w(100.0).h(100.0))
            .child(div().w(50.0).h(100.0));

        let mut tree = RenderTree::from_element(&ui);
        tree.compute_layout(300.0, 100.0);

        let root = tree.root().unwrap();
        let children: Vec<_> = tree.layout_tree.children(root);

        // First child at x=0
        let first = tree
            .layout_tree
            .get_bounds(children[0], (0.0, 0.0))
            .unwrap();
        assert_eq!(first.x, 0.0);
        assert_eq!(first.width, 50.0);

        // Second child at x=50
        let second = tree
            .layout_tree
            .get_bounds(children[1], (0.0, 0.0))
            .unwrap();
        assert_eq!(second.x, 50.0);
        assert_eq!(second.width, 100.0);

        // Third child at x=150
        let third = tree
            .layout_tree
            .get_bounds(children[2], (0.0, 0.0))
            .unwrap();
        assert_eq!(third.x, 150.0);
        assert_eq!(third.width, 50.0);
    }

    #[test]
    fn test_layout_flex_col_with_gap() {
        // Column with gap between children (10px gap using gap_px)
        let ui = div()
            .w(100.0)
            .h(200.0)
            .flex_col()
            .gap_px(10.0) // 10px gap
            .child(div().w_full().h(40.0))
            .child(div().w_full().h(40.0))
            .child(div().w_full().h(40.0));

        let mut tree = RenderTree::from_element(&ui);
        tree.compute_layout(100.0, 200.0);

        let root = tree.root().unwrap();
        let children: Vec<_> = tree.layout_tree.children(root);

        // First child at y=0
        let first = tree
            .layout_tree
            .get_bounds(children[0], (0.0, 0.0))
            .unwrap();
        assert_eq!(first.y, 0.0);
        assert_eq!(first.height, 40.0);

        // Second child at y=50 (40 + 10 gap)
        let second = tree
            .layout_tree
            .get_bounds(children[1], (0.0, 0.0))
            .unwrap();
        assert_eq!(second.y, 50.0);
        assert_eq!(second.height, 40.0);

        // Third child at y=100 (50 + 40 + 10 gap)
        let third = tree
            .layout_tree
            .get_bounds(children[2], (0.0, 0.0))
            .unwrap();
        assert_eq!(third.y, 100.0);
        assert_eq!(third.height, 40.0);
    }

    #[test]
    fn test_layout_flex_grow() {
        // One fixed child, one growing child
        let ui = div()
            .w(200.0)
            .h(100.0)
            .flex_row()
            .child(div().w(50.0).h(100.0))
            .child(div().flex_grow().h(100.0));

        let mut tree = RenderTree::from_element(&ui);
        tree.compute_layout(200.0, 100.0);

        let root = tree.root().unwrap();
        let children: Vec<_> = tree.layout_tree.children(root);

        // Fixed child
        let fixed = tree
            .layout_tree
            .get_bounds(children[0], (0.0, 0.0))
            .unwrap();
        assert_eq!(fixed.width, 50.0);

        // Growing child should fill remaining space
        let growing = tree
            .layout_tree
            .get_bounds(children[1], (0.0, 0.0))
            .unwrap();
        assert_eq!(growing.x, 50.0);
        assert_eq!(growing.width, 150.0);
    }

    #[test]
    fn test_layout_padding() {
        // Container with padding
        let ui = div()
            .w(100.0)
            .h(100.0)
            .p(2.0) // 8px padding (2 * 4px base unit)
            .child(div().w_full().h_full());

        let mut tree = RenderTree::from_element(&ui);
        tree.compute_layout(100.0, 100.0);

        let root = tree.root().unwrap();
        let children: Vec<_> = tree.layout_tree.children(root);

        // Child should be inset by padding
        let child = tree
            .layout_tree
            .get_bounds(children[0], (0.0, 0.0))
            .unwrap();
        assert_eq!(child.x, 8.0);
        assert_eq!(child.y, 8.0);
        assert_eq!(child.width, 84.0); // 100 - 8 - 8
        assert_eq!(child.height, 84.0);
    }

    #[test]
    fn test_layout_justify_between() {
        // Three children with space between
        let ui = div()
            .w(200.0)
            .h(50.0)
            .flex_row()
            .justify_between()
            .child(div().w(30.0).h(50.0))
            .child(div().w(30.0).h(50.0))
            .child(div().w(30.0).h(50.0));

        let mut tree = RenderTree::from_element(&ui);
        tree.compute_layout(200.0, 50.0);

        let root = tree.root().unwrap();
        let children: Vec<_> = tree.layout_tree.children(root);

        // First at start
        let first = tree
            .layout_tree
            .get_bounds(children[0], (0.0, 0.0))
            .unwrap();
        assert_eq!(first.x, 0.0);

        // Last at end
        let third = tree
            .layout_tree
            .get_bounds(children[2], (0.0, 0.0))
            .unwrap();
        assert_eq!(third.x, 170.0); // 200 - 30

        // Middle should be centered between first and third
        let second = tree
            .layout_tree
            .get_bounds(children[1], (0.0, 0.0))
            .unwrap();
        assert_eq!(second.x, 85.0); // (170 - 0) / 2 - 30/2 + 30/2 = 85
    }

    #[test]
    fn test_nested_layout() {
        // Nested flex containers
        let ui = div()
            .w(200.0)
            .h(200.0)
            .flex_col()
            .child(
                div()
                    .w_full()
                    .h(50.0)
                    .flex_row()
                    .child(div().w(50.0).h(50.0))
                    .child(div().flex_grow().h(50.0)),
            )
            .child(div().w_full().flex_grow());

        let mut tree = RenderTree::from_element(&ui);
        tree.compute_layout(200.0, 200.0);

        let root = tree.root().unwrap();
        let root_bounds = tree.get_bounds(root).unwrap();
        assert_eq!(root_bounds.width, 200.0);
        assert_eq!(root_bounds.height, 200.0);

        let root_children: Vec<_> = tree.layout_tree.children(root);

        // First row
        let row = tree
            .layout_tree
            .get_bounds(root_children[0], (0.0, 0.0))
            .unwrap();
        assert_eq!(row.height, 50.0);
        assert_eq!(row.width, 200.0);

        // Second element should fill remaining height
        let bottom = tree
            .layout_tree
            .get_bounds(root_children[1], (0.0, 0.0))
            .unwrap();
        assert_eq!(bottom.y, 50.0);
        assert_eq!(bottom.height, 150.0);
    }

    #[test]
    fn test_element_ref_basic() {
        // Create a ref
        let div_ref: ElementRef<Div> = ElementRef::new();

        assert!(!div_ref.is_bound());

        // Set a div
        div_ref.set(div().w(100.0).h(50.0).bg(Color::BLUE));

        assert!(div_ref.is_bound());

        // Read from the ref
        let width = div_ref.with(|d| d.style.size.width);
        assert!(matches!(width, Some(Dimension::Length(100.0))));
    }

    #[test]
    fn test_element_ref_with_mut() {
        let div_ref: ElementRef<Div> = ElementRef::new();

        div_ref.set(div().w(100.0));

        // Modify through the ref
        div_ref.with_mut(|d| {
            *d = d.swap().h(200.0).bg(Color::RED);
        });

        // Verify modification
        let height = div_ref.with(|d| d.style.size.height);
        assert!(matches!(height, Some(Dimension::Length(200.0))));
    }

    #[test]
    fn test_element_ref_clone() {
        let div_ref: ElementRef<Div> = ElementRef::new();
        let div_ref_clone = div_ref.clone();

        // Set on original
        div_ref.set(div().w(100.0));

        // Should be visible on clone (shared storage)
        assert!(div_ref_clone.is_bound());
        let width = div_ref_clone.with(|d| d.style.size.width);
        assert!(matches!(width, Some(Dimension::Length(100.0))));
    }

    /// `h_fit()` on a child in a flex-row parent must actually shrink
    /// the child to content. Regression for the Lottie gallery bug.
    #[test]
    fn test_h_fit_shrinks_in_flex_row_parent() {
        let ui = div().w(800.0).h(600.0).flex_row().child(
            div()
                .w(340.0)
                .h_fit()
                .flex_col()
                // Two fixed-height children totalling 140px.
                .child(div().w(340.0).h(100.0))
                .child(div().w(340.0).h(40.0)),
        );

        let mut tree = RenderTree::from_element(&ui);
        tree.compute_layout(800.0, 600.0);

        let root = tree.root().unwrap();
        let children: Vec<_> = tree.layout_tree.children(root);
        let card = tree
            .layout_tree
            .get_bounds(children[0], (0.0, 0.0))
            .unwrap();

        assert_eq!(
            card.height, 140.0,
            "h_fit child should be 140px (sum of children), not {} (parent's 600)",
            card.height
        );
    }

    /// Same as above but the parent is fullscreen-huge. If
    /// `align-content: Stretch` on the wrap line grows the line to
    /// fill and drags the child, this will also fail — captures the
    /// fullscreen-regression reported by the user.
    #[test]
    fn test_h_fit_shrinks_in_large_flex_row_parent() {
        let ui = div().w(2000.0).h(1400.0).flex_row().child(
            div()
                .w(340.0)
                .h_fit()
                .flex_col()
                .child(div().w(340.0).h(100.0))
                .child(div().w(340.0).h(40.0)),
        );

        let mut tree = RenderTree::from_element(&ui);
        tree.compute_layout(2000.0, 1400.0);

        let root = tree.root().unwrap();
        let children: Vec<_> = tree.layout_tree.children(root);
        let card = tree
            .layout_tree
            .get_bounds(children[0], (0.0, 0.0))
            .unwrap();

        assert_eq!(
            card.height, 140.0,
            "h_fit child in huge flex-row parent should be 140px, not {}",
            card.height
        );
    }

    /// Reproduces the exact gallery-failure shape: the card itself
    /// has both `flex_wrap()` and `h_fit()` on it (the user's
    /// workaround-of-a-workaround). `flex_wrap` on a container with
    /// an auto cross-size changes how taffy resolves the container's
    /// main-axis size. Specifically — without explicit height — a
    /// flex-column + wrap container can report its main axis as
    /// whatever available space the parent handed down, instead of
    /// the sum of its children. Regression guard.
    #[test]
    fn test_h_fit_with_flex_wrap_on_same_div() {
        // Match the gallery exactly: TWO cards side-by-side in a
        // fullscreen-sized flex-row parent. Every card has the same
        // `.flex_col().flex_wrap().h_fit().overflow_clip()` stack.
        // Children are 18 + 316 + 32 with 8px gaps and 12px padding,
        // so each card should be 12+18+8+316+8+32+12 = 406 px.
        let make_card = || {
            div()
                .w(340.0)
                .h_fit()
                .flex_col()
                .flex_wrap()
                .overflow_clip()
                .gap_px(8.0)
                .p_px(12.0)
                .child(div().w(316.0).h(18.0))
                .child(div().w(316.0).h(316.0))
                .child(div().w(316.0).h(32.0))
        };

        let ui = div()
            .w(2000.0)
            .h(1400.0)
            .flex_row()
            .gap_px(20.0)
            .child(make_card())
            .child(make_card());

        let mut tree = RenderTree::from_element(&ui);
        tree.compute_layout(2000.0, 1400.0);

        let root = tree.root().unwrap();
        let children: Vec<_> = tree.layout_tree.children(root);
        let card_0 = tree
            .layout_tree
            .get_bounds(children[0], (0.0, 0.0))
            .unwrap();
        let card_1 = tree
            .layout_tree
            .get_bounds(children[1], (0.0, 0.0))
            .unwrap();

        assert_eq!(
            card_0.height, 406.0,
            "card 0 should be 406px content height, got {}",
            card_0.height
        );
        assert_eq!(
            card_1.height, 406.0,
            "card 1 should be 406px content height, got {}",
            card_1.height
        );
    }

    /// Same card shape but with a real `canvas()` element wrapped
    /// inside a div with w_full/h_full (mirrors `sketch()`'s wrap).
    /// This catches any interaction between canvas's layout_style
    /// returning its own style (not using defaults) and the
    /// surrounding flex-wrap container.
    #[test]
    fn test_h_fit_with_real_canvas_child() {
        use crate::canvas::canvas;
        let ui = div().w(2000.0).h(1400.0).flex_row().child(
            div()
                .w(340.0)
                .h_fit()
                .flex_col()
                .flex_wrap()
                .overflow_clip()
                .gap_px(8.0)
                .p_px(12.0)
                .child(div().w(316.0).h(18.0))
                .child(
                    div().w(316.0).h(316.0).child(
                        div()
                            .w_full()
                            .h_full()
                            .child(canvas(|_ctx, _bounds| {}).w_full().h_full()),
                    ),
                )
                .child(div().w(316.0).h(32.0)),
        );

        let mut tree = RenderTree::from_element(&ui);
        tree.compute_layout(2000.0, 1400.0);

        let root = tree.root().unwrap();
        let children: Vec<_> = tree.layout_tree.children(root);
        let card = tree
            .layout_tree
            .get_bounds(children[0], (0.0, 0.0))
            .unwrap();

        assert!(
            card.height < 500.0,
            "card with canvas child should not balloon; got {}",
            card.height
        );
    }

    /// Same card shape but using a real `text()` element for the
    /// first child (wrap=true by default, so taffy uses the measure
    /// function). If the text-measure is returning an unexpectedly
    /// large main-axis size when queried at max-content, the card's
    /// hypothetical-main-size exceeds available → multi-line wrap →
    /// taffy inflates height to `max(content, available)`.
    #[test]
    fn test_h_fit_with_real_text_child() {
        use crate::text::text;
        let ui = div().w(2000.0).h(1400.0).flex_row().child(
            div()
                .w(340.0)
                .h_fit()
                .flex_col()
                .flex_wrap()
                .overflow_clip()
                .gap_px(8.0)
                .p_px(12.0)
                .child(text("Sandy Loading (JSON)").size(13.0))
                .child(div().w(316.0).h(316.0))
                .child(div().w(316.0).h(32.0)),
        );

        let mut tree = RenderTree::from_element(&ui);
        tree.compute_layout(2000.0, 1400.0);

        let root = tree.root().unwrap();
        let children: Vec<_> = tree.layout_tree.children(root);
        let card = tree
            .layout_tree
            .get_bounds(children[0], (0.0, 0.0))
            .unwrap();

        assert!(
            card.height < 500.0,
            "card with text child should not balloon to available space, got {}",
            card.height
        );
    }

    /// Reproduce the Lottie-gallery shape: outer windowed div (flex-row
    /// default) → an intermediate wrap container (`flex_row + gap`) →
    /// card with `h_fit`. The user's gallery sees the card stretch.
    /// This test pins whether the extra wrapping layer alone breaks
    /// `h_fit`, without involving any Stateful or sketch.
    #[test]
    fn test_h_fit_through_gap_container() {
        let ui = div().w(800.0).h(600.0).p_px(20.0).child(
            div().flex_row().gap_px(20.0).child(
                div()
                    .w(340.0)
                    .h_fit()
                    .flex_col()
                    .gap_px(8.0)
                    .p_px(12.0)
                    .child(div().w(316.0).h(20.0))
                    .child(div().w(316.0).h(316.0))
                    .child(div().w(316.0).h(36.0)),
            ),
        );

        let mut tree = RenderTree::from_element(&ui);
        tree.compute_layout(800.0, 600.0);

        let root = tree.root().unwrap();
        let outer_children: Vec<_> = tree.layout_tree.children(root);
        let wrap = outer_children[0];
        let wrap_children: Vec<_> = tree.layout_tree.children(wrap);
        let card = tree
            .layout_tree
            .get_bounds(wrap_children[0], (0.0, 0.0))
            .unwrap();

        // Card content: padding(12) + 20 + gap(8) + 316 + gap(8) + 36
        // + padding(12) = 412
        assert_eq!(
            card.height, 412.0,
            "gallery-shaped card in flex-row gap container should be 412px, not {}",
            card.height
        );
    }
}
