//! Stack - Container for overlayed elements
//!
//! Stack is a specialized container where all children are positioned absolutely
//! and stack on top of each other. The last child in document order appears on top.

use blinc_core::{Brush, Color, Shadow, Transform};
use taffy::{LengthPercentageAuto, Overflow, Position, Rect, Style};

use crate::div::Div;
use crate::element::{Material, RenderLayer, RenderProps};
use crate::tree::{LayoutNodeId, LayoutTree};
use crate::ElementBuilder;

/// A stack container where all children are positioned absolutely and stack over each other.
///
/// Stack is a specialized Div that:
/// - Sets position: relative on itself to establish a positioning context
/// - Automatically sets position: absolute on all children
/// - Children stack from bottom to top in document order (last child is on top)
///
/// This is perfect for overlays, layered UIs, and any case where you need
/// elements to overlap without affecting each other's layout.
///
/// # Example
///
/// ```ignore
/// use blinc_layout::prelude::*;
///
/// // Three layers stacked on top of each other
/// stack()
///     .w(200.0)
///     .h(200.0)
///     .child(
///         div().w_full().h_full().bg(Color::RED) // Background layer
///     )
///     .child(
///         div().w(100.0).h(100.0).bg(Color::GREEN) // Middle layer at top-left
///     )
///     .child(
///         div()
///             .absolute()
///             .right(10.0)
///             .bottom(10.0)
///             .w(50.0)
///             .h(50.0)
///             .bg(Color::BLUE) // Top layer at bottom-right
///     )
/// ```
pub struct Stack {
    inner: Div,
}

impl Stack {
    /// Create a new stack container
    pub fn new() -> Self {
        let mut inner = Div::new();
        // Stack is a positioning context
        inner.style.position = Position::Relative;
        // Clip children to Stack bounds by default
        inner.style.overflow.x = Overflow::Clip;
        inner.style.overflow.y = Overflow::Clip;
        Self { inner }
    }

    /// Add a child element (will be absolutely positioned)
    pub fn child(mut self, child: impl ElementBuilder + 'static) -> Self {
        // Wrap child in an absolutely positioned container
        let wrapper = StackChild::new(Box::new(child));
        self.inner.children.push(Box::new(wrapper));
        self
    }

    /// Add multiple children (each will be absolutely positioned)
    pub fn children<I>(mut self, children: I) -> Self
    where
        I: IntoIterator,
        I::Item: ElementBuilder + 'static,
    {
        for child in children {
            let wrapper = StackChild::new(Box::new(child));
            self.inner.children.push(Box::new(wrapper));
        }
        self
    }
}

impl Default for Stack {
    fn default() -> Self {
        Self::new()
    }
}

// Delegate all Div methods to inner
impl Stack {
    // =========================================================================
    // Display & Flex
    // =========================================================================

    /// Set display to flex
    pub fn flex(mut self) -> Self {
        self.inner = self.inner.flex();
        self
    }

    /// Set display to block
    pub fn block(mut self) -> Self {
        self.inner = self.inner.block();
        self
    }

    /// Set display to grid
    pub fn grid(mut self) -> Self {
        self.inner = self.inner.grid();
        self
    }

    /// Set display to none (hidden)
    pub fn hidden(mut self) -> Self {
        self.inner = self.inner.hidden();
        self
    }

    /// Set flex direction to row
    pub fn flex_row(mut self) -> Self {
        self.inner = self.inner.flex_row();
        self
    }

    /// Set flex direction to column
    pub fn flex_col(mut self) -> Self {
        self.inner = self.inner.flex_col();
        self
    }

    /// Set flex direction to row-reverse
    pub fn flex_row_reverse(mut self) -> Self {
        self.inner = self.inner.flex_row_reverse();
        self
    }

    /// Set flex direction to column-reverse
    pub fn flex_col_reverse(mut self) -> Self {
        self.inner = self.inner.flex_col_reverse();
        self
    }

    /// Set flex-grow to 1
    pub fn flex_grow(mut self) -> Self {
        self.inner = self.inner.flex_grow();
        self
    }

    /// Set flex-shrink to 1
    pub fn flex_shrink(mut self) -> Self {
        self.inner = self.inner.flex_shrink();
        self
    }

