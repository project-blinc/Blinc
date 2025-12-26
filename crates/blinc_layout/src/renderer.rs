//! RenderTree bridge connecting layout to rendering
//!
//! This module provides the bridge between Taffy layout computation
//! and the DrawContext rendering API.

use std::any::Any;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use indexmap::IndexMap;

use blinc_core::{Brush, ClipShape, Color, CornerRadius, DrawContext, GlassStyle, Rect, Transform};
use taffy::prelude::*;

use crate::canvas::CanvasData;
use crate::div::{ElementBuilder, ElementTypeId};
use crate::element::{ElementBounds, GlassMaterial, Material, RenderLayer, RenderProps};
use crate::tree::{LayoutNodeId, LayoutTree};

/// A computed glass panel ready for GPU rendering
///
/// This contains all the information needed to render a glass effect,
/// with bounds computed from the layout system.
///
/// # Deprecated
/// Use `Brush::Glass` instead. Glass is now rendered as part of the
/// normal render pipeline - just use `fill_rect` with a glass brush.
#[deprecated(
    since = "0.2.0",
    note = "Use Brush::Glass instead. Glass is now integrated into the normal render pipeline."
)]
#[derive(Clone, Debug)]
pub struct GlassPanel {
    /// Absolute bounds (x, y, width, height)
    pub bounds: ElementBounds,
    /// Corner radii
    pub corner_radius: CornerRadius,
    /// Glass material properties
    pub material: GlassMaterial,
    /// The layout node this panel belongs to
    pub node_id: LayoutNodeId,
}

/// Stores an element's type for rendering
#[derive(Clone)]
pub enum ElementType {
    /// A div/container element
    Div,
    /// A text element with content
    Text(TextData),
    /// An SVG element
    Svg(SvgData),
    /// An image element
    Image(ImageData),
    /// A canvas element with custom render callback
    Canvas(CanvasData),
}

/// Text data for rendering
#[derive(Clone)]
pub struct TextData {
    pub content: String,
    pub font_size: f32,
    pub color: [f32; 4],
    pub align: crate::div::TextAlign,
    pub weight: crate::div::FontWeight,
    pub v_align: crate::div::TextVerticalAlign,
}

/// SVG data for rendering
#[derive(Clone)]
pub struct SvgData {
    pub source: String,
    pub tint: Option<Color>,
}

/// Image data for rendering
#[derive(Clone)]
pub struct ImageData {
    /// Image source (file path, URL, or base64 data)
    pub source: String,
    /// Object-fit mode (0=cover, 1=contain, 2=fill, 3=scale-down, 4=none)
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

/// Node data for rendering
#[derive(Clone)]
pub struct RenderNode {
    /// Render properties
    pub props: RenderProps,
    /// Element type
    pub element_type: ElementType,
}

/// Trait for rendering layout trees with text, SVG, and glass support
///
/// Implement this trait to provide custom rendering for your platform.
/// The renderer handles:
/// - Background/foreground DrawContext separation for glass effects
/// - Text rendering at layout-computed positions
/// - SVG rendering at layout-computed positions
pub trait LayoutRenderer {
    /// Get the background DrawContext (for elements behind glass)
    fn background(&mut self) -> &mut dyn DrawContext;

    /// Get the foreground DrawContext (for elements on top of glass)
    fn foreground(&mut self) -> &mut dyn DrawContext;

    /// Render text to the foreground layer at absolute position
    ///
    /// Called for text elements that are children of glass elements.
    /// Position is absolute (after applying all parent transforms).
    fn render_text_foreground(
        &mut self,
        content: &str,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        font_size: f32,
        color: [f32; 4],
        align: crate::div::TextAlign,
        weight: crate::div::FontWeight,
    );

    /// Render text to the background layer at absolute position
    fn render_text_background(
        &mut self,
        content: &str,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        font_size: f32,
        color: [f32; 4],
        align: crate::div::TextAlign,
        weight: crate::div::FontWeight,
    );

    /// Render an SVG to the foreground layer at absolute position
    fn render_svg_foreground(
        &mut self,
        source: &str,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        tint: Option<Color>,
    );

    /// Render an SVG to the background layer at absolute position
    fn render_svg_background(
        &mut self,
        source: &str,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        tint: Option<Color>,
    );
}

/// Type-erased node state storage
pub type NodeStateStorage = Arc<Mutex<dyn Any + Send>>;

/// RenderTree - bridges layout computation and rendering
pub struct RenderTree {
    /// The underlying layout tree
    pub layout_tree: LayoutTree,
    /// Render data for each node (ordered by insertion/tree order)
    render_nodes: IndexMap<LayoutNodeId, RenderNode>,
    /// Root node ID
    root: Option<LayoutNodeId>,
    /// Event handlers registry for dispatching events
    handler_registry: crate::event_handler::HandlerRegistry,
    /// Dirty tracker for incremental rebuilds
    dirty_tracker: crate::interactive::DirtyTracker,
    /// Per-node state storage (survives across rebuilds if tree is reused)
    node_states: HashMap<LayoutNodeId, NodeStateStorage>,
    /// Scroll offsets for scroll containers (node_id -> (offset_x, offset_y))
    scroll_offsets: HashMap<LayoutNodeId, (f32, f32)>,
    /// Scroll physics for scroll containers (keyed by node_id)
    scroll_physics: HashMap<LayoutNodeId, crate::scroll::SharedScrollPhysics>,
}

impl Default for RenderTree {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderTree {
    /// Create a new empty render tree
    pub fn new() -> Self {
        Self {
            layout_tree: LayoutTree::new(),
            render_nodes: IndexMap::new(),
            root: None,
            handler_registry: crate::event_handler::HandlerRegistry::new(),
            dirty_tracker: crate::interactive::DirtyTracker::new(),
            node_states: HashMap::new(),
            scroll_offsets: HashMap::new(),
            scroll_physics: HashMap::new(),
        }
    }

    /// Build a render tree from an element builder
    pub fn from_element<E: ElementBuilder>(element: &E) -> Self {
        let mut tree = Self::new();
        tree.root = Some(tree.build_element(element));
        tree
    }

    /// Recursively build elements into the tree
    ///
    /// This builds the layout tree first (via element.build()), then walks the
    /// element tree again to collect render properties for each node.
    fn build_element<E: ElementBuilder>(&mut self, element: &E) -> LayoutNodeId {
        // First, build the entire layout tree (this creates all nodes and parent-child relationships)
        let root_id = element.build(&mut self.layout_tree);

        // Now walk the element tree to collect render props for each node
        self.collect_render_props(element, root_id);

        root_id
    }

    /// Collect render properties from an element and its children
    fn collect_render_props<E: ElementBuilder>(&mut self, element: &E, node_id: LayoutNodeId) {
        let mut props = element.render_props();
        props.node_id = Some(node_id);

        // Determine element type using the trait methods
        let element_type = Self::determine_element_type(element);

        self.render_nodes.insert(
            node_id,
            RenderNode {
                props,
                element_type,
            },
        );

        // Register event handlers if present
        if let Some(handlers) = element.event_handlers() {
            self.handler_registry.register(node_id, handlers.clone());
        }

        // Store scroll physics if this is a scroll element
        if let Some(physics) = element.scroll_physics() {
            self.scroll_physics.insert(node_id, physics);
        }

        // Get child node IDs from the layout tree
        let child_node_ids = self.layout_tree.children(node_id);
        let child_builders = element.children_builders();

        // Match children by index (they were built in order)
        for (child_builder, &child_node_id) in child_builders.iter().zip(child_node_ids.iter()) {
            self.collect_render_props_boxed(child_builder.as_ref(), child_node_id);
        }
    }

