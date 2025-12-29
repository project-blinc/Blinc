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
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

use blinc_core::{Brush, Color, CornerRadius, Shadow, Transform};
use taffy::prelude::*;
use taffy::Overflow;

use crate::element::{
    ElementBounds, GlassMaterial, Material, MetallicMaterial, RenderLayer, RenderProps,
    WoodMaterial,
};
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
        self.layout_bounds.lock().unwrap().clone()
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
    pub(crate) border_color: Option<Color>,
    pub(crate) border_width: f32,
    pub(crate) render_layer: RenderLayer,
    pub(crate) material: Option<Material>,
    pub(crate) shadow: Option<Shadow>,
    pub(crate) transform: Option<Transform>,
    pub(crate) opacity: f32,
    pub(crate) event_handlers: crate::event_handler::EventHandlers,
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
            border_color: None,
            border_width: 0.0,
            render_layer: RenderLayer::default(),
            material: None,
            shadow: None,
            transform: None,
            opacity: 1.0,
            event_handlers: crate::event_handler::EventHandlers::new(),
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
    }

    /// Set the transform without consuming self
    #[inline]
    pub fn set_transform(&mut self, transform: Transform) {
        self.transform = Some(transform);
    }

    /// Set the shadow without consuming self
    #[inline]
    pub fn set_shadow(&mut self, shadow: Shadow) {
        self.shadow = Some(shadow);
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
        if other.border_radius != default.border_radius {
            self.border_radius = other.border_radius;
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
        if other.shadow.is_some() {
            self.shadow = other.shadow;
        }
        if other.transform.is_some() {
            self.transform = other.transform;
        }
        if other.opacity != default.opacity {
            self.opacity = other.opacity;
        }

        // Merge children - if other has children, replace ours
        if !other.children.is_empty() {
            self.children = other.children;
        }

        // Note: event_handlers are NOT merged - they're set on the base element
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

    // =========================================================================
    // Sizing (pixel values)
    // =========================================================================

    /// Set width in pixels
    pub fn w(mut self, px: f32) -> Self {
        self.style.size.width = Dimension::Length(px);
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

    /// Set width to fit content (shrink-wrap to children)
    ///
    /// This sets width to auto with flex_basis auto and prevents flex growing/shrinking,
    /// so the element will size exactly to fit its content.
    pub fn w_fit(mut self) -> Self {
        self.style.size.width = Dimension::Auto;
        self.style.flex_basis = Dimension::Auto;
        self.style.flex_grow = 0.0;
        self.style.flex_shrink = 0.0;
        self
    }

    /// Set height in pixels
    pub fn h(mut self, px: f32) -> Self {
        self.style.size.height = Dimension::Length(px);
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

    /// Set height to fit content (shrink-wrap to children)
    ///
    /// This sets height to auto and prevents flex growing/shrinking, so the element
    /// will size exactly to fit its content.
    pub fn h_fit(mut self) -> Self {
        self.style.size.height = Dimension::Auto;
        self.style.flex_basis = Dimension::Auto;
        self.style.flex_grow = 0.0;
        self.style.flex_shrink = 0.0;
        self
    }

    /// Set both width and height to fit content
    ///
    /// This makes the element shrink-wrap to its content in both dimensions.
    pub fn size_fit(mut self) -> Self {
        self.style.size.width = Dimension::Auto;
        self.style.size.height = Dimension::Auto;
        self.style.flex_basis = Dimension::Auto;
        self.style.flex_grow = 0.0;
        self.style.flex_shrink = 0.0;
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

    /// Set gap between children (in 4px units)
    /// gap(4) = 16px
    pub fn gap(mut self, units: f32) -> Self {
        let px = units * 4.0;
        self.style.gap = taffy::Size {
            width: LengthPercentage::Length(px),
            height: LengthPercentage::Length(px),
        };
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

    /// Set padding on all sides (in 4px units)
    /// p(4) = 16px padding
    pub fn p(mut self, units: f32) -> Self {
        let px = LengthPercentage::Length(units * 4.0);
        self.style.padding = Rect {
            left: px,
            right: px,
            top: px,
            bottom: px,
        };
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

    /// Set overflow to visible (default, content can extend beyond bounds)
    pub fn overflow_visible(mut self) -> Self {
        self.style.overflow.x = Overflow::Visible;
        self.style.overflow.y = Overflow::Visible;
        self
    }

    /// Set overflow to scroll (enable scrolling)
    ///
    /// Note: For custom scroll behavior with spring physics, use the `scroll()` element instead.
    pub fn overflow_scroll(mut self) -> Self {
        self.style.overflow.x = Overflow::Scroll;
        self.style.overflow.y = Overflow::Scroll;
        self
    }

    /// Set horizontal overflow only (X-axis)
    pub fn overflow_x(mut self, overflow: Overflow) -> Self {
        self.style.overflow.x = overflow;
        self
    }

    /// Set vertical overflow only (Y-axis)
    pub fn overflow_y(mut self, overflow: Overflow) -> Self {
        self.style.overflow.y = overflow;
        self
    }

    // =========================================================================
    // Visual Properties
    // =========================================================================

    /// Set background color
    pub fn bg(mut self, color: Color) -> Self {
        self.background = Some(Brush::Solid(color));
        self
    }

    /// Set background brush (for gradients)
    pub fn background(mut self, brush: impl Into<Brush>) -> Self {
        self.background = Some(brush.into());
        self
    }

    /// Set corner radius (all corners)
    pub fn rounded(mut self, radius: f32) -> Self {
        self.border_radius = CornerRadius::uniform(radius);
        self
    }

    /// Set corner radius with full pill shape (radius = min(w,h)/2)
    pub fn rounded_full(mut self) -> Self {
        // Use a large value; actual pill shape depends on element size
        self.border_radius = CornerRadius::uniform(9999.0);
        self
    }

    /// Set individual corner radii
    pub fn rounded_corners(mut self, tl: f32, tr: f32, br: f32, bl: f32) -> Self {
        self.border_radius = CornerRadius::new(tl, tr, br, bl);
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

    /// Set border color only
    pub fn border_color(mut self, color: Color) -> Self {
        self.border_color = Some(color);
        self
    }

    /// Set border width only
    pub fn border_width(mut self, width: f32) -> Self {
        self.border_width = width;
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

    /// Apply a drop shadow to this element
    pub fn shadow(mut self, shadow: Shadow) -> Self {
        self.shadow = Some(shadow);
        self
    }

    /// Apply a drop shadow with the given parameters
    pub fn shadow_params(self, offset_x: f32, offset_y: f32, blur: f32, color: Color) -> Self {
        self.shadow(Shadow::new(offset_x, offset_y, blur, color))
    }

    /// Apply a small drop shadow (2px offset, 4px blur)
    pub fn shadow_sm(self) -> Self {
        self.shadow(Shadow::new(0.0, 2.0, 4.0, Color::rgba(0.0, 0.0, 0.0, 0.1)))
    }

    /// Apply a medium drop shadow (4px offset, 8px blur)
    pub fn shadow_md(self) -> Self {
        self.shadow(Shadow::new(0.0, 4.0, 8.0, Color::rgba(0.0, 0.0, 0.0, 0.15)))
    }

    /// Apply a large drop shadow (8px offset, 16px blur)
    pub fn shadow_lg(self) -> Self {
        self.shadow(Shadow::new(0.0, 8.0, 16.0, Color::rgba(0.0, 0.0, 0.0, 0.2)))
    }

    /// Apply an extra large drop shadow (12px offset, 24px blur)
    pub fn shadow_xl(self) -> Self {
        self.shadow(Shadow::new(
            0.0,
            12.0,
            24.0,
            Color::rgba(0.0, 0.0, 0.0, 0.25),
        ))
    }

    // =========================================================================
    // Transform
    // =========================================================================

    /// Apply a transform to this element
    pub fn transform(mut self, transform: Transform) -> Self {
        self.transform = Some(transform);
        self
    }

    /// Translate this element by the given x and y offset
    pub fn translate(self, x: f32, y: f32) -> Self {
        self.transform(Transform::translate(x, y))
    }

    /// Scale this element uniformly
    pub fn scale(self, factor: f32) -> Self {
        self.transform(Transform::scale(factor, factor))
    }

    /// Scale this element with different x and y factors
    pub fn scale_xy(self, sx: f32, sy: f32) -> Self {
        self.transform(Transform::scale(sx, sy))
    }

    /// Rotate this element by the given angle in radians
    pub fn rotate(self, angle: f32) -> Self {
        self.transform(Transform::rotate(angle))
    }

    /// Rotate this element by the given angle in degrees
    pub fn rotate_deg(self, degrees: f32) -> Self {
        self.rotate(degrees * std::f32::consts::PI / 180.0)
    }

    // =========================================================================
    // Opacity
    // =========================================================================

    /// Set opacity (0.0 = transparent, 1.0 = opaque)
    pub fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity.clamp(0.0, 1.0);
        self
    }

    /// Fully opaque (opacity = 1.0)
    pub fn opaque(self) -> Self {
        self.opacity(1.0)
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
    // Children
    // =========================================================================

    /// Add a child element
    pub fn child(mut self, child: impl ElementBuilder + 'static) -> Self {
        self.children.push(Box::new(child));
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

    /// Register a drag handler (pointer movement while button is pressed)
    pub fn on_drag<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + 'static,
    {
        self.event_handlers
            .on(blinc_core::events::event_types::DRAG, handler);
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
    /// Text is positioned at the top of its bounding box (default for multi-line text)
    #[default]
    Top,
    /// Text is optically centered within its bounding box (for single-line centered text)
    /// Uses cap-height based centering for better visual appearance
    Center,
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
}

/// SVG render data extracted from element
#[derive(Clone)]
pub struct SvgRenderInfo {
    pub source: String,
    pub tint: Option<blinc_core::Color>,
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
}

impl ElementBuilder for Div {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        let node = tree.create_node(self.style.clone());

        // Build and add children
        for child in &self.children {
            let child_node = child.build(tree);
            tree.add_child(node, child_node);
        }

        node
    }

    fn render_props(&self) -> RenderProps {
        // Check if overflow is set to clip content (Clip or Scroll)
        // Overflow::Visible is the only mode that doesn't clip
        let clips_content = !matches!(self.style.overflow.x, Overflow::Visible)
            || !matches!(self.style.overflow.y, Overflow::Visible);

        RenderProps {
            background: self.background.clone(),
            border_radius: self.border_radius,
            border_color: self.border_color,
            border_width: self.border_width,
            layer: self.render_layer,
            material: self.material.clone(),
            node_id: None,
            shadow: self.shadow,
            transform: self.transform.clone(),
            opacity: self.opacity,
            clips_content,
            motion: None,
            is_stack_layer: false,
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
        let width = div_ref.with(|d| d.style.size.width.clone());
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
        let height = div_ref.with(|d| d.style.size.height.clone());
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
        let width = div_ref_clone.with(|d| d.style.size.width.clone());
        assert!(matches!(width, Some(Dimension::Length(100.0))));
    }
}