    /// Set flex to auto (grow and shrink)
    pub fn flex_auto(mut self) -> Self {
        self.inner = self.inner.flex_auto();
        self
    }

    /// Enable flex wrap
    pub fn flex_wrap(mut self) -> Self {
        self.inner = self.inner.flex_wrap();
        self
    }

    // =========================================================================
    // Alignment
    // =========================================================================

    /// Align items to center
    pub fn items_center(mut self) -> Self {
        self.inner = self.inner.items_center();
        self
    }

    /// Align items to start
    pub fn items_start(mut self) -> Self {
        self.inner = self.inner.items_start();
        self
    }

    /// Align items to end
    pub fn items_end(mut self) -> Self {
        self.inner = self.inner.items_end();
        self
    }

    /// Stretch items
    pub fn items_stretch(mut self) -> Self {
        self.inner = self.inner.items_stretch();
        self
    }

    /// Align items to baseline
    pub fn items_baseline(mut self) -> Self {
        self.inner = self.inner.items_baseline();
        self
    }

    /// Justify content to start
    pub fn justify_start(mut self) -> Self {
        self.inner = self.inner.justify_start();
        self
    }

    /// Justify content to center
    pub fn justify_center(mut self) -> Self {
        self.inner = self.inner.justify_center();
        self
    }

    /// Justify content to end
    pub fn justify_end(mut self) -> Self {
        self.inner = self.inner.justify_end();
        self
    }

    /// Justify content with space between
    pub fn justify_between(mut self) -> Self {
        self.inner = self.inner.justify_between();
        self
    }

    /// Justify content with space around
    pub fn justify_around(mut self) -> Self {
        self.inner = self.inner.justify_around();
        self
    }

    /// Justify content with space evenly
    pub fn justify_evenly(mut self) -> Self {
        self.inner = self.inner.justify_evenly();
        self
    }

    // =========================================================================
    // Sizing
    // =========================================================================

    /// Set width in pixels
    pub fn w(mut self, px: f32) -> Self {
        self.inner = self.inner.w(px);
        self
    }

    /// Set width to 100%
    pub fn w_full(mut self) -> Self {
        self.inner = self.inner.w_full();
        self
    }

    /// Set width to auto
    pub fn w_auto(mut self) -> Self {
        self.inner = self.inner.w_auto();
        self
    }

    /// Set width to fit content
    pub fn w_fit(mut self) -> Self {
        self.inner = self.inner.w_fit();
        self
    }

    /// Set height in pixels
    pub fn h(mut self, px: f32) -> Self {
        self.inner = self.inner.h(px);
        self
    }

    /// Set height to 100%
    pub fn h_full(mut self) -> Self {
        self.inner = self.inner.h_full();
        self
    }

    /// Set height to auto
    pub fn h_auto(mut self) -> Self {
        self.inner = self.inner.h_auto();
        self
    }

    /// Set height to fit content
    pub fn h_fit(mut self) -> Self {
        self.inner = self.inner.h_fit();
        self
    }

    /// Set both width and height to fit content
    pub fn size_fit(mut self) -> Self {
        self.inner = self.inner.size_fit();
        self
    }

    /// Set both width and height in pixels
    pub fn size(mut self, w: f32, h: f32) -> Self {
        self.inner = self.inner.size(w, h);
        self
    }

    /// Set square size (width and height equal)
    pub fn square(mut self, size: f32) -> Self {
        self.inner = self.inner.square(size);
        self
    }

    /// Set minimum width
    pub fn min_w(mut self, px: f32) -> Self {
        self.inner = self.inner.min_w(px);
        self
    }

    /// Set minimum height
    pub fn min_h(mut self, px: f32) -> Self {
        self.inner = self.inner.min_h(px);
        self
    }

    /// Set maximum width
    pub fn max_w(mut self, px: f32) -> Self {
        self.inner = self.inner.max_w(px);
        self
    }

    /// Set maximum height
    pub fn max_h(mut self, px: f32) -> Self {
        self.inner = self.inner.max_h(px);
        self
    }

    // =========================================================================
    // Gap
    // =========================================================================