    /// Collect render props from a boxed element builder
    fn collect_render_props_boxed(&mut self, element: &dyn ElementBuilder, node_id: LayoutNodeId) {
        let mut props = element.render_props();
        props.node_id = Some(node_id);

        // Use the element_type_id to determine type
        let element_type = match element.element_type_id() {
            ElementTypeId::Text => {
                if let Some(info) = element.text_render_info() {
                    ElementType::Text(TextData {
                        content: info.content,
                        font_size: info.font_size,
                        color: info.color,
                        align: info.align,
                        weight: info.weight,
                        v_align: info.v_align,
                    })
                } else {
                    ElementType::Div
                }
            }
            ElementTypeId::Svg => {
                if let Some(info) = element.svg_render_info() {
                    ElementType::Svg(SvgData {
                        source: info.source,
                        tint: info.tint,
                    })
                } else {
                    ElementType::Div
                }
            }
            ElementTypeId::Image => {
                if let Some(info) = element.image_render_info() {
                    ElementType::Image(ImageData {
                        source: info.source,
                        object_fit: info.object_fit,
                        object_position: info.object_position,
                        opacity: info.opacity,
                        border_radius: info.border_radius,
                        tint: info.tint,
                        filter: info.filter,
                    })
                } else {
                    ElementType::Div
                }
            }
            ElementTypeId::Canvas => {
                ElementType::Canvas(CanvasData {
                    render_fn: element.canvas_render_info(),
                })
            }
            ElementTypeId::Div => ElementType::Div,
        };

        self.render_nodes.insert(
            node_id,
            RenderNode {
                props,
                element_type,
            },
        );

        // Register event handlers if present
        if let Some(handlers) = element.event_handlers() {
            self.handler_registry.register(node_id, handlers.clone());
        }

        // Store scroll physics if this is a scroll element
        if let Some(physics) = element.scroll_physics() {
            self.scroll_physics.insert(node_id, physics);
        }

        // Get child node IDs from the layout tree
        let child_node_ids = self.layout_tree.children(node_id);
        let child_builders = element.children_builders();

        // Match children by index (they were built in order)
        for (child_builder, &child_node_id) in child_builders.iter().zip(child_node_ids.iter()) {
            self.collect_render_props_boxed(child_builder.as_ref(), child_node_id);
        }
    }

    /// Determine element type from an element builder
    fn determine_element_type<E: ElementBuilder>(element: &E) -> ElementType {
        match element.element_type_id() {
            ElementTypeId::Text => {
                if let Some(info) = element.text_render_info() {
                    ElementType::Text(TextData {
                        content: info.content,
                        font_size: info.font_size,
                        color: info.color,
                        align: info.align,
                        weight: info.weight,
                        v_align: info.v_align,
                    })
                } else {
                    ElementType::Div
                }
            }
            ElementTypeId::Svg => {
                if let Some(info) = element.svg_render_info() {
                    ElementType::Svg(SvgData {
                        source: info.source,
                        tint: info.tint,
                    })
                } else {
                    ElementType::Div
                }
            }
            ElementTypeId::Image => {
                if let Some(info) = element.image_render_info() {
                    ElementType::Image(ImageData {
                        source: info.source,
                        object_fit: info.object_fit,
                        object_position: info.object_position,
                        opacity: info.opacity,
                        border_radius: info.border_radius,
                        tint: info.tint,
                        filter: info.filter,
                    })
                } else {
                    ElementType::Div
                }
            }
            ElementTypeId::Canvas => {
                ElementType::Canvas(CanvasData {
                    render_fn: element.canvas_render_info(),
                })
            }
            ElementTypeId::Div => ElementType::Div,
        }
    }

    /// Get the root node ID
    pub fn root(&self) -> Option<LayoutNodeId> {
        self.root
    }

    /// Compute layout for the given viewport size
    pub fn compute_layout(&mut self, width: f32, height: f32) {
        if let Some(root) = self.root {
            self.layout_tree.compute_layout(
                root,
                Size {
                    width: AvailableSpace::Definite(width),
                    height: AvailableSpace::Definite(height),
                },
            );

            // Update scroll physics with computed content dimensions
            self.update_scroll_content_dimensions();
        }
    }

    /// Update scroll physics with content dimensions from layout
    fn update_scroll_content_dimensions(&mut self) {
        // Collect node_ids to avoid borrowing issues
        let node_ids: Vec<_> = self.scroll_physics.keys().copied().collect();

        for node_id in node_ids {
            // Get viewport bounds (the scroll container's own size)
            let bounds = self.layout_tree.get_bounds(node_id, (0.0, 0.0));
            let viewport_width = bounds.map(|b| b.width).unwrap_or(0.0);
            let viewport_height = bounds.map(|b| b.height).unwrap_or(0.0);

            // Get content size from Taffy's content_size (enabled via feature)
            // This tells us the total size of all content that may overflow
            let (content_width, content_height) = self.layout_tree
                .get_content_size(node_id)
                .unwrap_or((viewport_width, viewport_height));

            // Update physics with dimensions
            if let Some(physics) = self.scroll_physics.get(&node_id) {
                if let Ok(mut p) = physics.lock() {
                    p.viewport_width = viewport_width;
                    p.viewport_height = viewport_height;
                    p.content_width = content_width;
                    p.content_height = content_height;

                    tracing::trace!(
                        "Scroll physics updated: viewport=({:.0}, {:.0}) content=({:.0}, {:.0}) max_offset_y={:.0}",
                        viewport_width, viewport_height, content_width, content_height, p.max_offset_y()
                    );
                }
            }
        }
    }

    /// Get the layout tree for inspection
    pub fn layout(&self) -> &LayoutTree {
        &self.layout_tree
    }

    /// Get the event handler registry
    pub fn handler_registry(&self) -> &crate::event_handler::HandlerRegistry {
        &self.handler_registry
    }

    /// Get the event handler registry mutably
    pub fn handler_registry_mut(&mut self) -> &mut crate::event_handler::HandlerRegistry {
        &mut self.handler_registry
    }

    /// Dispatch an event to a node's handlers
    ///
    /// This automatically marks the tree as dirty after dispatching,
    /// signaling that the UI needs to be rebuilt.
    pub fn dispatch_event(
        &mut self,
        node_id: LayoutNodeId,
        event_type: blinc_core::events::EventType,
        mouse_x: f32,
        mouse_y: f32,
    ) {
        let ctx = crate::event_handler::EventContext::new(event_type, node_id)
            .with_mouse_pos(mouse_x, mouse_y);

        // Check if this node has handlers for this event type
        if self.handler_registry.has_handler(node_id, event_type) {
            self.handler_registry.dispatch(&ctx);
            // Don't auto-mark dirty - handlers update values in place
        }
    }

