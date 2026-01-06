//! RenderTree bridge connecting layout to rendering
//!
//! This module provides the bridge between Taffy layout computation
//! and the DrawContext rendering API.

use std::any::Any;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, Weak};

use blinc_animation::AnimationScheduler;
use indexmap::IndexMap;

use blinc_core::{
    Brush, ClipShape, Color, CornerRadius, DrawContext, GlassStyle, Rect, Shadow, Stroke, Transform,
};
use taffy::prelude::*;

use crate::canvas::CanvasData;
use crate::css_parser::{ElementState, Stylesheet};
use crate::diff::{render_props_eq, ChangeCategory, DivHash};
use crate::div::{ElementBuilder, ElementTypeId};
use crate::element::{ElementBounds, GlassMaterial, Material, RenderLayer, RenderProps};
use crate::selector::{ElementRegistry, ScrollRef};
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
    /// Styled text with multiple color spans (for syntax highlighting)
    StyledText(StyledTextData),
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
    /// Whether to use italic style
    pub italic: bool,
    pub v_align: crate::div::TextVerticalAlign,
    /// Whether to wrap text at container bounds
    pub wrap: bool,
    /// Line height multiplier
    pub line_height: f32,
    /// Measured width (before layout constraints)
    pub measured_width: f32,
    /// Font family category
    pub font_family: crate::div::FontFamily,
    /// Word spacing in pixels (0.0 = normal)
    pub word_spacing: f32,
    /// Font ascender in pixels (distance from baseline to top)
    pub ascender: f32,
    /// Whether text has strikethrough decoration
    pub strikethrough: bool,
    /// Whether text has underline decoration
    pub underline: bool,
}

/// A styled span within rich text
#[derive(Clone, Debug)]
pub struct StyledTextSpan {
    /// Start byte index in text
    pub start: usize,
    /// End byte index in text (exclusive)
    pub end: usize,
    /// RGBA color
    pub color: [f32; 4],
    /// Whether text is bold
    pub bold: bool,
    /// Whether text is italic
    pub italic: bool,
    /// Whether text has underline decoration
    pub underline: bool,
    /// Whether text has strikethrough decoration
    pub strikethrough: bool,
    /// Optional link URL (for clickable spans)
    pub link_url: Option<String>,
}

impl StyledTextSpan {
    /// Create a new styled text span with just color (no decorations)
    pub fn new(start: usize, end: usize, color: [f32; 4]) -> Self {
        Self {
            start,
            end,
            color,
            bold: false,
            italic: false,
            underline: false,
            strikethrough: false,
            link_url: None,
        }
    }

    /// Create from a TextSpan (from styled_text module)
    pub fn from_text_span(span: &crate::styled_text::TextSpan) -> Self {
        Self {
            start: span.start,
            end: span.end,
            color: span.color.to_array(),
            bold: span.bold,
            italic: span.italic,
            underline: span.underline,
            strikethrough: span.strikethrough,
            link_url: span.link_url.clone(),
        }
    }
}

/// Styled text data for rendering with multiple color spans
#[derive(Clone)]
pub struct StyledTextData {
    /// The full text content
    pub content: String,
    /// Color spans (must cover entire text, sorted by start position)
    pub spans: Vec<StyledTextSpan>,
    /// Default color for unspanned regions
    pub default_color: [f32; 4],
    /// Font size
    pub font_size: f32,
    /// Text alignment
    pub align: crate::div::TextAlign,
    /// Vertical alignment
    pub v_align: crate::div::TextVerticalAlign,
    /// Font family
    pub font_family: crate::div::FontFamily,
    /// Line height multiplier
    pub line_height: f32,
    /// Default font weight (for unspanned regions)
    pub weight: crate::div::FontWeight,
    /// Default italic style (for unspanned regions)
    pub italic: bool,
    /// Measured ascender for consistent baseline alignment
    pub ascender: f32,
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
    /// Loading strategy: 0 = Eager (default), 1 = Lazy
    pub loading_strategy: u8,
    /// Placeholder type: 0 = None, 1 = Color, 2 = Image, 3 = Skeleton
    pub placeholder_type: u8,
    /// Placeholder color [r, g, b, a]
    pub placeholder_color: [f32; 4],
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

/// Storage for computed layout bounds (shared with ElementRef)
pub type LayoutBoundsStorage = Arc<Mutex<Option<ElementBounds>>>;

/// Callback type for layout bounds change notifications
pub type LayoutBoundsCallback = Arc<dyn Fn(ElementBounds) + Send + Sync>;

/// Entry for layout bounds storage with optional change callback
pub struct LayoutBoundsEntry {
    /// The shared storage for bounds
    pub storage: LayoutBoundsStorage,
    /// Optional callback when bounds change (width or height differ from previous)
    pub on_change: Option<LayoutBoundsCallback>,
}

/// Callback type for on_ready notifications when an element is laid out and rendered
///
/// The callback receives the element's computed bounds after layout.
/// This is triggered once per element after its first successful layout computation.
pub type OnReadyCallback = Arc<dyn Fn(ElementBounds) + Send + Sync>;

/// Entry for on_ready callbacks
pub struct OnReadyEntry {
    /// The callback to invoke when the element is ready
    pub callback: OnReadyCallback,
    /// Whether this callback has been triggered (only fires once)
    pub triggered: bool,
}

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
    /// Motion bindings for continuous animations (keyed by node_id)
    motion_bindings: HashMap<LayoutNodeId, crate::motion::MotionBindings>,
    /// Last tick time for scroll physics (in milliseconds)
    last_scroll_tick_ms: Option<u64>,
    /// DPI scale factor (physical / logical pixels)
    ///
    /// When set, all layout positions and sizes are multiplied by this factor
    /// before rendering. This allows users to specify sizes in logical pixels
    /// while rendering happens at physical pixel resolution.
    scale_factor: f32,
    /// Animation scheduler for scroll bounce springs
    animations: Weak<Mutex<AnimationScheduler>>,
    /// Hash of the element tree used to build this RenderTree
    /// Used for quick equality checks to skip unnecessary rebuilds
    tree_hash: Option<DivHash>,
    /// Per-node hashes for incremental change detection
    /// Maps node_id to (own_hash, tree_hash) - own excludes children, tree includes children
    node_hashes: HashMap<LayoutNodeId, (DivHash, DivHash)>,
    /// Layout bounds storages to update after layout computation
    /// Maps node_id to entry with shared storage and optional change callback
    layout_bounds_storages: HashMap<LayoutNodeId, LayoutBoundsEntry>,
    /// Element registry for O(1) lookups by string ID
    element_registry: Arc<ElementRegistry>,
    /// Bound ScrollRefs for programmatic scroll control
    /// Note: NOT cleared on rebuild - ScrollRef inner state persists and node_id is updated
    scroll_refs: HashMap<LayoutNodeId, ScrollRef>,
    /// Active scroll refs (persists across rebuilds, keyed by inner pointer address)
    /// Maps inner pointer -> ScrollRef for persistence across rebuilds
    active_scroll_refs: Vec<ScrollRef>,
    /// On-ready callbacks for elements (fires once after first layout)
    /// Maps string_id to callback entry for stable tracking across rebuilds.
    on_ready_callbacks: HashMap<String, OnReadyEntry>,
    /// Optional stylesheet for automatic state modifier application
    /// When set, elements with IDs will automatically get :hover, :active, :focus, :disabled styles
    stylesheet: Option<Arc<Stylesheet>>,
    /// Base styles for elements (before state modifiers)
    /// Used to restore original styles when state changes
    base_styles: HashMap<LayoutNodeId, RenderProps>,
}