    /// Set gap between children (in 4px units)
    pub fn gap(mut self, units: f32) -> Self {
        self.inner = self.inner.gap(units);
        self
    }

    /// Set gap between children in pixels
    pub fn gap_px(mut self, px: f32) -> Self {
        self.inner = self.inner.gap_px(px);
        self
    }

    /// Set horizontal gap (in 4px units)
    pub fn gap_x(mut self, units: f32) -> Self {
        self.inner = self.inner.gap_x(units);
        self
    }

    /// Set vertical gap (in 4px units)
    pub fn gap_y(mut self, units: f32) -> Self {
        self.inner = self.inner.gap_y(units);
        self
    }

    // =========================================================================
    // Padding
    // =========================================================================

    /// Set padding on all sides (in 4px units)
    pub fn p(mut self, units: f32) -> Self {
        self.inner = self.inner.p(units);
        self
    }

    /// Set padding in pixels
    pub fn p_px(mut self, px: f32) -> Self {
        self.inner = self.inner.p_px(px);
        self
    }

    /// Set horizontal padding (in 4px units)
    pub fn px(mut self, units: f32) -> Self {
        self.inner = self.inner.px(units);
        self
    }

    /// Set vertical padding (in 4px units)
    pub fn py(mut self, units: f32) -> Self {
        self.inner = self.inner.py(units);
        self
    }

    /// Set left padding (in 4px units)
    pub fn pl(mut self, units: f32) -> Self {
        self.inner = self.inner.pl(units);
        self
    }

    /// Set right padding (in 4px units)
    pub fn pr(mut self, units: f32) -> Self {
        self.inner = self.inner.pr(units);
        self
    }

    /// Set top padding (in 4px units)
    pub fn pt(mut self, units: f32) -> Self {
        self.inner = self.inner.pt(units);
        self
    }

    /// Set bottom padding (in 4px units)
    pub fn pb(mut self, units: f32) -> Self {
        self.inner = self.inner.pb(units);
        self
    }

    /// Set horizontal padding in pixels
    pub fn padding_x_px(mut self, pixels: f32) -> Self {
        self.inner = self.inner.padding_x_px(pixels);
        self
    }

    /// Set vertical padding in pixels
    pub fn padding_y_px(mut self, pixels: f32) -> Self {
        self.inner = self.inner.padding_y_px(pixels);
        self
    }

    // =========================================================================
    // Margin
    // =========================================================================

    /// Set margin on all sides (in 4px units)
    pub fn m(mut self, units: f32) -> Self {
        self.inner = self.inner.m(units);
        self
    }

    /// Set margin in pixels
    pub fn m_px(mut self, px: f32) -> Self {
        self.inner = self.inner.m_px(px);
        self
    }

    /// Set horizontal margin (in 4px units)
    pub fn mx(mut self, units: f32) -> Self {
        self.inner = self.inner.mx(units);
        self
    }

    /// Set vertical margin (in 4px units)
    pub fn my(mut self, units: f32) -> Self {
        self.inner = self.inner.my(units);
        self
    }

    /// Set horizontal margin to auto (centering)
    pub fn mx_auto(mut self) -> Self {
        self.inner = self.inner.mx_auto();
        self
    }

    /// Set left margin (in 4px units)
    pub fn ml(mut self, units: f32) -> Self {
        self.inner = self.inner.ml(units);
        self
    }

    /// Set right margin (in 4px units)
    pub fn mr(mut self, units: f32) -> Self {
        self.inner = self.inner.mr(units);
        self
    }

    /// Set top margin (in 4px units)
    pub fn mt(mut self, units: f32) -> Self {
        self.inner = self.inner.mt(units);
        self
    }

    /// Set bottom margin (in 4px units)
    pub fn mb(mut self, units: f32) -> Self {
        self.inner = self.inner.mb(units);
        self
    }

    // =========================================================================
    // Positioning (for the Stack container itself)
    // =========================================================================

    /// Set position to absolute
    pub fn absolute(mut self) -> Self {
        self.inner = self.inner.absolute();
        self
    }

    /// Set inset (all sides)
    pub fn inset(mut self, px: f32) -> Self {
        self.inner = self.inner.inset(px);
        self
    }

    /// Set top position
    pub fn top(mut self, px: f32) -> Self {
        self.inner = self.inner.top(px);
        self
    }