    /// Dispatch an event with local coordinates
    ///
    /// Dispatches an event to a node's handler.
    ///
    /// Note: This does NOT automatically mark the tree as dirty.
    /// Handlers that need a rebuild should use EventContext::request_rebuild().
    pub fn dispatch_event_with_local(
        &mut self,
        node_id: LayoutNodeId,
        event_type: blinc_core::events::EventType,
        mouse_x: f32,
        mouse_y: f32,
        local_x: f32,
        local_y: f32,
    ) {
        let ctx = crate::event_handler::EventContext::new(event_type, node_id)
            .with_mouse_pos(mouse_x, mouse_y)
            .with_local_pos(local_x, local_y);

        if self.handler_registry.has_handler(node_id, event_type) {
            self.handler_registry.dispatch(&ctx);
            // Don't auto-mark dirty - handlers update values in place
            // Rebuild only when explicitly requested via State::set() or structural changes
        }
    }

    /// Dispatch a text input event with character data
    ///
    /// This is used for character input in text fields.
    pub fn dispatch_text_input_event(
        &mut self,
        node_id: LayoutNodeId,
        key_char: char,
        shift: bool,
        ctrl: bool,
        alt: bool,
        meta: bool,
    ) {
        let ctx = crate::event_handler::EventContext::new(
            blinc_core::events::event_types::TEXT_INPUT,
            node_id,
        )
        .with_key_char(key_char)
        .with_modifiers(shift, ctrl, alt, meta);

        if self
            .handler_registry
            .has_handler(node_id, blinc_core::events::event_types::TEXT_INPUT)
        {
            self.handler_registry.dispatch(&ctx);
            // Don't auto-mark dirty - text input handler updates values in place
            // and calls State::set() which marks dirty if structural change needed
        }
    }

    /// Dispatch a text input event with bubbling through ancestors
    ///
    /// This is used for character input in text fields. The event bubbles up
    /// through ancestors until a handler is found.
    pub fn dispatch_text_input_event_bubbling(
        &mut self,
        ancestors: &[LayoutNodeId],
        key_char: char,
        shift: bool,
        ctrl: bool,
        alt: bool,
        meta: bool,
    ) {
        let event_type = blinc_core::events::event_types::TEXT_INPUT;

        // Try each node in reverse order (leaf to root) until we find a handler
        for &node_id in ancestors.iter().rev() {
            if self.handler_registry.has_handler(node_id, event_type) {
                let ctx = crate::event_handler::EventContext::new(event_type, node_id)
                    .with_key_char(key_char)
                    .with_modifiers(shift, ctrl, alt, meta);
                self.handler_registry.dispatch(&ctx);
                // Don't auto-mark dirty - handler updates state in place
                return; // Stop after first handler found
            }
        }
    }

    /// Dispatch a key event with key code and modifiers
    ///
    /// This is used for KEY_DOWN and KEY_UP events.
    pub fn dispatch_key_event(
        &mut self,
        node_id: LayoutNodeId,
        event_type: blinc_core::events::EventType,
        key_code: u32,
        shift: bool,
        ctrl: bool,
        alt: bool,
        meta: bool,
    ) {
        let ctx = crate::event_handler::EventContext::new(event_type, node_id)
            .with_key_code(key_code)
            .with_modifiers(shift, ctrl, alt, meta);

        if self.handler_registry.has_handler(node_id, event_type) {
            self.handler_registry.dispatch(&ctx);
            // Don't auto-mark dirty - handler updates state in place
        }
    }

    /// Dispatch a key event with bubbling through ancestors
    ///
    /// This is used for KEY_DOWN and KEY_UP events. The event bubbles up
    /// through ancestors until a handler is found.
    pub fn dispatch_key_event_bubbling(
        &mut self,
        ancestors: &[LayoutNodeId],
        event_type: blinc_core::events::EventType,
        key_code: u32,
        shift: bool,
        ctrl: bool,
        alt: bool,
        meta: bool,
    ) {
        // Try each node in reverse order (leaf to root) until we find a handler
        for &node_id in ancestors.iter().rev() {
            if self.handler_registry.has_handler(node_id, event_type) {
                let ctx = crate::event_handler::EventContext::new(event_type, node_id)
                    .with_key_code(key_code)
                    .with_modifiers(shift, ctrl, alt, meta);
                self.handler_registry.dispatch(&ctx);
                // Don't auto-mark dirty - handler updates state in place
                return; // Stop after first handler found
            }
        }
    }

    /// Dispatch a scroll event with scroll delta
    ///
    /// Updates the scroll offset for this node and dispatches to handlers.
    /// Does NOT mark the tree as dirty since scroll only affects rendering,
    /// not the UI tree structure.
    pub fn dispatch_scroll_event(
        &mut self,
        node_id: LayoutNodeId,
        mouse_x: f32,
        mouse_y: f32,
        scroll_delta_x: f32,
        scroll_delta_y: f32,
    ) {
        let ctx = crate::event_handler::EventContext::new(
            blinc_core::events::event_types::SCROLL,
            node_id,
        )
        .with_mouse_pos(mouse_x, mouse_y)
        .with_scroll_delta(scroll_delta_x, scroll_delta_y);

        let has_handler = self
            .handler_registry
            .has_handler(node_id, blinc_core::events::event_types::SCROLL);

        if has_handler {
            // Dispatch to handlers - the Scroll element's internal handler will update
            // ScrollPhysics with direction-aware bounds checking. We also update
            // scroll_offsets here for rendering, but the internal handler may clamp values.
            self.handler_registry.dispatch(&ctx);
            // Don't mark dirty - scroll doesn't require tree rebuild
        }
    }

    // =========================================================================
    // Scroll Offset Management
    // =========================================================================

    /// Apply a scroll delta to a node's scroll offset (without bounds checking)
    pub fn apply_scroll_delta(&mut self, node_id: LayoutNodeId, delta_x: f32, delta_y: f32) {
        let (current_x, current_y) = self.scroll_offsets.get(&node_id).copied().unwrap_or((0.0, 0.0));
        self.scroll_offsets.insert(node_id, (current_x + delta_x, current_y + delta_y));
    }

    /// Apply a scroll delta with bounds checking based on viewport and content size
    pub fn apply_scroll_delta_with_bounds(&mut self, node_id: LayoutNodeId, delta_x: f32, delta_y: f32) {
        let (current_x, current_y) = self.scroll_offsets.get(&node_id).copied().unwrap_or((0.0, 0.0));

        // Get the viewport bounds for this node (parent offset doesn't matter for size)
        let bounds = self.layout_tree.get_bounds(node_id, (0.0, 0.0));
        let viewport_width = bounds.map(|b| b.width).unwrap_or(0.0);
        let viewport_height = bounds.map(|b| b.height).unwrap_or(0.0);

        // Get content size from Taffy's content_size
        let (content_width, content_height) = self.layout_tree
            .get_content_size(node_id)
            .unwrap_or((viewport_width, viewport_height));

        // Calculate scroll limits
        let min_offset_x = 0.0;
        let max_offset_x = if content_width > viewport_width {
            -(content_width - viewport_width)
        } else {
            0.0
        };
        let min_offset_y = 0.0;
        let max_offset_y = if content_height > viewport_height {
            -(content_height - viewport_height)
        } else {
            0.0
        };

        // Apply delta with clamping
        let new_x = (current_x + delta_x).clamp(max_offset_x, min_offset_x);
        let new_y = (current_y + delta_y).clamp(max_offset_y, min_offset_y);

        tracing::debug!(
            "Scroll bounds: viewport=({:.0}, {:.0}) content=({:.0}, {:.0}) limits_y=({:.0}, {:.0}) delta_y={:.1} current={:.1} new={:.1}",
            viewport_width, viewport_height, content_width, content_height,
            max_offset_y, min_offset_y, delta_y, current_y, new_y
        );

        self.scroll_offsets.insert(node_id, (new_x, new_y));
    }