/// Result of an incremental update attempt
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateResult {
    /// No changes detected, tree unchanged
    NoChanges,
    /// Only visual properties changed (no layout needed)
    VisualOnly,
    /// Layout properties changed (layout needs recomputation)
    LayoutChanged,
    /// Children changed - subtree rebuilds queued, needs layout recomputation
    ChildrenChanged,
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
            motion_bindings: HashMap::new(),
            last_scroll_tick_ms: None,
            scale_factor: 1.0,
            animations: Weak::new(),
            tree_hash: None,
            node_hashes: HashMap::new(),
            layout_bounds_storages: HashMap::new(),
            element_registry: Arc::new(ElementRegistry::new()),
            scroll_refs: HashMap::new(),
            active_scroll_refs: Vec::new(),
            on_ready_callbacks: HashMap::new(),
            stylesheet: None,
            base_styles: HashMap::new(),
        }
    }

    /// Set the animation scheduler for scroll bounce animations
    pub fn set_animations(&mut self, scheduler: &Arc<Mutex<AnimationScheduler>>) {
        self.animations = Arc::downgrade(scheduler);
        // Update any existing scroll physics with the scheduler
        for physics in self.scroll_physics.values() {
            if let Some(scheduler_arc) = self.animations.upgrade() {
                physics.lock().unwrap().set_scheduler(&scheduler_arc);
            }
        }
    }

    /// Set a shared external element registry
    ///
    /// This allows the WindowedContext to share the same registry for query operations.
    /// The registry is automatically populated during tree building.
    pub fn set_element_registry(&mut self, registry: Arc<ElementRegistry>) {
        self.element_registry = registry;
    }

    /// Build a render tree from an element builder
    pub fn from_element<E: ElementBuilder>(element: &E) -> Self {
        let mut tree = Self::new();
        // Compute tree hash for change detection
        tree.tree_hash = Some(DivHash::compute_element_tree(element));
        tree.root = Some(tree.build_element(element));
        tree
    }

    /// Build a render tree from an element builder with a shared element registry
    ///
    /// This ensures element IDs are registered to the shared registry during build,
    /// rather than to an internal registry that gets replaced later.
    pub fn from_element_with_registry<E: ElementBuilder>(
        element: &E,
        registry: Arc<ElementRegistry>,
    ) -> Self {
        let mut tree = Self::new();
        // Clear the shared registry before building to avoid duplicate ID warnings
        registry.clear();
        // Set shared registry BEFORE building so IDs are registered correctly
        tree.element_registry = registry;
        // Compute tree hash for change detection
        tree.tree_hash = Some(DivHash::compute_element_tree(element));
        tree.root = Some(tree.build_element(element));
        tree
    }

    /// Get the tree hash for this render tree
    pub fn tree_hash(&self) -> Option<DivHash> {
        self.tree_hash
    }

    /// Check if a new element tree would produce the same render tree
    ///
    /// Returns true if the element tree hash matches, meaning no rebuild is needed.
    pub fn matches_element<E: ElementBuilder>(&self, element: &E) -> bool {
        match self.tree_hash {
            Some(hash) => hash == DivHash::compute_element_tree(element),
            None => false,
        }
    }

    /// Update the render tree from a new element if it has changed
    ///
    /// Returns `true` if the tree was updated, `false` if no changes were detected.
    /// This is an optimization to skip full rebuilds when the UI hasn't changed.
    pub fn update_if_changed<E: ElementBuilder>(&mut self, element: &E) -> bool {
        let new_hash = DivHash::compute_element_tree(element);

        // If hash matches, no changes - skip rebuild
        if self.tree_hash == Some(new_hash) {
            return false;
        }

        // Hash differs - need to rebuild
        // For now, do a full rebuild. Future optimization: use diff for incremental updates
        self.tree_hash = Some(new_hash);

        // Clear existing data that will be repopulated during rebuild
        self.render_nodes.clear();
        self.handler_registry = crate::event_handler::HandlerRegistry::new();
        self.element_registry.clear();
        // Clear scroll_refs HashMap (node_id keyed) - it will be repopulated during rebuild
        // but active_scroll_refs persists for process_pending_scroll_refs
        self.scroll_refs.clear();

        // Preserve node_states, scroll_offsets, scroll_physics, motion_bindings, active_scroll_refs
        // as these should survive rebuilds

        // Rebuild the layout tree
        self.layout_tree = LayoutTree::new();
        self.root = Some(self.build_element(element));

        true
    }

    /// Incrementally update the render tree from a new element
    ///
    /// This method attempts to apply minimal updates based on what changed:
    /// - If nothing changed: returns NoChanges, no work done
    /// - If only visual props changed: updates render props, returns VisualOnly
    /// - If layout changed: updates props + needs relayout, returns LayoutChanged
    /// - If children changed: rebuilds affected subtrees, returns ChildrenChanged
    ///
    /// The caller should:
    /// - NoChanges: skip layout and just render
    /// - VisualOnly: skip layout, just render with updated props
    /// - LayoutChanged: call compute_layout(), then render
    /// - ChildrenChanged: call compute_layout(), then render
    pub fn incremental_update<E: ElementBuilder>(&mut self, element: &E) -> UpdateResult {
        let new_tree_hash = DivHash::compute_element_tree(element);

        // Quick path: if tree hash matches, nothing changed
        if self.tree_hash == Some(new_tree_hash) {
            return UpdateResult::NoChanges;
        }

        // Tree hash differs - analyze what kind of changes occurred
        // Walk the tree comparing per-node hashes to detect change categories
        let Some(root_id) = self.root else {
            // No existing tree - build it (this is initial build, not an update)
            self.tree_hash = Some(new_tree_hash);
            self.root = Some(self.build_element(element));
            return UpdateResult::ChildrenChanged;
        };

        // Analyze changes by comparing stored hashes with new element
        let changes = self.analyze_changes(element, root_id);

        tracing::trace!(
            "incremental_update: layout={}, visual={}, children={}, handlers={}",
            changes.layout,
            changes.visual,
            changes.children,
            changes.handlers
        );

        // Update tree hash
        self.tree_hash = Some(new_tree_hash);

        // Determine update strategy based on change category
        if changes.children {
            // Children changed - rebuild affected subtrees in place
            // Walk tree and rebuild nodes with changed children
            self.rebuild_changed_subtrees(element, root_id);
            // Also update props for nodes that didn't get rebuilt
            self.update_render_props_in_place(element, root_id);
            UpdateResult::ChildrenChanged
        } else if changes.layout {
            // Layout changed - update props and need relayout
            self.update_render_props_in_place(element, root_id);
            UpdateResult::LayoutChanged
        } else if changes.visual || changes.handlers {
            // Only visual/handler changes - update props in place, no layout needed
            self.update_render_props_in_place(element, root_id);
            UpdateResult::VisualOnly
        } else {
            // No changes detected (shouldn't happen if tree hash differed)
            UpdateResult::NoChanges
        }
    }

    /// Rebuild subtrees for nodes with changed children
    ///
    /// This walks the tree comparing stored hashes with the new element tree.
    /// When it finds a node whose children have changed (different count),
    /// it rebuilds that subtree in place.
    fn rebuild_changed_subtrees<E: ElementBuilder>(&mut self, element: &E, node_id: LayoutNodeId) {
        let child_node_ids = self.layout_tree.children(node_id);
        let child_builders = element.children_builders();

        // Check if children count changed - rebuild children of this node
        if child_node_ids.len() != child_builders.len() {
            self.rebuild_children_in_place(node_id, child_builders);
            return;
        }

        // Same child count - check each child for deeper changes
        for (child_builder, &child_node_id) in child_builders.iter().zip(child_node_ids.iter()) {
            // Get stored hash for this child
            if let Some(&(_, stored_tree_hash)) = self.node_hashes.get(&child_node_id) {
                let new_tree_hash = DivHash::compute_element_tree(child_builder.as_ref());
                if stored_tree_hash != new_tree_hash {
                    // Child's subtree changed - check if it's the child count or deeper changes
                    let child_children_count = self.layout_tree.children(child_node_id).len();
                    let new_children_count = child_builder.children_builders().len();

                    if child_children_count != new_children_count {
                        // This child's children changed - rebuild its children
                        self.rebuild_children_in_place(
                            child_node_id,
                            child_builder.children_builders(),
                        );
                    } else {
                        // Recurse to find deeper changes
                        self.rebuild_changed_subtrees_boxed(child_builder.as_ref(), child_node_id);
                    }
                }
            }
        }
    }

    /// Rebuild subtrees for boxed element builder
    fn rebuild_changed_subtrees_boxed(
        &mut self,
        element: &dyn ElementBuilder,
        node_id: LayoutNodeId,
    ) {
        let child_node_ids = self.layout_tree.children(node_id);
        let child_builders = element.children_builders();

        if child_node_ids.len() != child_builders.len() {
            self.rebuild_children_in_place(node_id, child_builders);
            return;
        }

        for (child_builder, &child_node_id) in child_builders.iter().zip(child_node_ids.iter()) {
            if let Some(&(_, stored_tree_hash)) = self.node_hashes.get(&child_node_id) {
                let new_tree_hash = DivHash::compute_element_tree(child_builder.as_ref());
                if stored_tree_hash != new_tree_hash {
                    let child_children_count = self.layout_tree.children(child_node_id).len();
                    let new_children_count = child_builder.children_builders().len();

                    if child_children_count != new_children_count {
                        self.rebuild_children_in_place(
                            child_node_id,
                            child_builder.children_builders(),
                        );
                    } else {
                        self.rebuild_changed_subtrees_boxed(child_builder.as_ref(), child_node_id);
                    }
                }
            }
        }
    }

    /// Rebuild children of a node in place
    ///
    /// This removes old children and builds new ones from the provided element builders.
    fn rebuild_children_in_place(
        &mut self,
        parent_id: LayoutNodeId,
        new_children: &[Box<dyn ElementBuilder>],
    ) {
        // Remove old children
        let old_children = self.layout_tree.children(parent_id);
        for child_id in &old_children {
            self.remove_subtree_nodes(*child_id);
        }
        self.layout_tree.clear_children(parent_id);

        // Build new children
        for child in new_children {
            let child_id = child.build(&mut self.layout_tree);
            self.layout_tree.add_child(parent_id, child_id);
            self.collect_render_props_boxed(child.as_ref(), child_id);
        }
    }

    /// Analyze what categories of changes occurred between stored tree and new element
    fn analyze_changes<E: ElementBuilder>(
        &self,
        element: &E,
        node_id: LayoutNodeId,
    ) -> ChangeCategory {
        let mut changes = ChangeCategory::none();

        // Get stored hash for this node
        let Some(&(stored_own_hash, stored_tree_hash)) = self.node_hashes.get(&node_id) else {
            // No stored hash - treat as everything changed
            changes.layout = true;
            changes.visual = true;
            changes.children = true;
            return changes;
        };

        // Compute new hashes
        let new_own_hash = DivHash::compute_element(element);
        let new_tree_hash = DivHash::compute_element_tree(element);

        // If tree hashes match, nothing changed in this subtree
        if stored_tree_hash == new_tree_hash {
            return changes;
        }

        // Tree hash differs - analyze further
        if stored_own_hash != new_own_hash {
            // This node's own properties changed
            // Check render props to distinguish visual vs layout
            if let Some(old_render_node) = self.render_nodes.get(&node_id) {
                let new_props = element.render_props();
                let old_props = &old_render_node.props;

                // Visual change detection: compare render-only properties
                if !Self::props_visually_equal(old_props, &new_props) {
                    changes.visual = true;
                }

                // Layout change: if hash differs but not just visual, assume layout changed
                // (We can't access Style directly from ElementBuilder, so we infer)
                if !changes.visual {
                    changes.layout = true;
                }
            } else {
                // No old render node - everything changed
                changes.layout = true;
                changes.visual = true;
            }
        }

        // Check children
        let child_node_ids = self.layout_tree.children(node_id);
        let child_builders = element.children_builders();

        // Different number of children = structural change
        if child_node_ids.len() != child_builders.len() {
            changes.children = true;
            return changes;
        }

        // Recursively check children
        for (child_builder, &child_node_id) in child_builders.iter().zip(child_node_ids.iter()) {
            let child_changes = self.analyze_changes_boxed(child_builder.as_ref(), child_node_id);
            changes.layout = changes.layout || child_changes.layout;
            changes.visual = changes.visual || child_changes.visual;
            changes.children = changes.children || child_changes.children;
            changes.handlers = changes.handlers || child_changes.handlers;

            // Short circuit if children changed (need full rebuild anyway)
            if changes.children {
                return changes;
            }
        }

        changes
    }

    /// Analyze changes for a boxed element builder
    fn analyze_changes_boxed(
        &self,
        element: &dyn ElementBuilder,
        node_id: LayoutNodeId,
    ) -> ChangeCategory {
        let mut changes = ChangeCategory::none();

        let Some(&(stored_own_hash, stored_tree_hash)) = self.node_hashes.get(&node_id) else {
            changes.layout = true;
            changes.visual = true;
            changes.children = true;
            return changes;
        };

        let new_own_hash = DivHash::compute_element(element);
        let new_tree_hash = DivHash::compute_element_tree(element);

        if stored_tree_hash == new_tree_hash {
            return changes;
        }

        if stored_own_hash != new_own_hash {
            if let Some(old_render_node) = self.render_nodes.get(&node_id) {
                let new_props = element.render_props();
                let old_props = &old_render_node.props;

                if !Self::props_visually_equal(old_props, &new_props) {
                    changes.visual = true;
                }
                if !changes.visual {
                    changes.layout = true;
                }
            } else {
                changes.layout = true;
                changes.visual = true;
            }
        }

        let child_node_ids = self.layout_tree.children(node_id);
        let child_builders = element.children_builders();

        if child_node_ids.len() != child_builders.len() {
            changes.children = true;
            return changes;
        }

        for (child_builder, &child_node_id) in child_builders.iter().zip(child_node_ids.iter()) {
            let child_changes = self.analyze_changes_boxed(child_builder.as_ref(), child_node_id);
            changes.layout = changes.layout || child_changes.layout;
            changes.visual = changes.visual || child_changes.visual;
            changes.children = changes.children || child_changes.children;
            changes.handlers = changes.handlers || child_changes.handlers;

            if changes.children {
                return changes;
            }
        }

        changes
    }

    /// Compare render props for visual equality
    fn props_visually_equal(old: &RenderProps, new: &RenderProps) -> bool {
        render_props_eq(old, new)
    }

    /// Update render props in place without rebuilding the tree
    fn update_render_props_in_place<E: ElementBuilder>(
        &mut self,
        element: &E,
        node_id: LayoutNodeId,
    ) {
        // Update this node's props
        if let Some(render_node) = self.render_nodes.get_mut(&node_id) {
            let mut new_props = element.render_props();
            new_props.node_id = Some(node_id);
            // Preserve motion from old props (set by parent)
            new_props.motion = render_node.props.motion.clone();
            render_node.props = new_props;
        } else {
            // Render node doesn't exist - create it
            tracing::debug!(
                "update_render_props_in_place: creating missing render_node for {:?}",
                node_id
            );
            let mut new_props = element.render_props();
            new_props.node_id = Some(node_id);
            let element_type = Self::determine_element_type(element);
            self.render_nodes.insert(
                node_id,
                RenderNode {
                    props: new_props,
                    element_type,
                },
            );
        }

        // Update taffy node's layout style if element provides one
        // This is critical for layout changes (width, height, padding, etc.)
        if let Some(style) = element.layout_style() {
            self.layout_tree.set_style(node_id, style.clone());
        }

        // Update stored hash
        let own_hash = DivHash::compute_element(element);
        let tree_hash = DivHash::compute_element_tree(element);
        self.node_hashes.insert(node_id, (own_hash, tree_hash));

        // Update event handlers
        if let Some(handlers) = element.event_handlers() {
            self.handler_registry.register(node_id, handlers.clone());
        }

        // Update scroll physics if this is a scroll element
        if let Some(physics) = element.scroll_physics() {
            // Set the animation scheduler for bounce springs
            if let Some(scheduler) = self.animations.upgrade() {
                physics.lock().unwrap().set_scheduler(&scheduler);
            }
            self.scroll_physics.insert(node_id, physics);
        }

        // Update motion bindings if this element has continuous animations
        if let Some(bindings) = element.motion_bindings() {
            self.motion_bindings.insert(node_id, bindings);
        }

        // Register layout bounds storage if element wants bounds updates
        self.register_element_bounds_storage(node_id, element);

        // Recursively update children
        let child_node_ids = self.layout_tree.children(node_id);
        let child_builders = element.children_builders();

        // Handle mismatch between layout children and builder children
        if child_node_ids.len() != child_builders.len() {
            // Rebuild children in place to fix the mismatch
            self.rebuild_children_in_place(node_id, child_builders);
        } else {
            for (child_builder, &child_node_id) in child_builders.iter().zip(child_node_ids.iter())
            {
                self.update_render_props_in_place_boxed(child_builder.as_ref(), child_node_id);
            }
        }
    }

    /// Update render props for a boxed element builder
    fn update_render_props_in_place_boxed(
        &mut self,
        element: &dyn ElementBuilder,
        node_id: LayoutNodeId,
    ) {
        if let Some(render_node) = self.render_nodes.get_mut(&node_id) {
            let mut new_props = element.render_props();
            new_props.node_id = Some(node_id);
            new_props.motion = render_node.props.motion.clone();
            render_node.props = new_props;
        } else {
            // Render node doesn't exist - this can happen if the tree structure changed
            // but rebuild_children_in_place wasn't called for this subtree.
            // Create a new render node entry.
            tracing::debug!(
                "update_render_props_in_place_boxed: creating missing render_node for {:?}",
                node_id
            );
            let mut new_props = element.render_props();
            new_props.node_id = Some(node_id);
            let element_type = Self::determine_element_type_boxed(element);
            self.render_nodes.insert(
                node_id,
                RenderNode {
                    props: new_props,
                    element_type,
                },
            );
        }

        // Update taffy node's layout style if element provides one
        // This is critical for layout changes (width, height, padding, etc.)
        if let Some(style) = element.layout_style() {
            self.layout_tree.set_style(node_id, style.clone());
        }

        let own_hash = DivHash::compute_element(element);
        let tree_hash = DivHash::compute_element_tree(element);
        self.node_hashes.insert(node_id, (own_hash, tree_hash));

        if let Some(handlers) = element.event_handlers() {
            self.handler_registry.register(node_id, handlers.clone());
        }

        // Update scroll physics if this is a scroll element
        if let Some(physics) = element.scroll_physics() {
            // Set the animation scheduler for bounce springs
            if let Some(scheduler) = self.animations.upgrade() {
                physics.lock().unwrap().set_scheduler(&scheduler);
            }
            self.scroll_physics.insert(node_id, physics);
        }

        // Update motion bindings if this element has continuous animations
        if let Some(bindings) = element.motion_bindings() {
            self.motion_bindings.insert(node_id, bindings);
        }

        // Register layout bounds storage if element wants bounds updates
        self.register_element_bounds_storage(node_id, element);

        let child_node_ids = self.layout_tree.children(node_id);
        let child_builders = element.children_builders();

        // Handle mismatch between layout children and builder children
        if child_node_ids.len() != child_builders.len() {
            // Rebuild children in place to fix the mismatch
            self.rebuild_children_in_place(node_id, child_builders);
        } else {
            for (child_builder, &child_node_id) in child_builders.iter().zip(child_node_ids.iter())
            {
                self.update_render_props_in_place_boxed(child_builder.as_ref(), child_node_id);
            }
        }
    }

    /// Set the DPI scale factor for this render tree
    ///
    /// This scales all layout positions and sizes by the given factor
    /// before rendering. Use this for HiDPI/Retina display support.
    ///
    /// # Arguments
    /// * `scale_factor` - The scale factor (1.0 = no scaling, 2.0 = 2x DPI)
    pub fn set_scale_factor(&mut self, scale_factor: f32) {
        self.scale_factor = scale_factor;
    }

    /// Get the current scale factor
    pub fn scale_factor(&self) -> f32 {
        self.scale_factor
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

        // Check for CSS animation from stylesheet if element has an ID
        // Only apply if no motion animation is already set (motion container takes precedence)
        if props.motion.is_none() {
            if let Some(ref stylesheet) = self.stylesheet {
                if let Some(id) = element.element_id() {
                    if let Some(motion) = stylesheet.resolve_animation(id) {
                        props.motion = Some(motion);
                        tracing::trace!(
                            "Applied CSS animation from stylesheet for element #{} ({:?})",
                            id,
                            node_id
                        );
                    }
                }
            }
        }

        // Determine element type using the trait methods
        let element_type = Self::determine_element_type(element);

        self.render_nodes.insert(
            node_id,
            RenderNode {
                props,
                element_type,
            },
        );

        // Store per-node hashes for incremental update detection
        let own_hash = DivHash::compute_element(element);
        let tree_hash = DivHash::compute_element_tree(element);
        self.node_hashes.insert(node_id, (own_hash, tree_hash));

        // Register event handlers if present
        if let Some(handlers) = element.event_handlers() {
            self.handler_registry.register(node_id, handlers.clone());
        }

        // Store scroll physics if this is a scroll element
        if let Some(physics) = element.scroll_physics() {
            // Set the animation scheduler for bounce springs
            if let Some(scheduler) = self.animations.upgrade() {
                physics.lock().unwrap().set_scheduler(&scheduler);
            }
            self.scroll_physics.insert(node_id, physics);
        }

        // Store motion bindings if this element has continuous animations
        if let Some(bindings) = element.motion_bindings() {
            self.motion_bindings.insert(node_id, bindings);
        }

        // Register layout bounds storage if element wants bounds updates
        self.register_element_bounds_storage(node_id, element);

        // Register element ID if present (for selector API)
        if let Some(id) = element.element_id() {
            self.element_registry.register(id, node_id);
        }

        // Bind ScrollRef if present (for scroll containers)
        if let Some(scroll_ref) = element.bound_scroll_ref() {
            self.register_scroll_ref(node_id, scroll_ref);
        }

        // Get child node IDs from the layout tree
        let child_node_ids = self.layout_tree.children(node_id);
        let child_builders = element.children_builders();

        // Log mismatch to help debug stateful/motion issues (in collect_render_props)
        if child_node_ids.len() != child_builders.len() && !child_node_ids.is_empty() {
            tracing::warn!(
                "collect_render_props: node {:?} has {} layout children but {} builder children (mismatch!)",
                node_id, child_node_ids.len(), child_builders.len()
            );
        }

        // Match children by index (they were built in order)
        for (child_builder, &child_node_id) in child_builders.iter().zip(child_node_ids.iter()) {
            self.collect_render_props_boxed(child_builder.as_ref(), child_node_id);
        }
    }

    /// Collect render props from a boxed element builder
    fn collect_render_props_boxed(&mut self, element: &dyn ElementBuilder, node_id: LayoutNodeId) {
        let mut props = element.render_props();
        props.node_id = Some(node_id);

        // Check for CSS animation from stylesheet if element has an ID
        // Only apply if no motion animation is already set (motion container takes precedence)
        if props.motion.is_none() {
            if let Some(ref stylesheet) = self.stylesheet {
                if let Some(id) = element.element_id() {
                    if let Some(motion) = stylesheet.resolve_animation(id) {
                        props.motion = Some(motion);
                        tracing::trace!(
                            "Applied CSS animation from stylesheet for element #{} ({:?})",
                            id,
                            node_id
                        );
                    }
                }
            }
        }

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
                        italic: info.italic,
                        v_align: info.v_align,
                        wrap: info.wrap,
                        line_height: info.line_height,
                        measured_width: info.measured_width,
                        font_family: info.font_family,
                        word_spacing: info.word_spacing,
                        ascender: info.ascender,
                        strikethrough: info.strikethrough,
                        underline: info.underline,
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
                        loading_strategy: info.loading_strategy,
                        placeholder_type: info.placeholder_type,
                        placeholder_color: info.placeholder_color,
                    })
                } else {
                    ElementType::Div
                }
            }
            ElementTypeId::Canvas => ElementType::Canvas(CanvasData {
                render_fn: element.canvas_render_info(),
            }),
            ElementTypeId::StyledText => {
                if let Some(info) = element.styled_text_render_info() {
                    ElementType::StyledText(StyledTextData {
                        content: info.content,
                        spans: info
                            .spans
                            .into_iter()
                            .map(|s| StyledTextSpan {
                                start: s.start,
                                end: s.end,
                                color: s.color,
                                bold: s.bold,
                                italic: s.italic,
                                underline: s.underline,
                                strikethrough: s.strikethrough,
                                link_url: s.link_url,
                            })
                            .collect(),
                        default_color: info.default_color,
                        font_size: info.font_size,
                        align: info.align,
                        v_align: info.v_align,
                        font_family: info.font_family,
                        line_height: info.line_height,
                        weight: info.weight,
                        italic: info.italic,
                        ascender: info.ascender,
                    })
                } else {
                    ElementType::Div
                }
            }
            ElementTypeId::Div => ElementType::Div,
            ElementTypeId::Motion => ElementType::Div, // Motion is a transparent container
        };

        self.render_nodes.insert(
            node_id,
            RenderNode {
                props,
                element_type,
            },
        );

        // Store per-node hashes for incremental update detection
        let own_hash = DivHash::compute_element(element);
        let tree_hash = DivHash::compute_element_tree(element);
        self.node_hashes.insert(node_id, (own_hash, tree_hash));

        // Register event handlers if present
        if let Some(handlers) = element.event_handlers() {
            self.handler_registry.register(node_id, handlers.clone());
        }

        // Store scroll physics if this is a scroll element
        if let Some(physics) = element.scroll_physics() {
            // Set the animation scheduler for bounce springs
            if let Some(scheduler) = self.animations.upgrade() {
                physics.lock().unwrap().set_scheduler(&scheduler);
            }
            self.scroll_physics.insert(node_id, physics);
        }

        // Store motion bindings if this element has continuous animations
        if let Some(bindings) = element.motion_bindings() {
            self.motion_bindings.insert(node_id, bindings);
        }

        // Register layout bounds storage if element wants bounds updates
        self.register_element_bounds_storage(node_id, element);

        // Register element ID if present (for selector API)
        if let Some(id) = element.element_id() {
            self.element_registry.register(id, node_id);
        }

        // Bind ScrollRef if present (for scroll containers)
        if let Some(scroll_ref) = element.bound_scroll_ref() {
            self.register_scroll_ref(node_id, scroll_ref);
        }

        // Get child node IDs from the layout tree
        let child_node_ids = self.layout_tree.children(node_id);
        let child_builders = element.children_builders();

        // Debug: warn on mismatch (in collect_render_props_boxed)
        if child_node_ids.len() != child_builders.len() {
            tracing::warn!(
                "collect_render_props_boxed: node {:?} has {} layout children but {} builder children",
                node_id,
                child_node_ids.len(),
                child_builders.len()
            );
        }

        // Check if this is a Motion container
        let is_motion = element.element_type_id() == ElementTypeId::Motion;
        // Get stable ID from Motion container (for overlay animations that survive tree rebuilds)
        let motion_stable_id = if is_motion {
            element.motion_stable_id().map(|s| s.to_string())
        } else {
            None
        };
        // Get replay and exiting flags from Motion container
        let motion_should_replay = if is_motion {
            element.motion_should_replay()
        } else {
            false
        };
        // DEPRECATED: motion_is_exiting is no longer used for triggering exit.
        // Motion exit is now triggered explicitly via MotionHandle.exit().
        // This field is kept for backwards compatibility but always false.
        #[allow(deprecated)]
        let motion_is_exiting = if is_motion {
            element.motion_is_exiting()
        } else {
            false
        };

        // Match children by index (they were built in order)
        for (index, (child_builder, &child_node_id)) in
            child_builders.iter().zip(child_node_ids.iter()).enumerate()
        {
            // If parent is Motion, propagate motion animation to child
            if is_motion {
                if let Some(motion_config) = element.motion_animation_for_child(index) {
                    // Append child index to stable key for unique stagger animations
                    let child_stable_id = motion_stable_id
                        .as_ref()
                        .map(|key| format!("{}:child:{}", key, index));
                    self.collect_render_props_boxed_with_motion(
                        child_builder.as_ref(),
                        child_node_id,
                        Some(motion_config),
                        child_stable_id,
                        motion_should_replay,
                        motion_is_exiting,
                    );
                    continue;
                }
            }
            self.collect_render_props_boxed(child_builder.as_ref(), child_node_id);
        }
    }

    /// Collect render props with motion animation config from parent
    #[allow(deprecated)]
    fn collect_render_props_boxed_with_motion(
        &mut self,
        element: &dyn ElementBuilder,
        node_id: LayoutNodeId,
        motion_config: Option<crate::element::MotionAnimation>,
        motion_stable_id: Option<String>,
        motion_should_replay: bool,
        motion_is_exiting: bool,
    ) {
        let mut props = element.render_props();
        props.node_id = Some(node_id);

        // Motion config from parent takes precedence
        if motion_config.is_some() {
            props.motion = motion_config;
            props.motion_stable_id = motion_stable_id.clone();
            props.motion_should_replay = motion_should_replay;
            // DEPRECATED: motion_is_exiting is no longer used for triggering exit.
            // Motion exit is now triggered explicitly via MotionHandle.exit().
            props.motion_is_exiting = motion_is_exiting;

            // Queue replay with the CHILD's stable key (includes :child:N suffix)
            // This ensures replay uses the same key as initialize_motion_animations
            if motion_should_replay {
                if let Some(ref key) = motion_stable_id {
                    crate::render_state::queue_global_motion_replay(key.clone());
                }
            }
        } else if props.motion.is_none() {
            // Fall back to CSS animation from stylesheet if element has an ID
            if let Some(ref stylesheet) = self.stylesheet {
                if let Some(id) = element.element_id() {
                    if let Some(motion) = stylesheet.resolve_animation(id) {
                        props.motion = Some(motion);
                        tracing::trace!(
                            "Applied CSS animation from stylesheet for element #{} ({:?})",
                            id,
                            node_id
                        );
                    }
                }
            }
        }

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
                        italic: info.italic,
                        v_align: info.v_align,
                        wrap: info.wrap,
                        line_height: info.line_height,
                        measured_width: info.measured_width,
                        font_family: info.font_family,
                        word_spacing: info.word_spacing,
                        ascender: info.ascender,
                        strikethrough: info.strikethrough,
                        underline: info.underline,
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
                        loading_strategy: info.loading_strategy,
                        placeholder_type: info.placeholder_type,
                        placeholder_color: info.placeholder_color,
                    })
                } else {
                    ElementType::Div
                }
            }
            ElementTypeId::Canvas => ElementType::Canvas(CanvasData {
                render_fn: element.canvas_render_info(),
            }),
            ElementTypeId::StyledText => {
                if let Some(info) = element.styled_text_render_info() {
                    ElementType::StyledText(StyledTextData {
                        content: info.content,
                        spans: info
                            .spans
                            .into_iter()
                            .map(|s| StyledTextSpan {
                                start: s.start,
                                end: s.end,
                                color: s.color,
                                bold: s.bold,
                                italic: s.italic,
                                underline: s.underline,
                                strikethrough: s.strikethrough,
                                link_url: s.link_url,
                            })
                            .collect(),
                        default_color: info.default_color,
                        font_size: info.font_size,
                        align: info.align,
                        v_align: info.v_align,
                        font_family: info.font_family,
                        line_height: info.line_height,
                        weight: info.weight,
                        italic: info.italic,
                        ascender: info.ascender,
                    })
                } else {
                    ElementType::Div
                }
            }
            ElementTypeId::Div => ElementType::Div,
            ElementTypeId::Motion => ElementType::Div,
        };

        self.render_nodes.insert(
            node_id,
            RenderNode {
                props,
                element_type,
            },
        );

        // Store per-node hashes for incremental update detection
        let own_hash = DivHash::compute_element(element);
        let tree_hash = DivHash::compute_element_tree(element);
        self.node_hashes.insert(node_id, (own_hash, tree_hash));

        // Register event handlers if present
        if let Some(handlers) = element.event_handlers() {
            self.handler_registry.register(node_id, handlers.clone());
        }

        // Store scroll physics if this is a scroll element
        if let Some(physics) = element.scroll_physics() {
            // Set the animation scheduler for bounce springs
            if let Some(scheduler) = self.animations.upgrade() {
                physics.lock().unwrap().set_scheduler(&scheduler);
            }
            self.scroll_physics.insert(node_id, physics);
        }

        // Store motion bindings if this element has continuous animations
        if let Some(bindings) = element.motion_bindings() {
            self.motion_bindings.insert(node_id, bindings);
        }

        // Register layout bounds storage if element wants bounds updates
        self.register_element_bounds_storage(node_id, element);

        // Register element ID if present (for selector API)
        if let Some(id) = element.element_id() {
            self.element_registry.register(id, node_id);
        }

        // Bind ScrollRef if present (for scroll containers)
        if let Some(scroll_ref) = element.bound_scroll_ref() {
            self.register_scroll_ref(node_id, scroll_ref);
        }

        // Recursively process children (without motion - motion only applies to direct children)
        let child_node_ids = self.layout_tree.children(node_id);
        let child_builders = element.children_builders();

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
                        italic: info.italic,
                        v_align: info.v_align,
                        wrap: info.wrap,
                        line_height: info.line_height,
                        measured_width: info.measured_width,
                        font_family: info.font_family,
                        word_spacing: info.word_spacing,
                        ascender: info.ascender,
                        strikethrough: info.strikethrough,
                        underline: info.underline,
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
                        loading_strategy: info.loading_strategy,
                        placeholder_type: info.placeholder_type,
                        placeholder_color: info.placeholder_color,
                    })
                } else {
                    ElementType::Div
                }
            }
            ElementTypeId::Canvas => ElementType::Canvas(CanvasData {
                render_fn: element.canvas_render_info(),
            }),
            ElementTypeId::StyledText => {
                if let Some(info) = element.styled_text_render_info() {
                    ElementType::StyledText(StyledTextData {
                        content: info.content,
                        spans: info
                            .spans
                            .into_iter()
                            .map(|s| StyledTextSpan {
                                start: s.start,
                                end: s.end,
                                color: s.color,
                                bold: s.bold,
                                italic: s.italic,
                                underline: s.underline,
                                strikethrough: s.strikethrough,
                                link_url: s.link_url,
                            })
                            .collect(),
                        default_color: info.default_color,
                        font_size: info.font_size,
                        align: info.align,
                        v_align: info.v_align,
                        font_family: info.font_family,
                        line_height: info.line_height,
                        weight: info.weight,
                        italic: info.italic,
                        ascender: info.ascender,
                    })
                } else {
                    ElementType::Div
                }
            }
            ElementTypeId::Div => ElementType::Div,
            ElementTypeId::Motion => ElementType::Div, // Motion is a transparent container
        }
    }

    /// Determine element type from a boxed element builder
    fn determine_element_type_boxed(element: &dyn ElementBuilder) -> ElementType {
        match element.element_type_id() {
            ElementTypeId::Text => {
                if let Some(info) = element.text_render_info() {
                    ElementType::Text(TextData {
                        content: info.content,
                        font_size: info.font_size,
                        color: info.color,
                        align: info.align,
                        weight: info.weight,
                        italic: info.italic,
                        v_align: info.v_align,
                        wrap: info.wrap,
                        line_height: info.line_height,
                        measured_width: info.measured_width,
                        font_family: info.font_family,
                        word_spacing: info.word_spacing,
                        ascender: info.ascender,
                        strikethrough: info.strikethrough,
                        underline: info.underline,
                    })
                } else {
                    ElementType::Div
                }
            }
            ElementTypeId::StyledText => {
                if let Some(info) = element.styled_text_render_info() {
                    ElementType::StyledText(StyledTextData {
                        content: info.content,
                        spans: info
                            .spans
                            .into_iter()
                            .map(|s| StyledTextSpan {
                                start: s.start,
                                end: s.end,
                                color: s.color,
                                bold: s.bold,
                                italic: s.italic,
                                underline: s.underline,
                                strikethrough: s.strikethrough,
                                link_url: s.link_url,
                            })
                            .collect(),
                        default_color: info.default_color,
                        font_size: info.font_size,
                        align: info.align,
                        v_align: info.v_align,
                        font_family: info.font_family,
                        line_height: info.line_height,
                        weight: info.weight,
                        italic: info.italic,
                        ascender: info.ascender,
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
                        loading_strategy: info.loading_strategy,
                        placeholder_type: info.placeholder_type,
                        placeholder_color: info.placeholder_color,
                    })
                } else {
                    ElementType::Div
                }
            }
            ElementTypeId::Canvas => ElementType::Canvas(CanvasData {
                render_fn: element.canvas_render_info(),
            }),
            ElementTypeId::Div => ElementType::Div,
            ElementTypeId::Motion => ElementType::Div,
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

            // Update registered layout bounds storages
            self.update_layout_bounds_storages();

            // Cache element bounds for ElementHandle.bounds() queries
            self.cache_element_bounds();

            // Process on_ready callbacks for newly laid out elements
            self.process_on_ready_callbacks();
        }
    }

    /// Cache element bounds for all elements with string IDs
    ///
    /// This populates the ElementRegistry's bounds cache so that
    /// `ElementHandle.bounds()` can return computed bounds.
    fn cache_element_bounds(&self) {
        // Clear the previous cache
        self.element_registry.clear_bounds();

        // Iterate through all render nodes and cache bounds for those with string IDs
        for (node_id, _render_node) in &self.render_nodes {
            if let Some(string_id) = self.element_registry.get_id(*node_id) {
                if let Some(bounds) = self.get_bounds(*node_id) {
                    self.element_registry.update_bounds(
                        &string_id,
                        blinc_core::Bounds::new(bounds.x, bounds.y, bounds.width, bounds.height),
                    );
                }
            }
        }
    }

    /// Register a layout bounds storage for a node
    ///
    /// After layout is computed, the storage will be updated with the node's
    /// computed bounds. This allows elements to react to layout changes.
    pub fn register_layout_bounds_storage(
        &mut self,
        node_id: LayoutNodeId,
        storage: LayoutBoundsStorage,
    ) {
        self.layout_bounds_storages.insert(
            node_id,
            LayoutBoundsEntry {
                storage,
                on_change: None,
            },
        );
    }

    /// Register a layout bounds storage with a change callback
    ///
    /// The callback is invoked when the computed bounds change (width or height differ).
    /// This is useful for elements that need to react to layout changes, like TextInput
    /// which needs to recalculate scroll offset when its width changes.
    pub fn register_layout_bounds_storage_with_callback(
        &mut self,
        node_id: LayoutNodeId,
        storage: LayoutBoundsStorage,
        on_change: LayoutBoundsCallback,
    ) {
        self.layout_bounds_storages.insert(
            node_id,
            LayoutBoundsEntry {
                storage,
                on_change: Some(on_change),
            },
        );
    }

    /// Unregister a layout bounds storage
    pub fn unregister_layout_bounds_storage(&mut self, node_id: LayoutNodeId) {
        self.layout_bounds_storages.remove(&node_id);
    }

    /// Register layout bounds storage from an element builder
    ///
    /// This helper checks both layout_bounds_storage() and layout_bounds_callback()
    /// from the ElementBuilder trait and registers them together.
    fn register_element_bounds_storage(
        &mut self,
        node_id: LayoutNodeId,
        element: &dyn ElementBuilder,
    ) {
        if let Some(storage) = element.layout_bounds_storage() {
            let callback = element.layout_bounds_callback();
            self.layout_bounds_storages.insert(
                node_id,
                LayoutBoundsEntry {
                    storage,
                    on_change: callback,
                },
            );
        }
    }

    /// Update all registered layout bounds storages after layout computation
    ///
    /// When bounds change (width or height differ), the on_change callback is invoked.
    fn update_layout_bounds_storages(&self) {
        for (&node_id, entry) in &self.layout_bounds_storages {
            if let Some(bounds) = self.layout_tree.get_bounds(node_id, (0.0, 0.0)) {
                let should_notify = if let Ok(mut guard) = entry.storage.lock() {
                    // Check if bounds changed (compare width and height)
                    let changed = match guard.as_ref() {
                        Some(old_bounds) => {
                            (old_bounds.width - bounds.width).abs() > 0.01
                                || (old_bounds.height - bounds.height).abs() > 0.01
                        }
                        None => true, // First time getting bounds
                    };
                    *guard = Some(bounds);
                    changed
                } else {
                    false
                };

                // Invoke callback if bounds changed and callback exists
                if should_notify {
                    if let Some(ref callback) = entry.on_change {
                        callback(bounds);
                    }
                }
            }
        }
    }

    /// Clear all layout bounds storages to force fresh calculations
    ///
    /// This should be called on window resize to ensure that cached bounds
    /// don't influence the new layout computation. Each element will get
    /// fresh bounds on the next `compute_layout` call.
    pub fn clear_layout_bounds_storages(&self) {
        for (_, entry) in &self.layout_bounds_storages {
            if let Ok(mut guard) = entry.storage.lock() {
                *guard = None;
            }
        }
    }

    // =========================================================================
    // On-Ready Callbacks
    // =========================================================================

    /// Process all pending on_ready callbacks
    ///
    /// This is called after layout computation. Each callback is invoked with
    /// the element's computed bounds, then marked as triggered so it won't
    /// fire again on subsequent layouts.
    ///
    /// Callbacks registered via the query API (ElementHandle.on_ready()) are
    /// tracked by string ID for stability across tree rebuilds.
    ///
    /// Callbacks are invoked after a short delay (200ms) to allow the window
    /// to finish resizing/animating on platforms like macOS where fullscreen
    /// transitions cause rapid resize events.
    fn process_on_ready_callbacks(&mut self) {
        // Pick up any pending callbacks from the registry (via query API)
        // These are already keyed by string ID for stable tracking
        let pending_from_registry = self.element_registry.take_pending_on_ready();
        for (string_id, callback) in pending_from_registry {
            // Only add if not already registered (avoid duplicates)
            if !self.on_ready_callbacks.contains_key(&string_id) {
                self.on_ready_callbacks.insert(
                    string_id,
                    OnReadyEntry {
                        callback,
                        triggered: false,
                    },
                );
            }
        }

        // Collect callbacks that need invocation
        // Look up node_id from string_id via registry for bounds lookup
        let registry = self.element_registry.clone();
        let to_trigger: Vec<(String, OnReadyCallback, ElementBounds)> = self
            .on_ready_callbacks
            .iter()
            .filter(|(_, entry)| !entry.triggered)
            .filter_map(|(string_id, entry)| {
                // Look up node_id from string_id
                let node_id = registry.get(string_id)?;

                self.layout_tree
                    .get_bounds(node_id, (0.0, 0.0))
                    .map(|bounds| (string_id.clone(), entry.callback.clone(), bounds))
            })
            .collect();

        // Mark as triggered before invoking (in case callback triggers rebuild)
        // Also mark in the registry for cross-rebuild deduplication
        for (string_id, _, _) in &to_trigger {
            if let Some(entry) = self.on_ready_callbacks.get_mut(string_id) {
                entry.triggered = true;
            }
            self.element_registry.mark_on_ready_triggered(string_id);
        }

        // Invoke callbacks with bounds after a delay
        // The delay allows window resize/fullscreen animations to complete
        // so that triggered animations are visible to the user
        if !to_trigger.is_empty() {
            std::thread::spawn(move || {
                // Magic delay to let the window settle
                std::thread::sleep(std::time::Duration::from_millis(200));

                for (string_id, callback, bounds) in to_trigger {
                    tracing::trace!("on_ready callback invoked for '{}'", string_id);
                    callback(bounds);
                }
            });
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
            let (content_width, content_height) = self
                .layout_tree
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
                        "Scroll physics updated: viewport=({:.0}, {:.0}) content=({:.0}, {:.0}) max_offset=({:.0}, {:.0}) direction={:?}",
                        viewport_width, viewport_height, content_width, content_height, p.max_offset_x(), p.max_offset_y(), p.config.direction
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

    /// Get the element registry for ID-based lookups
    pub fn element_registry(&self) -> &Arc<ElementRegistry> {
        &self.element_registry
    }

    /// Query an element by ID
    ///
    /// Returns the node ID if an element with the given ID exists.
    pub fn query_by_id(&self, id: &str) -> Option<LayoutNodeId> {
        self.element_registry.get(id)
    }

    /// Get a bound ScrollRef by node ID
    pub fn scroll_ref(&self, node_id: LayoutNodeId) -> Option<&ScrollRef> {
        self.scroll_refs.get(&node_id)
    }

    /// Register a ScrollRef for a scroll container node
    ///
    /// This binds the ScrollRef to the node and adds it to both the node-keyed
    /// HashMap (for quick lookup) and the active_scroll_refs Vec (for persistence
    /// across rebuilds).
    fn register_scroll_ref(&mut self, node_id: LayoutNodeId, scroll_ref: &ScrollRef) {
        scroll_ref.bind_to_node(node_id, Arc::downgrade(&self.element_registry));
        self.scroll_refs.insert(node_id, scroll_ref.clone());
        // Also track in active_scroll_refs for persistence across rebuilds
        // Check if already present by comparing inner pointer
        let inner_ptr = Arc::as_ptr(&scroll_ref.inner());
        if !self
            .active_scroll_refs
            .iter()
            .any(|sr| Arc::as_ptr(&sr.inner()) == inner_ptr)
        {
            self.active_scroll_refs.push(scroll_ref.clone());
        }
    }

    /// Process all pending scroll operations from bound ScrollRefs
    ///
    /// This should be called each frame before rendering to apply any
    /// programmatic scroll commands (scroll_to, scroll_to_bottom, etc.).
    ///
    /// Returns true if any scroll state was modified.
    pub fn process_pending_scroll_refs(&mut self) -> bool {
        use crate::selector::PendingScroll;

        let mut any_modified = false;

        // Collect scroll refs that have pending operations from active_scroll_refs
        // (active_scroll_refs persists across rebuilds, unlike scroll_refs HashMap)
        let pending: Vec<_> = self
            .active_scroll_refs
            .iter()
            .filter_map(|scroll_ref| {
                let node_id = scroll_ref.node_id()?;
                scroll_ref
                    .take_pending_scroll()
                    .map(|pending| (node_id, pending))
            })
            .collect();
        for (node_id, pending_scroll) in pending {
            let Some(physics) = self.scroll_physics.get(&node_id) else {
                continue;
            };

            let mut physics = physics.lock().unwrap();
            any_modified = true;

            match pending_scroll {
                PendingScroll::ToOffset { x, y, smooth: _ } => {
                    // For now, instant scroll (smooth animation TBD)
                    physics.offset_x = -x;
                    physics.offset_y = -y;
                }
                PendingScroll::ByAmount { dx, dy, smooth: _ } => {
                    physics.apply_scroll_delta(dx, dy);
                }
                PendingScroll::ToTop { smooth: _ } => {
                    physics.offset_y = 0.0;
                }
                PendingScroll::ToBottom { smooth: _ } => {
                    physics.offset_y = physics.max_offset_y();
                }
                PendingScroll::ToElement {
                    element_id,
                    options,
                } => {
                    // Look up element bounds and scroll to make it visible
                    if let Some(target_node) = self.element_registry.get(&element_id) {
                        // Get target element's bounds
                        if let Some(target_bounds) = self.get_bounds(target_node) {
                            // Get scroll container's bounds
                            if let Some(container_bounds) = self.get_bounds(node_id) {
                                // Calculate scroll offset to bring element into view
                                // Element's position relative to scroll container
                                let relative_y = target_bounds.y - container_bounds.y;
                                let relative_x = target_bounds.x - container_bounds.x;

                                // Scroll to center the element (or just make it visible)
                                let viewport_height = physics.viewport_height;
                                let viewport_width = physics.viewport_width;

                                // Calculate target offsets
                                // Center vertically
                                let target_center_y =
                                    relative_y + target_bounds.height / 2.0 - viewport_height / 2.0;
                                let target_offset_y = (-target_center_y)
                                    .clamp(physics.max_offset_y(), physics.min_offset_y());

                                // Center horizontally
                                let target_center_x =
                                    relative_x + target_bounds.width / 2.0 - viewport_width / 2.0;
                                let target_offset_x = (-target_center_x)
                                    .clamp(physics.max_offset_x(), physics.min_offset_x());

                                // Use smooth animation if requested
                                if options.behavior == crate::selector::ScrollBehavior::Smooth {
                                    physics.scroll_to_animated(target_offset_x, target_offset_y);
                                } else {
                                    // Instant scroll
                                    physics.offset_y = target_offset_y;
                                    if matches!(
                                        physics.config.direction,
                                        crate::scroll::ScrollDirection::Horizontal
                                            | crate::scroll::ScrollDirection::Both
                                    ) {
                                        physics.offset_x = target_offset_x;
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Update ScrollRef with current state
            if let Some(scroll_ref) = self.scroll_refs.get(&node_id) {
                scroll_ref.update_state(
                    (physics.offset_x.abs(), physics.offset_y.abs()),
                    (physics.content_width, physics.content_height),
                    (physics.viewport_width, physics.viewport_height),
                );
            }
        }

        any_modified
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
    #[allow(clippy::too_many_arguments)]
    pub fn dispatch_event_with_local(
        &mut self,
        node_id: LayoutNodeId,
        event_type: blinc_core::events::EventType,
        mouse_x: f32,
        mouse_y: f32,
        local_x: f32,
        local_y: f32,
        bounds_x: f32,
        bounds_y: f32,
        bounds_width: f32,
        bounds_height: f32,
    ) {
        self.dispatch_event_full(
            node_id,
            event_type,
            mouse_x,
            mouse_y,
            local_x,
            local_y,
            bounds_x,
            bounds_y,
            bounds_width,
            bounds_height,
            0.0,
            0.0,
        );
    }

    /// Dispatch an event with all context data including drag delta
    ///
    /// This is the full dispatch method that includes drag_delta for DRAG events.
    #[allow(clippy::too_many_arguments)]
    pub fn dispatch_event_full(
        &mut self,
        node_id: LayoutNodeId,
        event_type: blinc_core::events::EventType,
        mouse_x: f32,
        mouse_y: f32,
        local_x: f32,
        local_y: f32,
        bounds_x: f32,
        bounds_y: f32,
        bounds_width: f32,
        bounds_height: f32,
        drag_delta_x: f32,
        drag_delta_y: f32,
    ) {
        let has_handler = self.handler_registry.has_handler(node_id, event_type);
        tracing::debug!(
            "dispatch_event_full: node={:?}, event_type={}, has_handler={}, drag_delta=({:.1}, {:.1})",
            node_id,
            event_type,
            has_handler,
            drag_delta_x,
            drag_delta_y
        );

        let ctx = crate::event_handler::EventContext::new(event_type, node_id)
            .with_mouse_pos(mouse_x, mouse_y)
            .with_local_pos(local_x, local_y)
            .with_bounds_pos(bounds_x, bounds_y)
            .with_bounds(bounds_width, bounds_height)
            .with_drag_delta(drag_delta_x, drag_delta_y);

        if has_handler {
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

    /// Broadcast a text input event to ALL text input handlers
    ///
    /// This is used when the router's focused node ID may be stale after a tree rebuild.
    /// Each text input handler checks its own internal focus state (`s.visual.is_focused()`)
    /// to determine if it should process the event.
    pub fn broadcast_text_input_event(
        &mut self,
        key_char: char,
        shift: bool,
        ctrl: bool,
        alt: bool,
        meta: bool,
    ) {
        let ctx = crate::event_handler::EventContext::new(
            blinc_core::events::event_types::TEXT_INPUT,
            crate::tree::LayoutNodeId::default(), // Will be overwritten per-node
        )
        .with_key_char(key_char)
        .with_modifiers(shift, ctrl, alt, meta);

        self.handler_registry
            .broadcast(blinc_core::events::event_types::TEXT_INPUT, &ctx);
    }

    /// Broadcast a key event to ALL key handlers
    ///
    /// This is used when the router's focused node ID may be stale after a tree rebuild.
    /// Each handler checks its own internal focus state to determine if it should process.
    pub fn broadcast_key_event(
        &mut self,
        event_type: blinc_core::events::EventType,
        key_code: u32,
        shift: bool,
        ctrl: bool,
        alt: bool,
        meta: bool,
    ) {
        let ctx = crate::event_handler::EventContext::new(
            event_type,
            crate::tree::LayoutNodeId::default(), // Will be overwritten per-node
        )
        .with_key_code(key_code)
        .with_modifiers(shift, ctrl, alt, meta);

        self.handler_registry.broadcast(event_type, &ctx);
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

    /// Dispatch scroll event through ancestor chain with consumption tracking
    ///
    /// For nested scrolls, inner scrolls consume delta for their direction,
    /// and outer scrolls only receive the remaining delta.
    ///
    /// - `hit_node`: The innermost node under the cursor
    /// - `ancestors`: The ancestor chain from root to hit_node
    /// - Returns the remaining delta after all consumption
    pub fn dispatch_scroll_chain(
        &mut self,
        hit_node: LayoutNodeId,
        ancestors: &[LayoutNodeId],
        mouse_x: f32,
        mouse_y: f32,
        mut delta_x: f32,
        mut delta_y: f32,
    ) -> (f32, f32) {
        // Build the chain from leaf to root (hit_node first, then ancestors in reverse)
        // ancestors is root to leaf, so we iterate in reverse and include hit_node
        let mut chain: Vec<LayoutNodeId> = vec![hit_node];
        for &ancestor in ancestors.iter().rev() {
            if ancestor != hit_node {
                chain.push(ancestor);
            }
        }

        tracing::trace!(
            "dispatch_scroll_chain: hit={:?}, chain_len={}, delta=({:.1}, {:.1})",
            hit_node,
            chain.len(),
            delta_x,
            delta_y
        );

        // Dispatch to each node in the chain
        for node_id in chain {
            // Skip if no remaining delta
            if delta_x.abs() < 0.001 && delta_y.abs() < 0.001 {
                break;
            }

            // Check if this node has a scroll handler
            let has_handler = self
                .handler_registry
                .has_handler(node_id, blinc_core::events::event_types::SCROLL);

            if !has_handler {
                continue;
            }

            // Get direction and check what this scroll can consume
            let direction = self.get_scroll_direction(node_id);
            let (can_consume_x, can_consume_y) = self.can_consume_scroll(node_id, delta_x, delta_y);

            // Determine if this scroll handles each axis (based on direction)
            // If no direction (custom scroll handler like TextArea), dispatch full delta
            let has_scroll_physics = direction.is_some();
            let handles_x = direction.map_or(true, |d| {
                matches!(
                    d,
                    crate::scroll::ScrollDirection::Horizontal
                        | crate::scroll::ScrollDirection::Both
                )
            });
            let handles_y = direction.map_or(true, |d| {
                matches!(
                    d,
                    crate::scroll::ScrollDirection::Vertical | crate::scroll::ScrollDirection::Both
                )
            });

            // Dispatch the remaining delta for axes this scroll handles
            let dispatch_x = if handles_x { delta_x } else { 0.0 };
            let dispatch_y = if handles_y { delta_y } else { 0.0 };

            tracing::trace!(
                "  node={:?}, direction={:?}, handles=({}, {}), can_consume=({}, {}), dispatch=({:.1}, {:.1})",
                node_id, direction, handles_x, handles_y, can_consume_x, can_consume_y, dispatch_x, dispatch_y
            );

            // Dispatch if there's delta for this scroll's direction
            if dispatch_x.abs() > 0.001 || dispatch_y.abs() > 0.001 {
                let ctx = crate::event_handler::EventContext::new(
                    blinc_core::events::event_types::SCROLL,
                    node_id,
                )
                .with_mouse_pos(mouse_x, mouse_y)
                .with_scroll_delta(dispatch_x, dispatch_y);

                tracing::trace!(
                    "    dispatching to {:?}: delta=({:.1}, {:.1})",
                    node_id,
                    dispatch_x,
                    dispatch_y
                );
                self.handler_registry.dispatch(&ctx);

                // Consume the delta for axes this scroll CAN consume (has room to scroll)
                // This prevents bubbling to outer scrolls for that axis
                // For custom scroll handlers (no physics), consume all dispatched delta
                if has_scroll_physics {
                    if can_consume_x && handles_x {
                        delta_x = 0.0;
                    }
                    if can_consume_y && handles_y {
                        delta_y = 0.0;
                    }
                } else {
                    // Custom scroll handler - consume all delta (it handles its own bounds)
                    if handles_x {
                        delta_x = 0.0;
                    }
                    if handles_y {
                        delta_y = 0.0;
                    }
                }
            }
        }

        (delta_x, delta_y)
    }

    // =========================================================================
    // Motion Animation Initialization
    // =========================================================================

    /// Initialize motion animations for nodes with motion config
    ///
    /// Call this after building/rebuilding the tree to start enter animations
    /// for any nodes wrapped in motion() containers.
    ///
    /// For nodes with a `motion_stable_id`, the animation state is tracked by
    /// stable key instead of node_id. This allows animations to persist across
    /// tree rebuilds (essential for overlays which are rebuilt every frame).
    pub fn initialize_motion_animations(
        &self,
        render_state: &mut crate::render_state::RenderState,
    ) {
        for (&node_id, render_node) in &self.render_nodes {
            if let Some(ref motion_config) = render_node.props.motion {
                // Use stable key if available (for overlays), otherwise use node_id
                if let Some(ref stable_key) = render_node.props.motion_stable_id {
                    // Start or replay stable motion based on replay flag
                    // Motion exit is now triggered explicitly via MotionHandle.exit()
                    render_state.start_stable_motion(
                        stable_key,
                        motion_config.clone(),
                        render_node.props.motion_should_replay,
                    );
                } else {
                    render_state.start_enter_motion(node_id, motion_config.clone());
                }
            }
        }
    }

    /// Get nodes with motion config (for external initialization)
    pub fn nodes_with_motion(&self) -> Vec<(LayoutNodeId, crate::element::MotionAnimation)> {
        self.render_nodes
            .iter()
            .filter_map(|(&node_id, render_node)| {
                render_node.props.motion.clone().map(|m| (node_id, m))
            })
            .collect()
    }

    // =========================================================================
    // Scroll Offset Management
    // =========================================================================

    /// Apply a scroll delta to a node's scroll offset (without bounds checking)
    pub fn apply_scroll_delta(&mut self, node_id: LayoutNodeId, delta_x: f32, delta_y: f32) {
        let (current_x, current_y) = self
            .scroll_offsets
            .get(&node_id)
            .copied()
            .unwrap_or((0.0, 0.0));
        self.scroll_offsets
            .insert(node_id, (current_x + delta_x, current_y + delta_y));
    }

    /// Apply a scroll delta with bounds checking based on viewport and content size
    pub fn apply_scroll_delta_with_bounds(
        &mut self,
        node_id: LayoutNodeId,
        delta_x: f32,
        delta_y: f32,
    ) {
        let (current_x, current_y) = self
            .scroll_offsets
            .get(&node_id)
            .copied()
            .unwrap_or((0.0, 0.0));

        // Get the viewport bounds for this node (parent offset doesn't matter for size)
        let bounds = self.layout_tree.get_bounds(node_id, (0.0, 0.0));
        let viewport_width = bounds.map(|b| b.width).unwrap_or(0.0);
        let viewport_height = bounds.map(|b| b.height).unwrap_or(0.0);

        // Get content size from Taffy's content_size
        let (content_width, content_height) = self
            .layout_tree
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
    ///
    /// Note: Returns rounded values to prevent subpixel jitter during scrolling.
    /// Fractional scroll offsets cause content to shift between pixel boundaries,
    /// resulting in wobbling text and lines.
    pub fn get_scroll_offset(&self, node_id: LayoutNodeId) -> (f32, f32) {
        // Check scroll physics first (has direction-aware scroll from element)
        let (x, y) = if let Some(physics) = self.scroll_physics.get(&node_id) {
            if let Ok(p) = physics.try_lock() {
                (p.offset_x, p.offset_y)
            } else {
                self.scroll_offsets
                    .get(&node_id)
                    .copied()
                    .unwrap_or((0.0, 0.0))
            }
        } else {
            // Fallback to legacy scroll_offsets
            self.scroll_offsets
                .get(&node_id)
                .copied()
                .unwrap_or((0.0, 0.0))
        };

        // Round to whole pixels to prevent subpixel jitter
        (x.round(), y.round())
    }

    /// Get the motion translation for a node (if it has motion bindings)
    ///
    /// Returns the current translation transform from any bound AnimatedValue(s).
    /// This is sampled every frame, enabling continuous smooth animations.
    pub fn get_motion_transform(&self, node_id: LayoutNodeId) -> Option<Transform> {
        self.motion_bindings
            .get(&node_id)
            .and_then(|b| b.get_transform())
    }

    /// Get the motion scale for a node (if it has motion bindings)
    ///
    /// Returns (scale_x, scale_y) if scale bindings are present.
    pub fn get_motion_scale(&self, node_id: LayoutNodeId) -> Option<(f32, f32)> {
        self.motion_bindings
            .get(&node_id)
            .and_then(|b| b.get_scale())
    }

    /// Get the motion rotation for a node (if it has motion bindings)
    ///
    /// Returns rotation in degrees if rotation binding is present.
    pub fn get_motion_rotation(&self, node_id: LayoutNodeId) -> Option<f32> {
        self.motion_bindings
            .get(&node_id)
            .and_then(|b| b.get_rotation())
    }

    /// Get the motion opacity for a node (if it has motion bindings)
    pub fn get_motion_opacity(&self, node_id: LayoutNodeId) -> Option<f32> {
        self.motion_bindings
            .get(&node_id)
            .and_then(|b| b.get_opacity())
    }

    /// Check if a node has motion bindings
    pub fn has_motion_bindings(&self, node_id: LayoutNodeId) -> bool {
        self.motion_bindings.contains_key(&node_id)
    }

    /// Get the scroll direction for a node (if it's a scroll container)
    ///
    /// Returns None if the node is not a scroll container.
    pub fn get_scroll_direction(
        &self,
        node_id: LayoutNodeId,
    ) -> Option<crate::scroll::ScrollDirection> {
        self.scroll_physics
            .get(&node_id)
            .and_then(|physics| physics.try_lock().ok().map(|p| p.config.direction))
    }

    /// Check if a scroll container can scroll in the given delta direction
    ///
    /// Returns true if the scroll container handles that axis.
    /// Used for nested scroll event handling.
    ///
    /// A scroll container consumes scroll for its direction(s) unless:
    /// - It has no scrollable content (content fits within viewport)
    /// - It's at an edge AND scrolling further into that edge AND bounce is disabled
    pub fn can_consume_scroll(
        &self,
        node_id: LayoutNodeId,
        delta_x: f32,
        delta_y: f32,
    ) -> (bool, bool) {
        let Some(physics) = self.scroll_physics.get(&node_id) else {
            return (false, false);
        };

        let Ok(p) = physics.try_lock() else {
            return (false, false);
        };

        let can_x = match p.config.direction {
            crate::scroll::ScrollDirection::Horizontal | crate::scroll::ScrollDirection::Both => {
                // Check if there's any scrollable content
                let scrollable_x = p.content_width - p.viewport_width;
                if scrollable_x <= 0.0 {
                    // No scrollable content - don't consume
                    false
                } else if delta_x.abs() < 0.001 {
                    // No horizontal delta to consume
                    false
                } else if delta_x < 0.0 {
                    // Scrolling left - can consume if not at left edge
                    // With bounce: only consume if we can still scroll OR are bouncing back
                    // Without bounce: only consume if not at edge
                    let at_left_edge = p.offset_x <= p.max_offset_x();
                    !at_left_edge || p.is_overscrolling_x()
                } else {
                    // Scrolling right - can consume if not at right edge
                    let at_right_edge = p.offset_x >= p.min_offset_x();
                    !at_right_edge || p.is_overscrolling_x()
                }
            }
            _ => false,
        };

        let can_y = match p.config.direction {
            crate::scroll::ScrollDirection::Vertical | crate::scroll::ScrollDirection::Both => {
                // Check if there's any scrollable content
                let scrollable_y = p.content_height - p.viewport_height;
                if scrollable_y <= 0.0 {
                    // No scrollable content - don't consume
                    false
                } else if delta_y.abs() < 0.001 {
                    // No vertical delta to consume
                    false
                } else if delta_y < 0.0 {
                    // Scrolling up (content moves down) - can consume if not at bottom edge
                    // With bounce: only consume if we can still scroll OR are bouncing back
                    // Without bounce: only consume if not at edge
                    let at_bottom_edge = p.offset_y <= p.max_offset_y();
                    !at_bottom_edge || p.is_overscrolling_y()
                } else {
                    // Scrolling down (content moves up) - can consume if not at top edge
                    let at_top_edge = p.offset_y >= p.min_offset_y();
                    !at_top_edge || p.is_overscrolling_y()
                }
            }
            _ => false,
        };

        (can_x, can_y)
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

    /// Notify all scroll physics that scrolling has ended
    ///
    /// Call this when a SCROLL_END event is received to start bounce-back animations.
    pub fn on_scroll_end(&self) {
        for physics in self.scroll_physics.values() {
            physics.lock().unwrap().on_scroll_end();
        }
    }

    /// Notify all scroll physics that the scroll gesture has ended (finger lifted)
    ///
    /// Call this when `ScrollPhase::Ended` is detected to start bounce-back
    /// animations immediately, without waiting for momentum scroll to finish.
    pub fn on_gesture_end(&self) {
        for physics in self.scroll_physics.values() {
            physics.lock().unwrap().on_gesture_end();
        }
    }

    /// Tick all scroll physics and return true if any are animating
    ///
    /// Call this each frame with the current time in milliseconds.
    /// Uses actual time delta for smooth, frame-rate independent animation.
    pub fn tick_scroll_physics(&mut self, current_time_ms: u64) -> bool {
        // Calculate actual delta time
        let dt_secs = if let Some(last_time) = self.last_scroll_tick_ms {
            (current_time_ms.saturating_sub(last_time)) as f32 / 1000.0
        } else {
            1.0 / 60.0 // Assume ~60fps for first frame
        };
        self.last_scroll_tick_ms = Some(current_time_ms);

        // Clamp dt to prevent huge jumps if app was paused
        let dt_secs = dt_secs.min(0.1);

        let mut any_animating = false;
        for physics in self.scroll_physics.values() {
            if physics.lock().unwrap().tick(dt_secs) {
                any_animating = true;
            }
        }
        any_animating
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
                return unsafe { Arc::from_raw(Arc::into_raw(cloned) as *const Mutex<S>) };
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

    // =========================================================================
    // Stylesheet Integration
    // =========================================================================

    /// Set the stylesheet for automatic state modifier application
    ///
    /// When a stylesheet is set, elements with IDs will automatically get
    /// `:hover`, `:active`, `:focus`, `:disabled` styles applied based on
    /// their current interaction state.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let css = r#"
    ///     #button { background: blue; }
    ///     #button:hover { opacity: 0.9; }
    ///     #button:active { transform: scale(0.98); }
    /// "#;
    /// let stylesheet = Stylesheet::parse_with_errors(css).stylesheet;
    /// tree.set_stylesheet(stylesheet);
    /// ```
    pub fn set_stylesheet(&mut self, stylesheet: Stylesheet) {
        self.stylesheet = Some(Arc::new(stylesheet));
    }

    /// Set a shared stylesheet reference
    pub fn set_stylesheet_arc(&mut self, stylesheet: Arc<Stylesheet>) {
        self.stylesheet = Some(stylesheet);
    }

    /// Get the current stylesheet, if any
    pub fn stylesheet(&self) -> Option<&Stylesheet> {
        self.stylesheet.as_ref().map(|s| s.as_ref())
    }

    /// Apply state-specific styles from the stylesheet to a node
    ///
    /// This is called when a node's interaction state changes (hover, pressed, focused).
    /// It looks up the node's string ID and applies any matching state styles.
    ///
    /// # Arguments
    /// * `node_id` - The node whose state changed
    /// * `hovered` - Whether the node is currently hovered
    /// * `pressed` - Whether the node is currently pressed
    /// * `focused` - Whether the node is currently focused
    ///
    /// # Returns
    /// `true` if styles were applied, `false` if no stylesheet or no matching styles
    pub fn apply_state_styles(
        &mut self,
        node_id: LayoutNodeId,
        hovered: bool,
        pressed: bool,
        focused: bool,
    ) -> bool {
        // Early return if no stylesheet
        let stylesheet = match &self.stylesheet {
            Some(s) => s.clone(),
            None => return false,
        };

        // Look up the node's string ID from the registry
        let element_id = match self.element_registry.get_id(node_id) {
            Some(id) => id,
            None => return false, // Node has no ID, can't apply stylesheet styles
        };

        // Get or store base style for this node
        if !self.base_styles.contains_key(&node_id) {
            if let Some(render_node) = self.render_nodes.get(&node_id) {
                self.base_styles.insert(node_id, render_node.props.clone());
            }
        }

        // Start with base style
        let base_props = match self.base_styles.get(&node_id) {
            Some(props) => props.clone(),
            None => return false,
        };

        // Apply state-specific styles in order of precedence
        let mut applied = false;
        let render_node = match self.render_nodes.get_mut(&node_id) {
            Some(node) => node,
            None => return false,
        };

        // Reset to base style first
        render_node.props = base_props;

        // Apply base stylesheet style (if any)
        if let Some(base_style) = stylesheet.get(&element_id) {
            Self::apply_element_style_to_props(&mut render_node.props, base_style);
            applied = true;
        }

        // Apply hover style
        if hovered {
            if let Some(hover_style) = stylesheet.get_with_state(&element_id, ElementState::Hover) {
                Self::apply_element_style_to_props(&mut render_node.props, hover_style);
                applied = true;
            }
        }

        // Apply active/pressed style (takes precedence over hover)
        if pressed {
            if let Some(active_style) = stylesheet.get_with_state(&element_id, ElementState::Active)
            {
                Self::apply_element_style_to_props(&mut render_node.props, active_style);
                applied = true;
            }
        }

        // Apply focus style
        if focused {
            if let Some(focus_style) = stylesheet.get_with_state(&element_id, ElementState::Focus) {
                Self::apply_element_style_to_props(&mut render_node.props, focus_style);
                applied = true;
            }
        }

        applied
    }

    /// Apply ElementStyle properties to RenderProps
    fn apply_element_style_to_props(
        props: &mut RenderProps,
        style: &crate::element_style::ElementStyle,
    ) {
        if let Some(ref bg) = style.background {
            props.background = Some(bg.clone());
        }
        if let Some(ref cr) = style.corner_radius {
            props.border_radius = *cr;
        }
        if let Some(ref shadow) = style.shadow {
            props.shadow = Some(shadow.clone());
        }
        if let Some(ref transform) = style.transform {
            props.transform = Some(transform.clone());
        }
        if let Some(opacity) = style.opacity {
            props.opacity = opacity;
        }
        if let Some(ref render_layer) = style.render_layer {
            props.layer = *render_layer;
        }
    }

    /// Check if a node has stylesheet state styles defined
    ///
    /// Returns true if the node has an ID and the stylesheet has any
    /// state-specific styles (`:hover`, `:active`, `:focus`, `:disabled`) for it.
    pub fn has_state_styles(&self, node_id: LayoutNodeId) -> bool {
        let stylesheet = match &self.stylesheet {
            Some(s) => s,
            None => return false,
        };

        let element_id = match self.element_registry.get_id(node_id) {
            Some(id) => id,
            None => return false,
        };

        // Check if any state styles exist
        stylesheet.contains_with_state(&element_id, ElementState::Hover)
            || stylesheet.contains_with_state(&element_id, ElementState::Active)
            || stylesheet.contains_with_state(&element_id, ElementState::Focus)
            || stylesheet.contains_with_state(&element_id, ElementState::Disabled)
    }

    /// Apply stylesheet state styles based on EventRouter state
    ///
    /// This should be called after mouse events to update styles for nodes
    /// whose interaction state has changed. It applies `:hover`, `:active`,
    /// and `:focus` styles from the stylesheet.
    ///
    /// # Arguments
    /// * `router` - The event router containing current interaction state
    ///
    /// # Returns
    /// `true` if any styles were applied, `false` otherwise
    pub fn apply_stylesheet_state_styles(
        &mut self,
        router: &crate::event_router::EventRouter,
    ) -> bool {
        // Early return if no stylesheet
        if self.stylesheet.is_none() {
            return false;
        }

        let mut any_applied = false;

        // Get all registered element IDs and their node IDs
        let registered_ids: Vec<(String, crate::tree::LayoutNodeId)> = self
            .element_registry
            .all_ids()
            .into_iter()
            .filter_map(|id| self.element_registry.get(&id).map(|node_id| (id, node_id)))
            .collect();

        // Apply state styles for each registered element
        for (element_id, node_id) in registered_ids {
            // Check if this element has any state styles in the stylesheet
            if !self.has_state_styles(node_id) {
                continue;
            }

            // Get current interaction state from router
            let hovered = router.is_hovered(node_id);
            let pressed = router.is_pressed(node_id);
            let focused = router.is_focused(node_id);

            // Apply state styles
            if self.apply_state_styles(node_id, hovered, pressed, focused) {
                any_applied = true;
                tracing::trace!(
                    "Applied stylesheet state styles to #{}: hovered={}, pressed={}, focused={}",
                    element_id,
                    hovered,
                    pressed,
                    focused
                );
            }
        }

        any_applied
    }

    /// Rebuild only the children of a specific node
    ///
    /// This is used for incremental updates when a stateful element's
    /// dependencies change. Instead of rebuilding the entire tree,
    /// we only rebuild the affected subtree.
    ///
    /// # Arguments
    /// * `parent_id` - The node whose children should be rebuilt
    /// * `new_child` - The new child element builder
    ///
    /// # Returns
    /// The ID of the new child node
    pub fn rebuild_children<E: ElementBuilder>(
        &mut self,
        parent_id: LayoutNodeId,
        new_child: &E,
    ) -> LayoutNodeId {
        // 1. Remove old children from layout tree and render nodes
        let old_children = self.layout_tree.children(parent_id);
        for child_id in &old_children {
            self.remove_subtree_nodes(*child_id);
        }
        self.layout_tree.clear_children(parent_id);

        // 2. Build the new child element into the layout tree
        let new_child_id = new_child.build(&mut self.layout_tree);

        // 3. Add the new child to the parent
        self.layout_tree.add_child(parent_id, new_child_id);

        // 4. Collect render props for the new subtree
        self.collect_render_props(new_child, new_child_id);

        new_child_id
    }

    /// Remove render nodes for a subtree (but don't touch layout tree)
    fn remove_subtree_nodes(&mut self, node_id: LayoutNodeId) {
        // Remove children first
        let children = self.layout_tree.children(node_id);
        for child_id in children {
            self.remove_subtree_nodes(child_id);
        }

        // Remove this node's render data
        self.render_nodes.swap_remove(&node_id);
        self.handler_registry.remove(node_id);
        self.node_states.remove(&node_id);
        self.scroll_offsets.remove(&node_id);
        self.scroll_physics.remove(&node_id);
        self.scroll_refs.remove(&node_id);
        // Unregister from element registry (removes by node_id)
        self.element_registry.unregister(node_id);
    }

    /// Process all pending subtree rebuilds
    ///
    /// This is called by the windowed app after processing events.
    /// It applies queued child rebuilds without rebuilding the entire tree.
    /// Process pending subtree rebuilds
    ///
    /// Returns true if any rebuild requires layout recomputation.
    /// Visual-only rebuilds (hover/press) return false.
    ///
    /// Processes only rebuilds for nodes that exist in this tree.
    /// Rebuilds for nodes in other trees (e.g., overlay) are put back in the queue.
    pub fn process_pending_subtree_rebuilds(&mut self) -> bool {
        let pending = crate::stateful::take_pending_subtree_rebuilds();
        if pending.is_empty() {
            return false;
        }

        tracing::debug!("Processing {} pending subtree rebuilds", pending.len());

        let mut needs_layout = false;
        let mut not_in_this_tree = Vec::new();

        for rebuild in pending {
            // Skip if this node doesn't exist in this tree - save for other trees
            if !self.layout_tree.node_exists(rebuild.parent_id) {
                tracing::debug!(
                    "Subtree rebuild: node {:?} not in this tree, requeuing",
                    rebuild.parent_id
                );
                not_in_this_tree.push(rebuild);
                continue;
            }
            tracing::debug!(
                "Subtree rebuild: processing node {:?}, needs_layout={}",
                rebuild.parent_id,
                rebuild.needs_layout
            );
            if rebuild.needs_layout {
                // Full structural rebuild - remove old children and build new ones
                needs_layout = true;

                // Update the parent node's own render props AND layout style
                // This is critical for overlay layer where size changes from 0x0 to full viewport
                if let Some(render_node) = self.render_nodes.get_mut(&rebuild.parent_id) {
                    let mut new_props = rebuild.new_child.render_props();
                    new_props.node_id = Some(rebuild.parent_id);
                    new_props.motion = render_node.props.motion.clone();
                    render_node.props = new_props;
                }
                // Also update the taffy layout style (width, height, padding, etc.)
                if let Some(style) = rebuild.new_child.layout_style() {
                    self.layout_tree.set_style(rebuild.parent_id, style.clone());
                }

                // Always remove old children first (even if new children is empty)
                // This fixes the bug where SVG checkmarks would persist after unchecking
                let old_children = self.layout_tree.children(rebuild.parent_id);
                for child_id in &old_children {
                    self.remove_subtree_nodes(*child_id);
                }
                self.layout_tree.clear_children(rebuild.parent_id);

                // Build new children (if any)
                let children = rebuild.new_child.children_builders();
                for child in children {
                    let child_id = child.build(&mut self.layout_tree);
                    self.layout_tree.add_child(rebuild.parent_id, child_id);
                    self.collect_render_props_boxed(child.as_ref(), child_id);
                }
            } else {
                // Visual-only update - just update render props of existing children
                // Don't remove/rebuild, just walk the tree and update props
                self.update_subtree_props_recursive(rebuild.parent_id, &rebuild.new_child);
            }
        }

        // Put back rebuilds for nodes not in this tree (for other trees to process)
        if !not_in_this_tree.is_empty() {
            crate::stateful::requeue_subtree_rebuilds(not_in_this_tree);
        }

        needs_layout
    }

    /// Recursively update render props for existing children without rebuilding
    ///
    /// This walks the existing layout tree children alongside the new element definition
    /// and updates render props for matching nodes (by position in child order).
    fn update_subtree_props_recursive(
        &mut self,
        parent_id: LayoutNodeId,
        new_element: &crate::div::Div,
    ) {
        self.update_subtree_props_from_builder(parent_id, new_element);
    }

    /// Update subtree props from a generic ElementBuilder (for recursion)
    fn update_subtree_props_from_builder(
        &mut self,
        parent_id: LayoutNodeId,
        new_element: &dyn crate::div::ElementBuilder,
    ) {
        let existing_children = self.layout_tree.children(parent_id);
        let new_children = new_element.children_builders();

        for (i, child_id) in existing_children.iter().enumerate() {
            if let Some(new_child) = new_children.get(i) {
                // Update this child's render props
                let new_props = new_child.render_props();
                if let Some(render_node) = self.render_nodes.get_mut(child_id) {
                    render_node.props.merge_from(&new_props);
                }

                // Recursively update grandchildren
                if !new_child.children_builders().is_empty() {
                    self.update_subtree_props_from_builder(*child_id, new_child.as_ref());
                }
            }
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
        tracing::trace!(
            "render: motion_bindings count = {}",
            self.motion_bindings.len()
        );
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

        // Apply element-specific transform if present (static, set at build time)
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

        // Apply motion binding translation if present (dynamic, sampled every frame)
        // Translation is NOT centered (moves element from its position)
        let motion_transform = self.get_motion_transform(node);
        let has_motion_transform = motion_transform.is_some();
        if let Some(ref transform) = motion_transform {
            // Log to verify animation is running
            if let Transform::Affine2D(a) = transform {
                tracing::debug!(
                    "paint_node: applying motion transform to {:?}: tx={}, ty={}",
                    node,
                    a.elements[4],
                    a.elements[5]
                );
            }
            ctx.push_transform(transform.clone());
        }

        // Apply motion binding scale if present (centered around element)
        let motion_scale = self.get_motion_scale(node);
        let has_motion_scale = motion_scale.is_some();
        if let Some((sx, sy)) = motion_scale {
            let center_x = bounds.width / 2.0;
            let center_y = bounds.height / 2.0;
            ctx.push_transform(Transform::translate(center_x, center_y));
            ctx.push_transform(Transform::scale(sx, sy));
            ctx.push_transform(Transform::translate(-center_x, -center_y));
        }

        // Apply motion binding rotation if present (centered around element)
        let motion_rotation = self.get_motion_rotation(node);
        let has_motion_rotation = motion_rotation.is_some();
        if let Some(deg) = motion_rotation {
            let center_x = bounds.width / 2.0;
            let center_y = bounds.height / 2.0;
            ctx.push_transform(Transform::translate(center_x, center_y));
            ctx.push_transform(Transform::rotate(deg.to_radians()));
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

        // Draw borders
        // Individual border sides take precedence over uniform border
        if render_node.props.border_sides.has_any() {
            // Draw individual border sides as filled rectangles
            let sides = &render_node.props.border_sides;

            // Left border
            if let Some(ref border) = sides.left {
                if border.is_visible() {
                    let border_rect = Rect::new(0.0, 0.0, border.width, rect.height());
                    ctx.fill_rect(
                        border_rect,
                        CornerRadius::default(),
                        Brush::Solid(border.color),
                    );
                }
            }

            // Right border
            if let Some(ref border) = sides.right {
                if border.is_visible() {
                    let border_rect = Rect::new(
                        rect.width() - border.width,
                        0.0,
                        border.width,
                        rect.height(),
                    );
                    ctx.fill_rect(
                        border_rect,
                        CornerRadius::default(),
                        Brush::Solid(border.color),
                    );
                }
            }

            // Top border
            if let Some(ref border) = sides.top {
                if border.is_visible() {
                    let border_rect = Rect::new(0.0, 0.0, rect.width(), border.width);
                    ctx.fill_rect(
                        border_rect,
                        CornerRadius::default(),
                        Brush::Solid(border.color),
                    );
                }
            }

            // Bottom border
            if let Some(ref border) = sides.bottom {
                if border.is_visible() {
                    let border_rect = Rect::new(
                        0.0,
                        rect.height() - border.width,
                        rect.width(),
                        border.width,
                    );
                    ctx.fill_rect(
                        border_rect,
                        CornerRadius::default(),
                        Brush::Solid(border.color),
                    );
                }
            }
        } else if render_node.props.border_width > 0.0 {
            // Fall back to uniform border
            if let Some(ref border_color) = render_node.props.border_color {
                let stroke = Stroke::new(render_node.props.border_width);
                ctx.stroke_rect(rect, radius, &stroke, Brush::Solid(*border_color));
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

        // Pop motion binding rotation (3 transforms for centering)
        if has_motion_rotation {
            ctx.pop_transform();
            ctx.pop_transform();
            ctx.pop_transform();
        }

        // Pop motion binding scale (3 transforms for centering)
        if has_motion_scale {
            ctx.pop_transform();
            ctx.pop_transform();
            ctx.pop_transform();
        }

        // Pop motion binding translation (1 transform)
        if has_motion_transform {
            ctx.pop_transform();
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
            ctx.set_foreground_layer(false);
            self.render_layer(ctx, root, (0.0, 0.0), RenderLayer::Background, false, false);

            // Pass 2: Glass - these render as Brush::Glass which becomes glass primitives
            self.render_layer(ctx, root, (0.0, 0.0), RenderLayer::Glass, false, false);

            // Pass 3: Foreground (includes children of glass elements, rendered after glass)
            ctx.set_foreground_layer(true);
            self.render_layer(ctx, root, (0.0, 0.0), RenderLayer::Foreground, false, false);
            ctx.set_foreground_layer(false);
        }
    }

    /// Render with motion animations from RenderState
    ///
    /// This method applies animated opacity, scale, and translation from motion
    /// animations stored in RenderState. Use this when you have elements wrapped
    /// in motion() containers.
    pub fn render_with_motion(
        &self,
        ctx: &mut dyn DrawContext,
        render_state: &crate::render_state::RenderState,
    ) {
        if let Some(root) = self.root {
            // Apply DPI scale factor if set (for HiDPI display support)
            let has_scale = self.scale_factor != 1.0;
            if has_scale {
                ctx.push_transform(Transform::scale(self.scale_factor, self.scale_factor));
            }

            // Pass 1: Background (primitives go to background batch)
            ctx.set_foreground_layer(false);
            self.render_layer_with_motion(
                ctx,
                root,
                (0.0, 0.0),
                RenderLayer::Background,
                false, // inside_glass
                false, // inside_foreground
                render_state,
                1.0, // Start with full opacity at root
            );

            // Pass 2: Glass (primitives go to glass batch)
            self.render_layer_with_motion(
                ctx,
                root,
                (0.0, 0.0),
                RenderLayer::Glass,
                false, // inside_glass
                false, // inside_foreground
                render_state,
                1.0, // Start with full opacity at root
            );

            // Pass 3: Foreground (primitives go to foreground batch, rendered after glass)
            ctx.set_foreground_layer(true);
            self.render_layer_with_motion(
                ctx,
                root,
                (0.0, 0.0),
                RenderLayer::Foreground,
                false, // inside_glass
                false, // inside_foreground
                render_state,
                1.0, // Start with full opacity at root
            );
            ctx.set_foreground_layer(false);

            // Pop the DPI scale transform
            if has_scale {
                ctx.pop_transform();
            }
        }
    }

    /// Render a layer with motion animation support
    ///
    /// The `inherited_opacity` parameter allows parent motion containers to pass
    /// their opacity down to children, ensuring the entire motion group fades together.
    ///
    /// The `inside_foreground` parameter tracks whether we're inside a foreground element,
    /// ensuring all descendants of foreground elements also render in the foreground pass.
    fn render_layer_with_motion(
        &self,
        ctx: &mut dyn DrawContext,
        node: LayoutNodeId,
        parent_offset: (f32, f32),
        target_layer: RenderLayer,
        inside_glass: bool,
        inside_foreground: bool,
        render_state: &crate::render_state::RenderState,
        inherited_opacity: f32,
    ) {
        let Some(bounds) = self.layout_tree.get_bounds(node, parent_offset) else {
            return;
        };

        let Some(render_node) = self.render_nodes.get(&node) else {
            tracing::trace!(
                "render_layer_with_motion: no render_node for {:?}, skipping",
                node
            );
            return;
        };

        // Check if this node should be skipped (motion removed)
        // For stable-keyed motions, check by key; for node-based, check by node_id
        let motion_removed = if let Some(ref stable_key) = render_node.props.motion_stable_id {
            render_state.is_stable_motion_removed(stable_key)
        } else {
            render_state.is_motion_removed(node)
        };
        if motion_removed {
            return;
        }

        // Get motion values from RenderState (for entry/exit animations)
        // For stable-keyed motions (overlays), look up by key; otherwise by node_id
        let motion_values = if let Some(ref stable_key) = render_node.props.motion_stable_id {
            render_state.get_stable_motion_values(stable_key)
        } else {
            render_state.get_motion_values(node)
        };

        // Get motion bindings from RenderTree (for continuous AnimatedValue animations)
        let binding_transform = self.get_motion_transform(node);
        let binding_opacity = self.get_motion_opacity(node);

        // Calculate this node's motion opacity (combine motion values and bindings)
        let node_motion_opacity = motion_values
            .and_then(|m| m.opacity)
            .unwrap_or_else(|| binding_opacity.unwrap_or(1.0));

        // Combine with inherited opacity from parent motion containers
        // This ensures children fade together with their parent motion container
        let motion_opacity = inherited_opacity * node_motion_opacity;

        // Skip rendering if completely transparent
        if motion_opacity <= 0.001 {
            return;
        }

        // Push position transform
        ctx.push_transform(Transform::translate(bounds.x, bounds.y));

        // Apply motion translation
        if let Some(motion) = motion_values {
            let (tx, ty) = motion.resolved_translate();
            if tx.abs() > 0.001 || ty.abs() > 0.001 {
                ctx.push_transform(Transform::translate(tx, ty));
            }
        }

        // Apply motion scale (centered)
        let has_motion_scale = motion_values
            .map(|m| {
                let (sx, sy) = m.resolved_scale();
                (sx - 1.0).abs() > 0.001 || (sy - 1.0).abs() > 0.001
            })
            .unwrap_or(false);

        if has_motion_scale {
            let (sx, sy) = motion_values.unwrap().resolved_scale();
            let center_x = bounds.width / 2.0;
            let center_y = bounds.height / 2.0;
            ctx.push_transform(Transform::translate(center_x, center_y));
            ctx.push_transform(Transform::scale(sx, sy));
            ctx.push_transform(Transform::translate(-center_x, -center_y));
        }

        // Apply motion binding transform if present (continuous AnimatedValue-driven animation)
        // Translation is NOT centered (moves element from its position)
        let has_binding_transform = binding_transform.is_some();
        if let Some(ref transform) = binding_transform {
            ctx.push_transform(transform.clone());
        }

        // Apply motion binding scale if present (centered around element)
        let binding_scale = self.get_motion_scale(node);
        let has_binding_scale = binding_scale.is_some();
        if let Some((sx, sy)) = binding_scale {
            let center_x = bounds.width / 2.0;
            let center_y = bounds.height / 2.0;
            ctx.push_transform(Transform::translate(center_x, center_y));
            ctx.push_transform(Transform::scale(sx, sy));
            ctx.push_transform(Transform::translate(-center_x, -center_y));
        }

        // Apply motion binding rotation if present (centered around element)
        let binding_rotation = self.get_motion_rotation(node);
        let has_binding_rotation = binding_rotation.is_some();
        if let Some(deg) = binding_rotation {
            let center_x = bounds.width / 2.0;
            let center_y = bounds.height / 2.0;
            ctx.push_transform(Transform::translate(center_x, center_y));
            ctx.push_transform(Transform::rotate(deg.to_radians()));
            ctx.push_transform(Transform::translate(-center_x, -center_y));
        }

        // Apply element-specific transform if present
        let has_element_transform = render_node.props.transform.is_some();
        if let Some(ref transform) = render_node.props.transform {
            let center_x = bounds.width / 2.0;
            let center_y = bounds.height / 2.0;
            ctx.push_transform(Transform::translate(center_x, center_y));
            ctx.push_transform(transform.clone());
            ctx.push_transform(Transform::translate(-center_x, -center_y));
        }

        // Determine if this node is a glass element
        let is_glass = matches!(render_node.props.material, Some(Material::Glass(_)));
        let children_inside_glass = inside_glass || is_glass;

        // Determine if this node is a foreground element
        let is_foreground = render_node.props.layer == RenderLayer::Foreground;
        let children_inside_foreground = inside_foreground || is_foreground;

        // Increment z_layer for Stack children for proper interleaved rendering
        // This ensures primitives AND text in each Stack layer render together
        let is_stack_layer = render_node.props.is_stack_layer;
        if is_stack_layer {
            let current_z = ctx.z_layer();
            ctx.set_z_layer(current_z + 1);
        }

        // Push clip if needed
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

        // Determine effective layer:
        // - Children of glass elements render in foreground
        // - Children of foreground elements also render in foreground
        // - Glass elements render in glass layer
        // - Otherwise, use the node's explicit layer setting
        let effective_layer = if inside_glass && !is_glass {
            RenderLayer::Foreground
        } else if inside_foreground {
            RenderLayer::Foreground
        } else if is_glass {
            RenderLayer::Glass
        } else {
            render_node.props.layer
        };

        // Render if this node matches target layer
        if effective_layer == target_layer {
            let rect = Rect::new(0.0, 0.0, bounds.width, bounds.height);
            let radius = render_node.props.border_radius;

            // Apply motion opacity to rendering
            // TODO: When DrawContext supports opacity, apply motion_opacity here
            // For now, we rely on the brush alpha

            if let Some(Material::Glass(glass)) = &render_node.props.material {
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
                if let Some(ref shadow) = render_node.props.shadow {
                    // Apply motion opacity to shadow color
                    let shadow = if motion_opacity < 1.0 {
                        Shadow {
                            color: Color::rgba(
                                shadow.color.r,
                                shadow.color.g,
                                shadow.color.b,
                                shadow.color.a * motion_opacity,
                            ),
                            ..*shadow
                        }
                    } else {
                        *shadow
                    };
                    ctx.draw_shadow(rect, radius, shadow);
                }
                if let Some(ref bg) = render_node.props.background {
                    // Apply motion opacity to background
                    let brush = if motion_opacity < 1.0 {
                        apply_opacity_to_brush(bg, motion_opacity)
                    } else {
                        bg.clone()
                    };
                    ctx.fill_rect(rect, radius, brush);
                }
            }

            // Draw borders
            // Individual border sides take precedence over uniform border
            if render_node.props.border_sides.has_any() {
                let sides = &render_node.props.border_sides;

                // Helper to apply motion opacity
                let apply_motion = |color: Color| -> Color {
                    if motion_opacity < 1.0 {
                        Color::rgba(color.r, color.g, color.b, color.a * motion_opacity)
                    } else {
                        color
                    }
                };

                // Left border
                if let Some(ref border) = sides.left {
                    if border.is_visible() {
                        let border_rect = Rect::new(0.0, 0.0, border.width, rect.height());
                        ctx.fill_rect(
                            border_rect,
                            CornerRadius::default(),
                            Brush::Solid(apply_motion(border.color)),
                        );
                    }
                }

                // Right border
                if let Some(ref border) = sides.right {
                    if border.is_visible() {
                        let border_rect = Rect::new(
                            rect.width() - border.width,
                            0.0,
                            border.width,
                            rect.height(),
                        );
                        ctx.fill_rect(
                            border_rect,
                            CornerRadius::default(),
                            Brush::Solid(apply_motion(border.color)),
                        );
                    }
                }

                // Top border
                if let Some(ref border) = sides.top {
                    if border.is_visible() {
                        let border_rect = Rect::new(0.0, 0.0, rect.width(), border.width);
                        ctx.fill_rect(
                            border_rect,
                            CornerRadius::default(),
                            Brush::Solid(apply_motion(border.color)),
                        );
                    }
                }

                // Bottom border
                if let Some(ref border) = sides.bottom {
                    if border.is_visible() {
                        let border_rect = Rect::new(
                            0.0,
                            rect.height() - border.width,
                            rect.width(),
                            border.width,
                        );
                        ctx.fill_rect(
                            border_rect,
                            CornerRadius::default(),
                            Brush::Solid(apply_motion(border.color)),
                        );
                    }
                }
            } else if render_node.props.border_width > 0.0 {
                if let Some(ref border_color) = render_node.props.border_color {
                    let stroke = Stroke::new(render_node.props.border_width);
                    let brush = if motion_opacity < 1.0 {
                        let mut color = *border_color;
                        color.a *= motion_opacity;
                        Brush::Solid(color)
                    } else {
                        Brush::Solid(*border_color)
                    };
                    ctx.stroke_rect(rect, radius, &stroke, brush);
                }
            }

            // Handle canvas elements
            if let ElementType::Canvas(canvas_data) = &render_node.element_type {
                if let Some(render_fn) = &canvas_data.render_fn {
                    let canvas_rect = Rect::new(0.0, 0.0, bounds.width, bounds.height);
                    ctx.push_clip(ClipShape::rect(canvas_rect));
                    let canvas_bounds = crate::canvas::CanvasBounds {
                        width: bounds.width,
                        height: bounds.height,
                    };
                    render_fn(ctx, canvas_bounds);
                    ctx.pop_clip();
                }
            }
        }

        // Apply scroll offset
        let scroll_offset = self.get_scroll_offset(node);
        let has_scroll = scroll_offset.0.abs() > 0.001 || scroll_offset.1.abs() > 0.001;
        if has_scroll {
            ctx.push_transform(Transform::translate(scroll_offset.0, scroll_offset.1));
        }

        // Render children, passing down the effective opacity and layer inheritance
        // This ensures all children inherit the parent motion's opacity and foreground layer
        for child_id in self.layout_tree.children(node) {
            self.render_layer_with_motion(
                ctx,
                child_id,
                (0.0, 0.0),
                target_layer,
                children_inside_glass,
                children_inside_foreground,
                render_state,
                motion_opacity, // Pass current opacity to children
            );
        }

        // Pop scroll transform
        if has_scroll {
            ctx.pop_transform();
        }

        // Pop clip
        if clips_content {
            ctx.pop_clip();
        }

        // Pop element transforms
        if has_element_transform {
            ctx.pop_transform();
            ctx.pop_transform();
            ctx.pop_transform();
        }

        // Pop motion binding rotation (3 transforms for centering)
        if has_binding_rotation {
            ctx.pop_transform();
            ctx.pop_transform();
            ctx.pop_transform();
        }

        // Pop motion binding scale (3 transforms for centering)
        if has_binding_scale {
            ctx.pop_transform();
            ctx.pop_transform();
            ctx.pop_transform();
        }

        // Pop motion binding translation (1 transform)
        if has_binding_transform {
            ctx.pop_transform();
        }

        // Pop motion scale transforms (from RenderState motion)
        if has_motion_scale {
            ctx.pop_transform();
            ctx.pop_transform();
            ctx.pop_transform();
        }

        // Pop motion translation
        if motion_values
            .map(|m| {
                let (tx, ty) = m.resolved_translate();
                tx.abs() > 0.001 || ty.abs() > 0.001
            })
            .unwrap_or(false)
        {
            ctx.pop_transform();
        }

        // Pop position transform
        ctx.pop_transform();
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
                false,
            );

            // Pass 2: Glass - render as Brush::Glass
            self.render_layer(glass_ctx, root, (0.0, 0.0), RenderLayer::Glass, false, false);

            // Pass 3: Foreground (includes children of glass elements)
            self.render_layer(
                foreground_ctx,
                root,
                (0.0, 0.0),
                RenderLayer::Foreground,
                false,
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
            // Apply DPI scale factor if set (for HiDPI display support)
            let has_scale = self.scale_factor != 1.0;
            if has_scale {
                ctx.push_transform(Transform::scale(self.scale_factor, self.scale_factor));
            }

            self.render_layer(ctx, root, (0.0, 0.0), target_layer, false, false);

            // Pop the DPI scale transform
            if has_scale {
                ctx.pop_transform();
            }
        }
    }

    /// Render only nodes in a specific layer
    ///
    /// The `inside_glass` flag tracks whether we're descending through a glass element.
    /// Children of glass elements are automatically rendered in the foreground pass.
    ///
    /// The `inside_foreground` flag tracks whether we're descending through a foreground element.
    /// Children of foreground elements are also rendered in the foreground pass.
    fn render_layer(
        &self,
        ctx: &mut dyn DrawContext,
        node: LayoutNodeId,
        parent_offset: (f32, f32),
        target_layer: RenderLayer,
        inside_glass: bool,
        inside_foreground: bool,
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

        // Track if children should be considered inside foreground
        // Once inside foreground, stay inside foreground for all descendants
        let is_foreground = render_node.props.layer == RenderLayer::Foreground;
        let children_inside_foreground = inside_foreground || is_foreground;

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
        // - If we're inside a foreground element, children also render as foreground
        // - Otherwise, use the node's explicit layer setting
        let effective_layer = if inside_glass && !is_glass {
            // Children of glass elements render in foreground
            RenderLayer::Foreground
        } else if inside_foreground {
            // Children of foreground elements render in foreground
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

            // Draw borders
            if render_node.props.border_sides.has_any() {
                let sides = &render_node.props.border_sides;

                if let Some(ref border) = sides.left {
                    if border.is_visible() {
                        let border_rect = Rect::new(0.0, 0.0, border.width, rect.height());
                        ctx.fill_rect(
                            border_rect,
                            CornerRadius::default(),
                            Brush::Solid(border.color),
                        );
                    }
                }
                if let Some(ref border) = sides.right {
                    if border.is_visible() {
                        let border_rect = Rect::new(
                            rect.width() - border.width,
                            0.0,
                            border.width,
                            rect.height(),
                        );
                        ctx.fill_rect(
                            border_rect,
                            CornerRadius::default(),
                            Brush::Solid(border.color),
                        );
                    }
                }
                if let Some(ref border) = sides.top {
                    if border.is_visible() {
                        let border_rect = Rect::new(0.0, 0.0, rect.width(), border.width);
                        ctx.fill_rect(
                            border_rect,
                            CornerRadius::default(),
                            Brush::Solid(border.color),
                        );
                    }
                }
                if let Some(ref border) = sides.bottom {
                    if border.is_visible() {
                        let border_rect = Rect::new(
                            0.0,
                            rect.height() - border.width,
                            rect.width(),
                            border.width,
                        );
                        ctx.fill_rect(
                            border_rect,
                            CornerRadius::default(),
                            Brush::Solid(border.color),
                        );
                    }
                }
            } else if render_node.props.border_width > 0.0 {
                if let Some(ref border_color) = render_node.props.border_color {
                    let stroke = Stroke::new(render_node.props.border_width);
                    ctx.stroke_rect(rect, radius, &stroke, Brush::Solid(*border_color));
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

        // Traverse children (they inherit our transform and layer inheritance)
        for child_id in self.layout_tree.children(node) {
            self.render_layer(
                ctx,
                child_id,
                (0.0, 0.0),
                target_layer,
                children_inside_glass,
                children_inside_foreground,
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

    /// Get the cursor style for a node
    ///
    /// Returns the cursor style if set on this node, None if not set.
    pub fn get_cursor(&self, node: LayoutNodeId) -> Option<crate::element::CursorStyle> {
        self.render_nodes.get(&node).and_then(|n| n.props.cursor)
    }

    /// Get the cursor style for the topmost hovered element at a point
    ///
    /// Walks up the ancestor chain starting from the topmost element,
    /// returning the first cursor style found. This allows child elements
    /// to override parent cursor styles.
    pub fn get_cursor_at(
        &self,
        router: &crate::event_router::EventRouter,
        x: f32,
        y: f32,
    ) -> Option<crate::element::CursorStyle> {
        // Hit test to find topmost element
        let hit = router.hit_test(self, x, y)?;

        // Check the hit node first
        if let Some(cursor) = self.get_cursor(hit.node) {
            return Some(cursor);
        }

        // Walk up ancestors (from leaf towards root) to find first cursor
        // Ancestors are stored from root to leaf, so iterate in reverse
        for &ancestor in hit.ancestors.iter().rev() {
            if let Some(cursor) = self.get_cursor(ancestor) {
                return Some(cursor);
            }
        }

        None
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

                    // Draw borders
                    if render_node.props.border_sides.has_any() {
                        let sides = &render_node.props.border_sides;

                        if let Some(ref border) = sides.left {
                            if border.is_visible() {
                                let border_rect = Rect::new(0.0, 0.0, border.width, rect.height());
                                ctx.fill_rect(
                                    border_rect,
                                    CornerRadius::default(),
                                    Brush::Solid(border.color),
                                );
                            }
                        }
                        if let Some(ref border) = sides.right {
                            if border.is_visible() {
                                let border_rect = Rect::new(
                                    rect.width() - border.width,
                                    0.0,
                                    border.width,
                                    rect.height(),
                                );
                                ctx.fill_rect(
                                    border_rect,
                                    CornerRadius::default(),
                                    Brush::Solid(border.color),
                                );
                            }
                        }
                        if let Some(ref border) = sides.top {
                            if border.is_visible() {
                                let border_rect = Rect::new(0.0, 0.0, rect.width(), border.width);
                                ctx.fill_rect(
                                    border_rect,
                                    CornerRadius::default(),
                                    Brush::Solid(border.color),
                                );
                            }
                        }
                        if let Some(ref border) = sides.bottom {
                            if border.is_visible() {
                                let border_rect = Rect::new(
                                    0.0,
                                    rect.height() - border.width,
                                    rect.width(),
                                    border.width,
                                );
                                ctx.fill_rect(
                                    border_rect,
                                    CornerRadius::default(),
                                    Brush::Solid(border.color),
                                );
                            }
                        }
                    } else if render_node.props.border_width > 0.0 {
                        if let Some(ref border_color) = render_node.props.border_color {
                            let stroke = Stroke::new(render_node.props.border_width);
                            ctx.stroke_rect(rect, radius, &stroke, Brush::Solid(*border_color));
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
            self.render_text_recursive(renderer, root, (0.0, 0.0), false, false);
        }
    }

    /// Recursively render text elements
    fn render_text_recursive<R: LayoutRenderer>(
        &self,
        renderer: &mut R,
        node: LayoutNodeId,
        parent_offset: (f32, f32),
        inside_glass: bool,
        inside_foreground: bool,
    ) {
        // Get bounds with (0,0) to get pure layout position relative to parent
        // parent_offset accumulates absolute position from ancestors + scroll + motion
        let Some(bounds) = self.layout_tree.get_bounds(node, (0.0, 0.0)) else {
            return;
        };

        let Some(render_node) = self.render_nodes.get(&node) else {
            return;
        };

        let is_glass = matches!(render_node.props.material, Some(Material::Glass(_)));
        let children_inside_glass = inside_glass || is_glass;

        // Track foreground inheritance
        let is_foreground = render_node.props.layer == RenderLayer::Foreground;
        let children_inside_foreground = inside_foreground || is_foreground;

        // Text inside glass or foreground goes to foreground layer
        let to_foreground =
            children_inside_glass || children_inside_foreground;

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

        // Calculate absolute position for this node's children:
        // - parent_offset: accumulated absolute position from ancestors (includes their scroll/motion)
        // - bounds.x/y: this node's position relative to parent (from Taffy layout)
        // - scroll_offset: this node's scroll offset (for scroll containers)
        // - motion_offset: this node's motion transform translation (for animated elements)
        let scroll_offset = self.get_scroll_offset(node);

        let motion_transform = self.get_motion_transform(node);
        let motion_offset = motion_transform
            .as_ref()
            .map(|t| match t {
                Transform::Affine2D(a) => (a.elements[4], a.elements[5]),
                _ => (0.0, 0.0),
            })
            .unwrap_or((0.0, 0.0));

        let new_offset = (
            parent_offset.0 + bounds.x + scroll_offset.0 + motion_offset.0,
            parent_offset.1 + bounds.y + scroll_offset.1 + motion_offset.1,
        );
        for child_id in self.layout_tree.children(node) {
            self.render_text_recursive(renderer, child_id, new_offset, children_inside_glass, children_inside_foreground);
        }
    }

    /// Render all SVG elements via the LayoutRenderer
    fn render_svg_elements<R: LayoutRenderer>(&self, renderer: &mut R) {
        if let Some(root) = self.root {
            self.render_svg_recursive(renderer, root, (0.0, 0.0), false, false);
        }
    }

    /// Recursively render SVG elements
    fn render_svg_recursive<R: LayoutRenderer>(
        &self,
        renderer: &mut R,
        node: LayoutNodeId,
        parent_offset: (f32, f32),
        inside_glass: bool,
        inside_foreground: bool,
    ) {
        // Get bounds with (0,0) to get pure layout position relative to parent
        // parent_offset accumulates absolute position from ancestors + scroll + motion
        let Some(bounds) = self.layout_tree.get_bounds(node, (0.0, 0.0)) else {
            return;
        };

        let Some(render_node) = self.render_nodes.get(&node) else {
            return;
        };

        let is_glass = matches!(render_node.props.material, Some(Material::Glass(_)));
        let children_inside_glass = inside_glass || is_glass;

        // Track foreground inheritance
        let is_foreground = render_node.props.layer == RenderLayer::Foreground;
        let children_inside_foreground = inside_foreground || is_foreground;

        // SVG inside glass or foreground goes to foreground layer
        let to_foreground = children_inside_glass || children_inside_foreground;

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

        // Calculate absolute position for this node's children:
        // - parent_offset: accumulated absolute position from ancestors (includes their scroll/motion)
        // - bounds.x/y: this node's position relative to parent (from Taffy layout)
        // - scroll_offset: this node's scroll offset (for scroll containers)
        // - motion_offset: this node's motion transform translation (for animated elements)
        let scroll_offset = self.get_scroll_offset(node);

        let motion_offset = self
            .get_motion_transform(node)
            .map(|t| match t {
                Transform::Affine2D(a) => (a.elements[4], a.elements[5]),
                _ => (0.0, 0.0),
            })
            .unwrap_or((0.0, 0.0));

        let new_offset = (
            parent_offset.0 + bounds.x + scroll_offset.0 + motion_offset.0,
            parent_offset.1 + bounds.y + scroll_offset.1 + motion_offset.1,
        );
        for child_id in self.layout_tree.children(node) {
            self.render_svg_recursive(renderer, child_id, new_offset, children_inside_glass, children_inside_foreground);
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

/// Apply opacity to a brush by modifying its alpha component
fn apply_opacity_to_brush(brush: &Brush, opacity: f32) -> Brush {
    match brush {
        Brush::Solid(color) => {
            Brush::Solid(Color::rgba(color.r, color.g, color.b, color.a * opacity))
        }
        Brush::Gradient(gradient) => {
            // For gradients, we'd need to modify both start and end colors
            // For now, just return the gradient as-is
            // TODO: Apply opacity to gradient stops
            Brush::Gradient(gradient.clone())
        }
        Brush::Glass(glass) => {
            // Glass already has its own opacity handling
            Brush::Glass(glass.clone())
        }
        Brush::Image(image) => {
            // Image brushes - return as-is for now
            // TODO: Apply opacity to image brush
            Brush::Image(image.clone())
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