    /// Set bottom position
    pub fn bottom(mut self, px: f32) -> Self {
        self.inner = self.inner.bottom(px);
        self
    }

    /// Set left position
    pub fn left(mut self, px: f32) -> Self {
        self.inner = self.inner.left(px);
        self
    }

    /// Set right position
    pub fn right(mut self, px: f32) -> Self {
        self.inner = self.inner.right(px);
        self
    }

    // =========================================================================
    // Overflow
    // =========================================================================

    /// Set overflow to hidden (clip content)
    pub fn overflow_clip(mut self) -> Self {
        self.inner = self.inner.overflow_clip();
        self
    }

    /// Set overflow to visible
    pub fn overflow_visible(mut self) -> Self {
        self.inner = self.inner.overflow_visible();
        self
    }

    /// Set overflow to scroll
    pub fn overflow_scroll(mut self) -> Self {
        self.inner = self.inner.overflow_scroll();
        self
    }

    /// Set horizontal overflow
    pub fn overflow_x(mut self, overflow: Overflow) -> Self {
        self.inner = self.inner.overflow_x(overflow);
        self
    }

    /// Set vertical overflow
    pub fn overflow_y(mut self, overflow: Overflow) -> Self {
        self.inner = self.inner.overflow_y(overflow);
        self
    }

    // =========================================================================
    // Visual Properties
    // =========================================================================

    /// Set background color
    pub fn bg(mut self, color: Color) -> Self {
        self.inner = self.inner.bg(color);
        self
    }

    /// Set background brush (for gradients)
    pub fn background(mut self, brush: impl Into<Brush>) -> Self {
        self.inner = self.inner.background(brush);
        self
    }

    /// Set corner radius (all corners)
    pub fn rounded(mut self, radius: f32) -> Self {
        self.inner = self.inner.rounded(radius);
        self
    }

    /// Set corner radius with full pill shape
    pub fn rounded_full(mut self) -> Self {
        self.inner = self.inner.rounded_full();
        self
    }

    /// Set individual corner radii
    pub fn rounded_corners(mut self, tl: f32, tr: f32, br: f32, bl: f32) -> Self {
        self.inner = self.inner.rounded_corners(tl, tr, br, bl);
        self
    }

    // =========================================================================
    // Border
    // =========================================================================

    /// Set border with color and width
    pub fn border(mut self, width: f32, color: Color) -> Self {
        self.inner = self.inner.border(width, color);
        self
    }

    /// Set border color only
    pub fn border_color(mut self, color: Color) -> Self {
        self.inner = self.inner.border_color(color);
        self
    }

    /// Set border width only
    pub fn border_width(mut self, width: f32) -> Self {
        self.inner = self.inner.border_width(width);
        self
    }

    /// Set render layer
    pub fn layer(mut self, layer: RenderLayer) -> Self {
        self.inner = self.inner.layer(layer);
        self
    }

    /// Render in foreground layer
    pub fn foreground(self) -> Self {
        Self {
            inner: self.inner.foreground(),
        }
    }

    // =========================================================================
    // Material System
    // =========================================================================

    /// Apply a material
    pub fn material(mut self, material: Material) -> Self {
        self.inner = self.inner.material(material);
        self
    }

    /// Apply an effect material
    pub fn effect(self, effect: impl Into<Material>) -> Self {
        Self {
            inner: self.inner.effect(effect),
        }
    }

    /// Apply glass material with default settings
    pub fn glass(self) -> Self {
        Self {
            inner: self.inner.glass(),
        }
    }

    /// Apply metallic material
    pub fn metallic(self) -> Self {
        Self {
            inner: self.inner.metallic(),
        }
    }

    /// Apply chrome material
    pub fn chrome(self) -> Self {
        Self {
            inner: self.inner.chrome(),
        }
    }

    /// Apply gold material
    pub fn gold(self) -> Self {
        Self {
            inner: self.inner.gold(),
        }
    }

    /// Apply wood material
    pub fn wood(self) -> Self {
        Self {
            inner: self.inner.wood(),
        }
    }

    // =========================================================================
    // Shadow
    // =========================================================================