    /// Set the scroll offset for a node
    pub fn set_scroll_offset(&mut self, node_id: LayoutNodeId, offset_x: f32, offset_y: f32) {
        self.scroll_offsets.insert(node_id, (offset_x, offset_y));
    }

    /// Get the scroll offset for a node
    ///
    /// Reads from scroll physics if available (has direction-aware bounds),
    /// falls back to legacy scroll_offsets.
    pub fn get_scroll_offset(&self, node_id: LayoutNodeId) -> (f32, f32) {
        // Check scroll physics first (has direction-aware scroll from element)
        if let Some(physics) = self.scroll_physics.get(&node_id) {
            if let Ok(p) = physics.try_lock() {
                return (p.offset_x, p.offset_y);
            }
        }
        // Fallback to legacy scroll_offsets
        self.scroll_offsets.get(&node_id).copied().unwrap_or((0.0, 0.0))
    }

    /// Transfer scroll offsets from another tree (preserves scroll position across rebuilds)
    pub fn transfer_scroll_offsets_from(&mut self, other: &RenderTree) {
        for (node_id, offset) in &other.scroll_offsets {
            self.scroll_offsets.insert(*node_id, *offset);
        }
    }

    /// Transfer scroll physics from another tree (preserves scroll physics across rebuilds)
    pub fn transfer_scroll_physics_from(&mut self, other: &RenderTree) {
        for (node_id, physics) in &other.scroll_physics {
            self.scroll_physics.insert(*node_id, physics.clone());
        }
    }

    /// Check if the tree has any dirty nodes (needs rebuild)
    pub fn needs_rebuild(&self) -> bool {
        self.dirty_tracker.has_dirty()
    }

    /// Clear dirty tracking state
    ///
    /// Call this after rebuilding the UI.
    pub fn clear_dirty(&mut self) {
        self.dirty_tracker.clear_all();
    }

    /// Get the dirty tracker for more granular control
    pub fn dirty_tracker(&self) -> &crate::interactive::DirtyTracker {
        &self.dirty_tracker
    }

    /// Get the dirty tracker mutably
    pub fn dirty_tracker_mut(&mut self) -> &mut crate::interactive::DirtyTracker {
        &mut self.dirty_tracker
    }

    // =========================================================================
    // Node State Storage (for Stateful elements)
    // =========================================================================

    /// Get or create state for a node
    ///
    /// If state doesn't exist for this node, creates it with the provided initial value.
    /// Returns a clone of the Arc handle to the state.
    pub fn get_or_create_state<S: Send + 'static>(
        &mut self,
        node_id: LayoutNodeId,
        initial: S,
    ) -> Arc<Mutex<S>> {
        // Check if state already exists
        if let Some(existing) = self.node_states.get(&node_id) {
            // Try to downcast to the expected type
            let guard = existing.lock().unwrap();
            if guard.downcast_ref::<S>().is_some() {
                drop(guard);
                // Clone and downcast the Arc
                let cloned = Arc::clone(existing);
                // SAFETY: We just verified the type matches
                return unsafe {
                    Arc::from_raw(Arc::into_raw(cloned) as *const Mutex<S>)
                };
            }
        }

        // Create new state
        let state: Arc<Mutex<S>> = Arc::new(Mutex::new(initial));
        let erased: NodeStateStorage = state.clone();
        self.node_states.insert(node_id, erased);
        state
    }

    /// Get existing state for a node (if any)
    pub fn get_state<S: Send + 'static>(&self, node_id: LayoutNodeId) -> Option<Arc<Mutex<S>>> {
        self.node_states.get(&node_id).and_then(|existing| {
            let guard = existing.lock().unwrap();
            if guard.downcast_ref::<S>().is_some() {
                drop(guard);
                let cloned = Arc::clone(existing);
                // SAFETY: We just verified the type matches
                Some(unsafe { Arc::from_raw(Arc::into_raw(cloned) as *const Mutex<S>) })
            } else {
                None
            }
        })
    }

    /// Update render props for a node
    ///
    /// This allows event handlers to modify visual properties without
    /// triggering a full tree rebuild.
    pub fn update_render_props<F>(&mut self, node_id: LayoutNodeId, f: F)
    where
        F: FnOnce(&mut RenderProps),
    {
        if let Some(render_node) = self.render_nodes.get_mut(&node_id) {
            f(&mut render_node.props);
        }
    }

    /// Transfer node states from another tree
    ///
    /// This preserves state across rebuilds by copying the state storage
    /// from the old tree to the new one.
    pub fn transfer_states_from(&mut self, other: &RenderTree) {
        for (node_id, state) in &other.node_states {
            self.node_states.insert(*node_id, Arc::clone(state));
        }
    }

    /// Get the node states map (for transferring to a new tree)
    pub fn node_states(&self) -> &HashMap<LayoutNodeId, NodeStateStorage> {
        &self.node_states
    }

    /// Render the entire tree to a DrawContext
    pub fn render(&self, ctx: &mut dyn DrawContext) {
        if let Some(root) = self.root {
            self.render_node(ctx, root, (0.0, 0.0));
        }
    }

    /// Render a single node and its children
    fn render_node(
        &self,
        ctx: &mut dyn DrawContext,
        node: LayoutNodeId,
        parent_offset: (f32, f32),
    ) {
        let Some(bounds) = self.layout_tree.get_bounds(node, parent_offset) else {
            return;
        };

        let Some(render_node) = self.render_nodes.get(&node) else {
            return;
        };

        // Push transform for this node's position
        ctx.push_transform(Transform::translate(bounds.x, bounds.y));

        // Apply element-specific transform if present
        // Transforms are applied around the element's center (like CSS transform-origin: 50% 50%)
        let has_element_transform = render_node.props.transform.is_some();
        if let Some(ref transform) = render_node.props.transform {
            // To center transforms:
            // 1. Translate so element center is at origin
            // 2. Apply the user's transform
            // 3. Translate back
            let center_x = bounds.width / 2.0;
            let center_y = bounds.height / 2.0;
            ctx.push_transform(Transform::translate(center_x, center_y));
            ctx.push_transform(transform.clone());
            ctx.push_transform(Transform::translate(-center_x, -center_y));
        }

        let rect = Rect::new(0.0, 0.0, bounds.width, bounds.height);
        let radius = render_node.props.border_radius;

        // Check if this node has a glass material - if so, render as glass with shadow
        if let Some(Material::Glass(glass)) = &render_node.props.material {
            // For glass elements, pass shadow through GlassStyle to use GPU glass shadow system
            let glass_brush = Brush::Glass(GlassStyle {
                blur: glass.blur,
                tint: glass.tint,
                saturation: glass.saturation,
                brightness: glass.brightness,
                noise: glass.noise,
                border_thickness: glass.border_thickness,
                shadow: render_node.props.shadow.clone(),
            });
            ctx.fill_rect(rect, radius, glass_brush);
        } else {
            // For non-glass elements, draw shadow first (renders behind the element)
            if let Some(ref shadow) = render_node.props.shadow {
                ctx.draw_shadow(rect, radius, shadow.clone());
            }
            // Draw regular background
            if let Some(ref bg) = render_node.props.background {
                ctx.fill_rect(rect, radius, bg.clone());
            }
        }

        // Push clip if this element clips its children (e.g., scroll containers)
        // Clip to padding box (full bounds, excludes border but includes padding)
        // This matches CSS overflow:hidden behavior
        let clips_content = render_node.props.clips_content;
        if clips_content {
            let clip_rect = Rect::new(0.0, 0.0, bounds.width, bounds.height);
            let clip_shape = if radius.is_uniform() && radius.top_left > 0.0 {
                ClipShape::rounded_rect(clip_rect, radius)
            } else {
                ClipShape::rect(clip_rect)
            };
            ctx.push_clip(clip_shape);
        }

        // Check if this node has scroll and apply the offset
        let scroll_offset = self.get_scroll_offset(node);
        let has_scroll = scroll_offset.0.abs() > 0.001 || scroll_offset.1.abs() > 0.001;

        if has_scroll {
            // Apply scroll offset as a transform
            // Positive offset_y = scrolled down = content moves up = negative translation
            ctx.push_transform(Transform::translate(scroll_offset.0, scroll_offset.1));
        }

        // Render children (relative to this node's transform + scroll offset)
        for child_id in self.layout_tree.children(node) {
            self.render_node(ctx, child_id, (0.0, 0.0));
        }

        // Pop scroll transform if we pushed one
        if has_scroll {
            ctx.pop_transform();
        }

        // Pop clip if we pushed one
        if clips_content {
            ctx.pop_clip();
        }

        // Pop element-specific transforms if we pushed them (3 transforms for centering)
        if has_element_transform {
            ctx.pop_transform(); // pop translate(-center_x, -center_y)
            ctx.pop_transform(); // pop the actual transform
            ctx.pop_transform(); // pop translate(center_x, center_y)
        }

        // Pop transform
        ctx.pop_transform();
    }

    /// Render with layer separation for glass effects
    ///
    /// This method renders elements in three passes:
    /// 1. Background elements (will be blurred behind glass)
    /// 2. Glass elements (blur effect via Brush::Glass)
    /// 3. Foreground elements (on top, not blurred)
    ///
    /// **Important:** Children of glass elements are automatically rendered
    /// in the foreground pass - no need to mark them with `.foreground()`.
    ///
    /// All three layers are rendered to the same context. Glass elements
    /// are rendered as `Brush::Glass` which the GPU renderer handles
    /// by pushing to the glass primitive batch for multi-pass rendering.
    pub fn render_layered_simple(&self, ctx: &mut dyn DrawContext) {
        if let Some(root) = self.root {
            // Pass 1: Background (excludes children of glass elements)
            self.render_layer(ctx, root, (0.0, 0.0), RenderLayer::Background, false);

            // Pass 2: Glass - these render as Brush::Glass which becomes glass primitives
            self.render_layer(ctx, root, (0.0, 0.0), RenderLayer::Glass, false);

            // Pass 3: Foreground (includes children of glass elements)
            self.render_layer(ctx, root, (0.0, 0.0), RenderLayer::Foreground, false);
        }
    }

    /// Render with layer separation and explicit context control
    ///
    /// For cases where you need separate DrawContext instances for
    /// background and foreground (e.g., different render targets).
    ///
    /// **Important:** Children of glass elements are automatically rendered
    /// in the foreground pass - no need to mark them with `.foreground()`.
    ///
    /// Note: Glass elements are rendered to `glass_ctx` using `Brush::Glass`
    /// which the GPU renderer collects as glass primitives.
    pub fn render_layered(
        &self,
        background_ctx: &mut dyn DrawContext,
        glass_ctx: &mut dyn DrawContext,
        foreground_ctx: &mut dyn DrawContext,
    ) {
        if let Some(root) = self.root {
            // Pass 1: Background (excludes children of glass elements)
            self.render_layer(
                background_ctx,
                root,
                (0.0, 0.0),
                RenderLayer::Background,
                false,
            );

            // Pass 2: Glass - render as Brush::Glass
            self.render_layer(glass_ctx, root, (0.0, 0.0), RenderLayer::Glass, false);

            // Pass 3: Foreground (includes children of glass elements)
            self.render_layer(
                foreground_ctx,
                root,
                (0.0, 0.0),
                RenderLayer::Foreground,
                false,
            );
        }
    }

    /// Render only elements in a specific layer to a DrawContext
    ///
    /// This is useful when you need to render background+glass to one context
    /// and foreground to another context (e.g., for proper glass compositing).
    ///
    /// **Important:** Children of glass elements are automatically considered
    /// as foreground - no need to mark them with `.foreground()`.
    pub fn render_to_layer(&self, ctx: &mut dyn DrawContext, target_layer: RenderLayer) {
        if let Some(root) = self.root {
            self.render_layer(ctx, root, (0.0, 0.0), target_layer, false);
        }
    }