    /// Apply a drop shadow
    pub fn shadow(mut self, shadow: Shadow) -> Self {
        self.inner = self.inner.shadow(shadow);
        self
    }

    /// Apply a shadow with custom parameters
    pub fn shadow_params(self, offset_x: f32, offset_y: f32, blur: f32, color: Color) -> Self {
        Self {
            inner: self.inner.shadow_params(offset_x, offset_y, blur, color),
        }
    }

    /// Apply a small drop shadow
    pub fn shadow_sm(self) -> Self {
        Self {
            inner: self.inner.shadow_sm(),
        }
    }

    /// Apply a medium drop shadow
    pub fn shadow_md(self) -> Self {
        Self {
            inner: self.inner.shadow_md(),
        }
    }

    /// Apply a large drop shadow
    pub fn shadow_lg(self) -> Self {
        Self {
            inner: self.inner.shadow_lg(),
        }
    }

    /// Apply an extra large drop shadow
    pub fn shadow_xl(self) -> Self {
        Self {
            inner: self.inner.shadow_xl(),
        }
    }

    // =========================================================================
    // Transform
    // =========================================================================

    /// Apply a transform
    pub fn transform(mut self, transform: Transform) -> Self {
        self.inner = self.inner.transform(transform);
        self
    }

    /// Apply a translation transform
    pub fn translate(self, x: f32, y: f32) -> Self {
        Self {
            inner: self.inner.translate(x, y),
        }
    }

    /// Apply a uniform scale transform
    pub fn scale(self, factor: f32) -> Self {
        Self {
            inner: self.inner.scale(factor),
        }
    }

    /// Apply a non-uniform scale transform
    pub fn scale_xy(self, sx: f32, sy: f32) -> Self {
        Self {
            inner: self.inner.scale_xy(sx, sy),
        }
    }

    /// Apply a rotation transform (radians)
    pub fn rotate(self, angle: f32) -> Self {
        Self {
            inner: self.inner.rotate(angle),
        }
    }

    /// Apply a rotation transform (degrees)
    pub fn rotate_deg(self, degrees: f32) -> Self {
        Self {
            inner: self.inner.rotate_deg(degrees),
        }
    }

    // =========================================================================
    // Opacity
    // =========================================================================

    /// Set opacity (0.0 = transparent, 1.0 = opaque)
    pub fn opacity(mut self, opacity: f32) -> Self {
        self.inner = self.inner.opacity(opacity);
        self
    }

    /// Set fully opaque
    pub fn opaque(self) -> Self {
        Self {
            inner: self.inner.opaque(),
        }
    }

    /// Set translucent (50% opacity)
    pub fn translucent(self) -> Self {
        Self {
            inner: self.inner.translucent(),
        }
    }

    /// Set invisible (0% opacity)
    pub fn invisible(self) -> Self {
        Self {
            inner: self.inner.invisible(),
        }
    }

    // =========================================================================
    // Event Handlers
    // =========================================================================

    /// Register a click handler
    pub fn on_click<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = self.inner.on_click(handler);
        self
    }

    /// Register a mouse down handler
    pub fn on_mouse_down<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = self.inner.on_mouse_down(handler);
        self
    }

    /// Register a mouse up handler
    pub fn on_mouse_up<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = self.inner.on_mouse_up(handler);
        self
    }

    /// Register a mouse move handler
    pub fn on_mouse_move<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = self.inner.on_mouse_move(handler);
        self
    }

    /// Register a drag handler
    pub fn on_drag<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = self.inner.on_drag(handler);
        self
    }

    /// Register a scroll handler
    pub fn on_scroll<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = self.inner.on_scroll(handler);
        self
    }

    /// Register a hover enter handler
    pub fn on_hover_enter<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = self.inner.on_hover_enter(handler);
        self
    }

    /// Register a hover leave handler
    pub fn on_hover_leave<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = self.inner.on_hover_leave(handler);
        self
    }

    /// Register a focus handler
    pub fn on_focus<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = self.inner.on_focus(handler);
        self
    }

    /// Register a blur handler
    pub fn on_blur<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = self.inner.on_blur(handler);
        self
    }

    // =========================================================================
    // Cursor
    // =========================================================================

    /// Set cursor style
    pub fn cursor(mut self, cursor: crate::element::CursorStyle) -> Self {
        self.inner = self.inner.cursor(cursor);
        self
    }

    /// Set cursor to pointer (hand)
    pub fn cursor_pointer(self) -> Self {
        self.cursor(crate::element::CursorStyle::Pointer)
    }

    /// Set cursor to text selection
    pub fn cursor_text(self) -> Self {
        self.cursor(crate::element::CursorStyle::Text)
    }

    /// Set cursor to move
    pub fn cursor_move(self) -> Self {
        self.cursor(crate::element::CursorStyle::Move)
    }

    /// Set cursor to grab
    pub fn cursor_grab(self) -> Self {
        self.cursor(crate::element::CursorStyle::Grab)
    }

    /// Set cursor to grabbing
    pub fn cursor_grabbing(self) -> Self {
        self.cursor(crate::element::CursorStyle::Grabbing)
    }

    /// Set cursor to not allowed
    pub fn cursor_not_allowed(self) -> Self {
        self.cursor(crate::element::CursorStyle::NotAllowed)
    }
}

impl ElementBuilder for Stack {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        self.inner.build(tree)
    }

    fn render_props(&self) -> RenderProps {
        self.inner.render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.inner.children_builders()
    }

    fn event_handlers(&self) -> Option<&crate::event_handler::EventHandlers> {
        // Delegate to inner Div's event_handlers
        if self.inner.event_handlers.is_empty() {
            None
        } else {
            Some(&self.inner.event_handlers)
        }
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        self.inner.layout_style()
    }
}

/// Internal wrapper that makes a child absolutely positioned
struct StackChild {
    /// The actual child element, stored in a Vec for children_builders() to return a slice
    children: Vec<Box<dyn ElementBuilder>>,
    /// The style for absolute positioning (stored for layout_style())
    style: Style,
}

impl StackChild {
    fn new(child: Box<dyn ElementBuilder>) -> Self {
        let mut style = Style::default();
        style.position = Position::Absolute;
        // Set all inset values to 0 to fill the entire containing block
        // This stretches the wrapper to match the Stack's size
        style.inset = Rect {
            left: LengthPercentageAuto::Length(0.0),
            right: LengthPercentageAuto::Length(0.0),
            top: LengthPercentageAuto::Length(0.0),
            bottom: LengthPercentageAuto::Length(0.0),
        };
        // Clip children to this layer's bounds - important for z-ordering
        // Each Stack layer clips its own content so text doesn't bleed through
        style.overflow.x = Overflow::Clip;
        style.overflow.y = Overflow::Clip;

        Self {
            children: vec![child],
            style,
        }
    }
}

impl ElementBuilder for StackChild {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        // Create a wrapper node with absolute positioning that fills the entire Stack
        // Use the stored style so layout_style() returns the same style
        let wrapper = tree.create_node(self.style.clone());

        // Build the child and add it to wrapper
        if let Some(child) = self.children.first() {
            let child_node = child.build(tree);
            tree.add_child(wrapper, child_node);
        }

        wrapper
    }

    fn render_props(&self) -> RenderProps {
        // Wrapper clips its children for proper z-ordering in Stack
        // is_stack_layer causes z_layer to increment when entering this node
        // pointer_events_none allows clicks to pass through to siblings when
        // children don't capture the click (important for overlay backdrops)
        RenderProps {
            clips_content: true,
            is_stack_layer: true,
            pointer_events_none: true,
            ..RenderProps::default()
        }
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        // Return the wrapped child so render traversal can continue
        &self.children
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        // Return the stored style so incremental updates can update the taffy node
        Some(&self.style)
    }
}

/// Create a stack container where children overlay each other
///
/// Stack is a positioning container where all children are absolutely positioned
/// and stack on top of each other. The last child in document order appears on top.
///
/// # Example
///
/// ```ignore
/// use blinc_layout::prelude::*;
///
/// stack()
///     .w(200.0).h(200.0)
///     .child(div().w_full().h_full().bg(Color::RED))  // Bottom layer
///     .child(div().w(100.0).h(100.0).bg(Color::GREEN)) // Top layer
/// ```
pub fn stack() -> Stack {
    Stack::new()
}