    /// Render only nodes in a specific layer
    ///
    /// The `inside_glass` flag tracks whether we're descending through a glass element.
    /// Children of glass elements are automatically rendered in the foreground pass.
    fn render_layer(
        &self,
        ctx: &mut dyn DrawContext,
        node: LayoutNodeId,
        parent_offset: (f32, f32),
        target_layer: RenderLayer,
        inside_glass: bool,
    ) {
        let Some(bounds) = self.layout_tree.get_bounds(node, parent_offset) else {
            return;
        };

        let Some(render_node) = self.render_nodes.get(&node) else {
            return;
        };

        // Always push transform for proper child positioning
        ctx.push_transform(Transform::translate(bounds.x, bounds.y));

        // Apply element-specific transform if present
        // Transforms are applied around the element's center (like CSS transform-origin: 50% 50%)
        let has_element_transform = render_node.props.transform.is_some();
        if let Some(ref transform) = render_node.props.transform {
            // To center transforms:
            // 1. Translate so element center is at origin
            // 2. Apply the user's transform
            // 3. Translate back
            let center_x = bounds.width / 2.0;
            let center_y = bounds.height / 2.0;
            ctx.push_transform(Transform::translate(center_x, center_y));
            ctx.push_transform(transform.clone());
            ctx.push_transform(Transform::translate(-center_x, -center_y));
        }

        // Determine if this node is a glass element
        let is_glass = matches!(render_node.props.material, Some(Material::Glass(_)));

        // Track if children should be considered inside glass
        // Once inside glass, stay inside glass for all descendants
        let children_inside_glass = inside_glass || is_glass;

        // Push clip BEFORE rendering content if this element clips its children
        // Clip to padding box (full bounds) - matches CSS overflow:hidden behavior
        let clips_content = render_node.props.clips_content;
        if clips_content {
            let clip_rect = Rect::new(0.0, 0.0, bounds.width, bounds.height);
            let radius = render_node.props.border_radius;
            let clip_shape = if radius.is_uniform() && radius.top_left > 0.0 {
                ClipShape::rounded_rect(clip_rect, radius)
            } else {
                ClipShape::rect(clip_rect)
            };
            ctx.push_clip(clip_shape);
        }

        // Determine the effective layer for this node:
        // - If we're inside a glass element, children render as foreground
        // - Otherwise, use the node's explicit layer setting
        let effective_layer = if inside_glass && !is_glass {
            // Children of glass elements render in foreground
            RenderLayer::Foreground
        } else if is_glass {
            // Glass elements render in glass layer
            RenderLayer::Glass
        } else {
            // Use the node's explicit layer
            render_node.props.layer
        };

        // Only render if this node matches the target layer
        if effective_layer == target_layer {
            let rect = Rect::new(0.0, 0.0, bounds.width, bounds.height);
            let radius = render_node.props.border_radius;

            // Check if this node has a glass material - if so, render as glass with shadow
            if let Some(Material::Glass(glass)) = &render_node.props.material {
                // For glass elements, pass shadow through GlassStyle to use GPU glass shadow system
                let glass_brush = Brush::Glass(GlassStyle {
                    blur: glass.blur,
                    tint: glass.tint,
                    saturation: glass.saturation,
                    brightness: glass.brightness,
                    noise: glass.noise,
                    border_thickness: glass.border_thickness,
                    shadow: render_node.props.shadow.clone(),
                });
                ctx.fill_rect(rect, radius, glass_brush);
            } else {
                // For non-glass elements, draw shadow first (renders behind the element)
                if let Some(ref shadow) = render_node.props.shadow {
                    ctx.draw_shadow(rect, radius, shadow.clone());
                }
                // Draw regular background
                if let Some(ref bg) = render_node.props.background {
                    ctx.fill_rect(rect, radius, bg.clone());
                }
            }

            // Handle canvas element rendering
            if let ElementType::Canvas(canvas_data) = &render_node.element_type {
                if let Some(render_fn) = &canvas_data.render_fn {
                    // Push clip for canvas bounds - canvas drawing should not overflow
                    let canvas_rect = Rect::new(0.0, 0.0, bounds.width, bounds.height);
                    ctx.push_clip(ClipShape::rect(canvas_rect));

                    let canvas_bounds = crate::canvas::CanvasBounds {
                        width: bounds.width,
                        height: bounds.height,
                    };
                    render_fn(ctx, canvas_bounds);

                    // Pop canvas clip
                    ctx.pop_clip();
                }
            }
        }

        // Check if this node has a scroll offset and apply it to children
        let scroll_offset = self.get_scroll_offset(node);
        let has_scroll = scroll_offset.0.abs() > 0.001 || scroll_offset.1.abs() > 0.001;

        if has_scroll {
            // Apply scroll offset as a transform
            // Positive offset_y = scrolled down = content moves up = negative translation
            ctx.push_transform(Transform::translate(scroll_offset.0, scroll_offset.1));
        }

        // Traverse children (they inherit our transform)
        for child_id in self.layout_tree.children(node) {
            self.render_layer(
                ctx,
                child_id,
                (0.0, 0.0),
                target_layer,
                children_inside_glass,
            );
        }

        // Pop scroll transform if we pushed one
        if has_scroll {
            ctx.pop_transform();
        }

        // Pop clip if we pushed one
        if clips_content {
            ctx.pop_clip();
        }

        // Pop element-specific transforms if we pushed them (3 transforms for centering)
        if has_element_transform {
            ctx.pop_transform(); // pop translate(-center_x, -center_y)
            ctx.pop_transform(); // pop the actual transform
            ctx.pop_transform(); // pop translate(center_x, center_y)
        }

        ctx.pop_transform();
    }

    /// Get bounds for a specific node
    pub fn get_bounds(&self, node: LayoutNodeId) -> Option<ElementBounds> {
        self.layout_tree.get_bounds(node, (0.0, 0.0))
    }

    /// Get absolute bounds for a node (traversing up the tree)
    pub fn get_absolute_bounds(&self, node: LayoutNodeId) -> Option<ElementBounds> {
        // For now, just return bounds from root (0,0)
        // A more complete implementation would track parent offsets
        self.layout_tree.get_bounds(node, (0.0, 0.0))
    }

    /// Get render node data
    pub fn get_render_node(&self, node: LayoutNodeId) -> Option<&RenderNode> {
        self.render_nodes.get(&node)
    }

    /// Iterate over all nodes with their bounds and render props
    pub fn iter_nodes(&self) -> impl Iterator<Item = (LayoutNodeId, &RenderNode)> {
        self.render_nodes.iter().map(|(&id, node)| (id, node))
    }

    /// Check if this tree contains any glass elements
    pub fn has_glass(&self) -> bool {
        self.render_nodes
            .values()
            .any(|node| matches!(node.props.material, Some(Material::Glass(_))))
    }

    /// Render the tree using a LayoutRenderer
    ///
    /// This is the primary rendering method. The LayoutRenderer handles:
    /// - Background/foreground layer separation (automatically if glass is present)
    /// - Text rendering at layout-computed positions
    /// - SVG rendering at layout-computed positions
    ///
    /// Example:
    /// ```ignore
    /// tree.render_to(&mut my_renderer);
    /// ```
    pub fn render_to<R: LayoutRenderer>(&self, renderer: &mut R) {
        if let Some(root) = self.root {
            // Pass 1: Background elements
            {
                let ctx = renderer.background();
                self.render_layer_with_content(
                    ctx,
                    root,
                    (0.0, 0.0),
                    RenderLayer::Background,
                    false,
                );
            }

            // Pass 2: Glass elements (to background context)
            {
                let ctx = renderer.background();
                self.render_layer_with_content(ctx, root, (0.0, 0.0), RenderLayer::Glass, false);
            }

            // Pass 3: Foreground elements (including glass children)
            {
                let ctx = renderer.foreground();
                self.render_layer_with_content(
                    ctx,
                    root,
                    (0.0, 0.0),
                    RenderLayer::Foreground,
                    false,
                );
            }

            // Pass 4: Render text elements
            self.render_text_elements(renderer);

            // Pass 5: Render SVG elements
            self.render_svg_elements(renderer);
        }
    }

    /// Render a layer (divs only - text/SVG handled separately)
    fn render_layer_with_content(
        &self,
        ctx: &mut dyn DrawContext,
        node: LayoutNodeId,
        parent_offset: (f32, f32),
        target_layer: RenderLayer,
        inside_glass: bool,
    ) {
        let Some(bounds) = self.layout_tree.get_bounds(node, parent_offset) else {
            return;
        };

        let Some(render_node) = self.render_nodes.get(&node) else {
            return;
        };

        // Always push transform for proper child positioning
        ctx.push_transform(Transform::translate(bounds.x, bounds.y));

        // Apply element-specific transform if present
        // Transforms are applied around the element's center (like CSS transform-origin: 50% 50%)
        let has_element_transform = render_node.props.transform.is_some();
        if let Some(ref transform) = render_node.props.transform {
            // To center transforms:
            // 1. Translate so element center is at origin
            // 2. Apply the user's transform
            // 3. Translate back
            let center_x = bounds.width / 2.0;
            let center_y = bounds.height / 2.0;
            ctx.push_transform(Transform::translate(center_x, center_y));
            ctx.push_transform(transform.clone());
            ctx.push_transform(Transform::translate(-center_x, -center_y));
        }

        // Determine if this node is a glass element
        let is_glass = matches!(render_node.props.material, Some(Material::Glass(_)));

        // Track if children should be considered inside glass
        let children_inside_glass = inside_glass || is_glass;

        // Push clip BEFORE rendering content if this element clips its children
        // Clip to padding box (full bounds) - matches CSS overflow:hidden behavior
        let clips_content = render_node.props.clips_content;
        if clips_content {
            let clip_rect = Rect::new(0.0, 0.0, bounds.width, bounds.height);
            let radius = render_node.props.border_radius;
            let clip_shape = if radius.is_uniform() && radius.top_left > 0.0 {
                ClipShape::rounded_rect(clip_rect, radius)
            } else {
                ClipShape::rect(clip_rect)
            };
            ctx.push_clip(clip_shape);
        }

        // Determine the effective layer for this node
        let effective_layer = if inside_glass && !is_glass {
            RenderLayer::Foreground
        } else if is_glass {
            RenderLayer::Glass
        } else {
            render_node.props.layer
        };

        // Only render divs and canvas here (text/SVG handled in separate passes)
        if effective_layer == target_layer {
            match &render_node.element_type {
                ElementType::Div => {
                    let rect = Rect::new(0.0, 0.0, bounds.width, bounds.height);
                    let radius = render_node.props.border_radius;

                    // Check if this node has a glass material - if so, render as glass with shadow
                    if let Some(Material::Glass(glass)) = &render_node.props.material {
                        // For glass elements, pass shadow through GlassStyle to use GPU glass shadow system
                        let glass_brush = Brush::Glass(GlassStyle {
                            blur: glass.blur,
                            tint: glass.tint,
                            saturation: glass.saturation,
                            brightness: glass.brightness,
                            noise: glass.noise,
                            border_thickness: glass.border_thickness,
                            shadow: render_node.props.shadow.clone(),
                        });
                        ctx.fill_rect(rect, radius, glass_brush);
                    } else {
                        // For non-glass elements, draw shadow first (renders behind the element)
                        if let Some(ref shadow) = render_node.props.shadow {
                            ctx.draw_shadow(rect, radius, shadow.clone());
                        }
                        // Draw regular background
                        if let Some(ref bg) = render_node.props.background {
                            ctx.fill_rect(rect, radius, bg.clone());
                        }
                    }
                }
                ElementType::Canvas(canvas_data) => {
                    // Canvas element: invoke the render callback with DrawContext
                    if let Some(render_fn) = &canvas_data.render_fn {
                        // Push clip for canvas bounds - canvas drawing should not overflow
                        let canvas_rect = Rect::new(0.0, 0.0, bounds.width, bounds.height);
                        ctx.push_clip(ClipShape::rect(canvas_rect));

                        let canvas_bounds = crate::canvas::CanvasBounds {
                            width: bounds.width,
                            height: bounds.height,
                        };
                        render_fn(ctx, canvas_bounds);

                        // Pop canvas clip
                        ctx.pop_clip();
                    }
                }
                // Text, SVG, Image are handled in separate passes
                _ => {}
            }
        }

        // Check if this node has a scroll offset and apply it to children
        let scroll_offset = self.get_scroll_offset(node);
        let has_scroll = scroll_offset.0.abs() > 0.001 || scroll_offset.1.abs() > 0.001;

        if has_scroll {
            // Apply scroll offset as a transform
            // Positive offset_y = scrolled down = content moves up = negative translation
            ctx.push_transform(Transform::translate(scroll_offset.0, scroll_offset.1));
        }

        // Traverse children
        for child_id in self.layout_tree.children(node) {
            self.render_layer_with_content(
                ctx,
                child_id,
                (0.0, 0.0),
                target_layer,
                children_inside_glass,
            );
        }

        // Pop scroll transform if we pushed one
        if has_scroll {
            ctx.pop_transform();
        }

        // Pop clip if we pushed one
        if clips_content {
            ctx.pop_clip();
        }

        // Pop element-specific transforms if we pushed them (3 transforms for centering)
        if has_element_transform {
            ctx.pop_transform(); // pop translate(-center_x, -center_y)
            ctx.pop_transform(); // pop the actual transform
            ctx.pop_transform(); // pop translate(center_x, center_y)
        }

        ctx.pop_transform();
    }

    /// Render all text elements via the LayoutRenderer
    fn render_text_elements<R: LayoutRenderer>(&self, renderer: &mut R) {
        if let Some(root) = self.root {
            self.render_text_recursive(renderer, root, (0.0, 0.0), false);
        }
    }

    /// Recursively render text elements
    fn render_text_recursive<R: LayoutRenderer>(
        &self,
        renderer: &mut R,
        node: LayoutNodeId,
        parent_offset: (f32, f32),
        inside_glass: bool,
    ) {
        let Some(bounds) = self.layout_tree.get_bounds(node, parent_offset) else {
            return;
        };

        let Some(render_node) = self.render_nodes.get(&node) else {
            return;
        };

        let is_glass = matches!(render_node.props.material, Some(Material::Glass(_)));
        let children_inside_glass = inside_glass || is_glass;

        // Text inside glass goes to foreground
        let to_foreground =
            children_inside_glass || render_node.props.layer == RenderLayer::Foreground;

        if let ElementType::Text(text_data) = &render_node.element_type {
            // Absolute position for text
            let abs_x = parent_offset.0 + bounds.x;
            let abs_y = parent_offset.1 + bounds.y;

            if to_foreground {
                renderer.render_text_foreground(
                    &text_data.content,
                    abs_x,
                    abs_y,
                    bounds.width,
                    bounds.height,
                    text_data.font_size,
                    text_data.color,
                    text_data.align,
                    text_data.weight,
                );
            } else {
                renderer.render_text_background(
                    &text_data.content,
                    abs_x,
                    abs_y,
                    bounds.width,
                    bounds.height,
                    text_data.font_size,
                    text_data.color,
                    text_data.align,
                    text_data.weight,
                );
            }
        }

        // Include scroll offset when calculating child positions
        let scroll_offset = self.get_scroll_offset(node);
        let new_offset = (
            parent_offset.0 + bounds.x + scroll_offset.0,
            parent_offset.1 + bounds.y + scroll_offset.1,
        );
        for child_id in self.layout_tree.children(node) {
            self.render_text_recursive(renderer, child_id, new_offset, children_inside_glass);
        }
    }

    /// Render all SVG elements via the LayoutRenderer
    fn render_svg_elements<R: LayoutRenderer>(&self, renderer: &mut R) {
        if let Some(root) = self.root {
            self.render_svg_recursive(renderer, root, (0.0, 0.0), false);
        }
    }

    /// Recursively render SVG elements
    fn render_svg_recursive<R: LayoutRenderer>(
        &self,
        renderer: &mut R,
        node: LayoutNodeId,
        parent_offset: (f32, f32),
        inside_glass: bool,
    ) {
        let Some(bounds) = self.layout_tree.get_bounds(node, parent_offset) else {
            return;
        };

        let Some(render_node) = self.render_nodes.get(&node) else {
            return;
        };

        let is_glass = matches!(render_node.props.material, Some(Material::Glass(_)));
        let children_inside_glass = inside_glass || is_glass;

        // SVG inside glass goes to foreground
        let to_foreground =
            children_inside_glass || render_node.props.layer == RenderLayer::Foreground;

        if let ElementType::Svg(svg_data) = &render_node.element_type {
            // Absolute position for SVG
            let abs_x = parent_offset.0 + bounds.x;
            let abs_y = parent_offset.1 + bounds.y;

            if to_foreground {
                renderer.render_svg_foreground(
                    &svg_data.source,
                    abs_x,
                    abs_y,
                    bounds.width,
                    bounds.height,
                    svg_data.tint,
                );
            } else {
                renderer.render_svg_background(
                    &svg_data.source,
                    abs_x,
                    abs_y,
                    bounds.width,
                    bounds.height,
                    svg_data.tint,
                );
            }
        }

        // Include scroll offset when calculating child positions
        let scroll_offset = self.get_scroll_offset(node);
        let new_offset = (
            parent_offset.0 + bounds.x + scroll_offset.0,
            parent_offset.1 + bounds.y + scroll_offset.1,
        );
        for child_id in self.layout_tree.children(node) {
            self.render_svg_recursive(renderer, child_id, new_offset, children_inside_glass);
        }
    }

    /// Collect all glass panels from the layout tree
    ///
    /// # Deprecated
    /// Use `render()` or `render_layered_simple()` instead. Glass elements
    /// are now rendered as `Brush::Glass` in the normal render pipeline.
    #[deprecated(
        since = "0.2.0",
        note = "Use render() or render_layered_simple() instead. Glass is now integrated into the normal render pipeline."
    )]
    #[allow(deprecated)]
    pub fn collect_glass_panels(&self) -> Vec<GlassPanel> {
        let mut panels = Vec::new();
        if let Some(root) = self.root {
            self.collect_glass_panels_recursive(root, (0.0, 0.0), &mut panels);
        }
        panels
    }

    /// Recursively collect glass panels (deprecated)
    #[allow(deprecated)]
    fn collect_glass_panels_recursive(
        &self,
        node: LayoutNodeId,
        parent_offset: (f32, f32),
        panels: &mut Vec<GlassPanel>,
    ) {
        let Some(bounds) = self.layout_tree.get_bounds(node, parent_offset) else {
            return;
        };

        if let Some(render_node) = self.render_nodes.get(&node) {
            // Check if this node has a glass material
            if let Some(Material::Glass(glass)) = &render_node.props.material {
                panels.push(GlassPanel {
                    bounds,
                    corner_radius: render_node.props.border_radius,
                    material: glass.clone(),
                    node_id: node,
                });
            }
        }

        // Traverse children
        let new_offset = (parent_offset.0 + bounds.x, parent_offset.1 + bounds.y);
        for child_id in self.layout_tree.children(node) {
            self.collect_glass_panels_recursive(child_id, new_offset, panels);
        }
    }

    // =========================================================================
    // Element iterators - for platform-specific text/SVG rendering
    // =========================================================================

    /// Get all text elements with their computed bounds
    ///
    /// Returns an iterator of (TextData, ElementBounds) for each text element
    /// in the tree. Use this to render text with your platform's text renderer.
    ///
    /// # Example
    /// ```ignore
    /// for (text, bounds) in tree.text_elements() {
    ///     my_renderer.draw_text(&text.content, bounds.x, bounds.y, text.font_size);
    /// }
    /// ```
    pub fn text_elements(&self) -> Vec<(TextData, ElementBounds)> {
        let mut result = Vec::new();
        if let Some(root) = self.root {
            self.collect_text_elements(root, (0.0, 0.0), &mut result);
        }
        result
    }

    fn collect_text_elements(
        &self,
        node: LayoutNodeId,
        parent_offset: (f32, f32),
        result: &mut Vec<(TextData, ElementBounds)>,
    ) {
        let Some(bounds) = self.layout_tree.get_bounds(node, parent_offset) else {
            return;
        };

        if let Some(render_node) = self.render_nodes.get(&node) {
            if let ElementType::Text(text_data) = &render_node.element_type {
                let abs_bounds = ElementBounds {
                    x: parent_offset.0 + bounds.x,
                    y: parent_offset.1 + bounds.y,
                    width: bounds.width,
                    height: bounds.height,
                };
                result.push((text_data.clone(), abs_bounds));
            }
        }

        // Include scroll offset when calculating child positions
        let scroll_offset = self.get_scroll_offset(node);
        let new_offset = (
            parent_offset.0 + bounds.x + scroll_offset.0,
            parent_offset.1 + bounds.y + scroll_offset.1,
        );
        for child_id in self.layout_tree.children(node) {
            self.collect_text_elements(child_id, new_offset, result);
        }
    }

    /// Get all SVG elements with their computed bounds
    ///
    /// Returns an iterator of (SvgData, ElementBounds) for each SVG element
    /// in the tree. Use this to render SVGs with your platform's SVG renderer.
    ///
    /// # Example
    /// ```ignore
    /// for (svg, bounds) in tree.svg_elements() {
    ///     my_renderer.draw_svg(&svg.source, bounds.x, bounds.y, bounds.width, bounds.height);
    /// }
    /// ```
    pub fn svg_elements(&self) -> Vec<(SvgData, ElementBounds)> {
        let mut result = Vec::new();
        if let Some(root) = self.root {
            self.collect_svg_elements(root, (0.0, 0.0), &mut result);
        }
        result
    }

    fn collect_svg_elements(
        &self,
        node: LayoutNodeId,
        parent_offset: (f32, f32),
        result: &mut Vec<(SvgData, ElementBounds)>,
    ) {
        let Some(bounds) = self.layout_tree.get_bounds(node, parent_offset) else {
            return;
        };

        if let Some(render_node) = self.render_nodes.get(&node) {
            if let ElementType::Svg(svg_data) = &render_node.element_type {
                let abs_bounds = ElementBounds {
                    x: parent_offset.0 + bounds.x,
                    y: parent_offset.1 + bounds.y,
                    width: bounds.width,
                    height: bounds.height,
                };
                result.push((svg_data.clone(), abs_bounds));
            }
        }

        // Include scroll offset when calculating child positions
        let scroll_offset = self.get_scroll_offset(node);
        let new_offset = (
            parent_offset.0 + bounds.x + scroll_offset.0,
            parent_offset.1 + bounds.y + scroll_offset.1,
        );
        for child_id in self.layout_tree.children(node) {
            self.collect_svg_elements(child_id, new_offset, result);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::div::div;

    #[test]
    fn test_render_tree_from_element() {
        let ui = div().w(100.0).h(100.0).child(div().w(50.0).h(50.0));

        let tree = RenderTree::from_element(&ui);
        assert!(tree.root().is_some());
    }

    #[test]
    fn test_compute_layout() {
        let ui = div()
            .w(200.0)
            .h(200.0)
            .flex_col()
            .child(div().h(50.0).w_full())
            .child(div().flex_grow().w_full());

        let mut tree = RenderTree::from_element(&ui);
        tree.compute_layout(200.0, 200.0);

        let root = tree.root().unwrap();
        let bounds = tree.get_bounds(root).unwrap();

        assert_eq!(bounds.width, 200.0);
        assert_eq!(bounds.height, 200.0);
    }
}
