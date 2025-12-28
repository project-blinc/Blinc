//! Structural Diff and Merge Algorithm for Div Elements
//!
//! This module provides efficient tree diffing and reconciliation for UI elements:
//!
//! - **Hash-based identity**: Content hashes for stable child matching across rebuilds
//! - **Category-level change detection**: Layout, Visual, Children, Handlers
//! - **Tree reconciliation**: Update existing trees with minimal changes
//!
//! # Example
//!
//! ```ignore
//! use blinc_layout::diff::{diff, reconcile, ChangeCategory};
//!
//! let old = div().w(100.0).bg(Color::RED);
//! let new = div().w(100.0).bg(Color::BLUE);
//!
//! let result = diff(&old, &new);
//! assert!(result.changes.visual);      // Background changed
//! assert!(!result.changes.layout);     // Width unchanged
//! ```

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;

use blinc_core::{Brush, Color, Shadow, Transform, CornerRadius, GlassStyle, Gradient, GradientStop, ImageBrush};
use taffy::Style;

use crate::div::{Div, ElementBuilder, ElementTypeId};
use crate::element::{Material, RenderLayer, RenderProps};
use crate::event_handler::EventHandlers;
use crate::tree::LayoutNodeId;

// =============================================================================
// DivHash - Content Hash for Identity
// =============================================================================

/// Content hash for stable element identity matching.
///
/// Used to detect whether elements have changed and to match children
/// across tree rebuilds even when reordered.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DivHash(pub u64);

impl DivHash {
    /// Compute hash of a Div's own properties (excluding children).
    ///
    /// This is useful for detecting if a node's properties changed
    /// without considering its descendants.
    pub fn compute(div: &Div) -> Self {
        let mut hasher = DefaultHasher::new();
        hash_div_props(div, &mut hasher);
        DivHash(hasher.finish())
    }

    /// Compute hash including the entire subtree (recursive).
    ///
    /// This produces a unique hash for the entire tree structure,
    /// useful for quick equality checks.
    pub fn compute_tree(div: &Div) -> Self {
        let mut hasher = DefaultHasher::new();
        hash_div_tree(div, &mut hasher);
        DivHash(hasher.finish())
    }

    /// Compute hash for a trait object ElementBuilder.
    ///
    /// Works with any element type (Div, Text, SVG, etc.)
    pub fn compute_element(element: &dyn ElementBuilder) -> Self {
        let mut hasher = DefaultHasher::new();
        hash_element(element, &mut hasher);
        DivHash(hasher.finish())
    }

    /// Compute tree hash for a boxed ElementBuilder.
    pub fn compute_element_tree(element: &dyn ElementBuilder) -> Self {
        let mut hasher = DefaultHasher::new();
        hash_element_tree(element, &mut hasher);
        DivHash(hasher.finish())
    }
}

// =============================================================================
// ChangeCategory - What Changed
// =============================================================================

/// Categories of changes between two Div elements.
///
/// Used to determine what kind of update is needed:
/// - Visual-only changes can use prop updates (no layout)
/// - Layout changes require re-layout computation
/// - Children changes may require subtree rebuilds
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ChangeCategory {
    /// Layout-affecting properties changed (size, padding, margin, flex, position).
    /// Requires layout recomputation.
    pub layout: bool,

    /// Visual-only properties changed (background, shadow, opacity, transform).
    /// Can be updated via prop update without layout.
    pub visual: bool,

    /// Children changed (added, removed, reordered, or modified).
    pub children: bool,

    /// Event handlers changed (handlers added or removed).
    pub handlers: bool,
}

impl ChangeCategory {
    /// Create a ChangeCategory with no changes.
    pub fn none() -> Self {
        Self::default()
    }

    /// Returns true if any category has changes.
    pub fn any(&self) -> bool {
        self.layout || self.visual || self.children || self.handlers
    }

    /// Returns true if only visual properties changed.
    ///
    /// When true, the change can be applied via prop update without layout.
    pub fn visual_only(&self) -> bool {
        self.visual && !self.layout && !self.children
    }

    /// Returns true if layout needs recomputation.
    pub fn needs_layout(&self) -> bool {
        self.layout || self.children
    }
}

// =============================================================================
// DiffResult - Diff Output
// =============================================================================

/// Result of diffing two Div elements.
#[derive(Clone, Debug)]
pub struct DiffResult {
    /// What categories of changes occurred.
    pub changes: ChangeCategory,

    /// Children diff results (how children changed).
    pub child_diffs: Vec<ChildDiff>,

    /// Hash of the old element.
    pub old_hash: DivHash,

    /// Hash of the new element.
    pub new_hash: DivHash,
}

/// Result of diffing a single child position.
#[derive(Clone, Debug)]
pub enum ChildDiff {
    /// Child was unchanged (same hash, same position).
    Unchanged {
        index: usize,
    },

    /// Child moved from one position to another (same hash).
    Moved {
        old_index: usize,
        new_index: usize,
        hash: DivHash,
    },

    /// Child was modified (content changed, matched by position).
    Modified {
        old_index: usize,
        new_index: usize,
        diff: Box<DiffResult>,
    },

    /// New child was added.
    Added {
        index: usize,
        hash: DivHash,
    },

    /// Old child was removed.
    Removed {
        index: usize,
        hash: DivHash,
    },
}

// =============================================================================
// ReconcileActions - What to Do
// =============================================================================

/// Actions to take after reconciling a diff.
#[derive(Default)]
pub struct ReconcileActions {
    /// Prop updates to queue (visual-only changes).
    /// These go via `PENDING_PROP_UPDATES`.
    pub prop_updates: Vec<(LayoutNodeId, RenderProps)>,

    /// Node IDs of subtrees that need rebuilding.
    /// The caller should use these with `PendingSubtreeRebuild`.
    pub subtree_rebuild_ids: Vec<LayoutNodeId>,

    /// Whether layout needs recomputation.
    pub needs_layout: bool,
}

// =============================================================================
// Core Diff Functions
// =============================================================================

/// Compare two Div elements and produce a DiffResult.
///
/// # Example
///
/// ```ignore
/// let old = div().w(100.0).bg(Color::RED);
/// let new = div().w(200.0).bg(Color::RED);
///
/// let result = diff(&old, &new);
/// assert!(result.changes.layout);  // Width changed
/// assert!(!result.changes.visual); // Background unchanged
/// ```
pub fn diff(old: &Div, new: &Div) -> DiffResult {
    let old_hash = DivHash::compute(old);
    let new_hash = DivHash::compute(new);

    let mut changes = ChangeCategory::none();

    // Quick path: if hashes match, only diff children
    if old_hash == new_hash {
        let child_diffs = diff_children(&old.children, &new.children);
        if child_diffs.iter().any(|d| !matches!(d, ChildDiff::Unchanged { .. })) {
            changes.children = true;
        }

        return DiffResult {
            changes,
            child_diffs,
            old_hash,
            new_hash,
        };
    }

    // Detailed comparison for changed elements
    changes.layout = detect_layout_changes(&old.style, &new.style);
    changes.visual = detect_visual_changes(old, new);
    changes.handlers = detect_handler_changes(&old.event_handlers, &new.event_handlers);

    // Diff children
    let child_diffs = diff_children(&old.children, &new.children);
    if child_diffs.iter().any(|d| !matches!(d, ChildDiff::Unchanged { .. })) {
        changes.children = true;
    }

    DiffResult {
        changes,
        child_diffs,
        old_hash,
        new_hash,
    }
}

/// Diff children using content-hash-based matching.
///
/// This algorithm:
/// 1. Computes hashes for all children
/// 2. Matches children by hash (handles reordering)
/// 3. Detects additions, removals, and modifications
pub fn diff_children(
    old_children: &[Box<dyn ElementBuilder>],
    new_children: &[Box<dyn ElementBuilder>],
) -> Vec<ChildDiff> {
    // Compute hashes for all children
    let old_hashes: Vec<DivHash> = old_children
        .iter()
        .map(|c| DivHash::compute_element_tree(c.as_ref()))
        .collect();

    let new_hashes: Vec<DivHash> = new_children
        .iter()
        .map(|c| DivHash::compute_element_tree(c.as_ref()))
        .collect();

    // Build hash -> indices map for old children
    let mut old_hash_map: HashMap<DivHash, Vec<usize>> = HashMap::new();
    for (i, hash) in old_hashes.iter().enumerate() {
        old_hash_map.entry(*hash).or_default().push(i);
    }

    let mut results = Vec::new();
    let mut matched_old_indices: HashSet<usize> = HashSet::new();
    let mut added_indices: Vec<usize> = Vec::new();

    // First pass: match by hash (finds unchanged/moved children)
    for (new_idx, new_hash) in new_hashes.iter().enumerate() {
        if let Some(old_indices) = old_hash_map.get_mut(new_hash) {
            if let Some(old_idx) = old_indices.pop() {
                matched_old_indices.insert(old_idx);
                if old_idx == new_idx {
                    results.push(ChildDiff::Unchanged { index: new_idx });
                } else {
                    results.push(ChildDiff::Moved {
                        old_index: old_idx,
                        new_index: new_idx,
                        hash: *new_hash,
                    });
                }
                continue;
            }
        }

        // No hash match - mark as potentially added
        added_indices.push(new_idx);
        results.push(ChildDiff::Added {
            index: new_idx,
            hash: *new_hash,
        });
    }

    // Second pass: find removed children
    let mut removed_indices: Vec<usize> = Vec::new();
    for (old_idx, old_hash) in old_hashes.iter().enumerate() {
        if !matched_old_indices.contains(&old_idx) {
            removed_indices.push(old_idx);
            results.push(ChildDiff::Removed {
                index: old_idx,
                hash: *old_hash,
            });
        }
    }

    // Third pass: try to match Added with Removed at same index (Modified detection)
    // This handles cases where a child changed content at the same position
    let mut modified_pairs: Vec<(usize, usize)> = Vec::new(); // (result_idx_add, result_idx_rem)

    for (add_result_idx, add_diff) in results.iter().enumerate() {
        if let ChildDiff::Added { index: new_idx, .. } = add_diff {
            for (rem_result_idx, rem_diff) in results.iter().enumerate() {
                if let ChildDiff::Removed { index: old_idx, .. } = rem_diff {
                    if new_idx == old_idx {
                        // Same position: likely a modification
                        modified_pairs.push((add_result_idx, rem_result_idx));
                    }
                }
            }
        }
    }

    // Replace Add/Remove pairs at same index with Modified
    // We need to do this carefully to not invalidate indices
    let mut indices_to_remove: HashSet<usize> = HashSet::new();
    let mut modifications: Vec<(usize, ChildDiff)> = Vec::new();

    for (add_idx, rem_idx) in modified_pairs {
        if let (
            ChildDiff::Added { index: new_idx, .. },
            ChildDiff::Removed { index: old_idx, .. },
        ) = (&results[add_idx], &results[rem_idx])
        {
            // Recursively diff the children to get detailed changes
            let child_diff = diff_elements(
                old_children[*old_idx].as_ref(),
                new_children[*new_idx].as_ref(),
            );

            modifications.push((
                add_idx,
                ChildDiff::Modified {
                    old_index: *old_idx,
                    new_index: *new_idx,
                    diff: Box::new(child_diff),
                },
            ));
            indices_to_remove.insert(add_idx);
            indices_to_remove.insert(rem_idx);
        }
    }

    // Apply modifications
    for (idx, modification) in modifications {
        results[idx] = modification;
    }

    // Remove replaced entries (in reverse order to preserve indices)
    let mut indices_vec: Vec<usize> = indices_to_remove.into_iter().collect();
    indices_vec.sort_by(|a, b| b.cmp(a)); // Sort descending

    // Only remove the Removed entries, as Modified replaced Added
    for idx in indices_vec {
        if matches!(results[idx], ChildDiff::Removed { .. }) {
            results.remove(idx);
        }
    }

    results
}

/// Diff two ElementBuilder trait objects.
pub fn diff_elements(old: &dyn ElementBuilder, new: &dyn ElementBuilder) -> DiffResult {
    let old_hash = DivHash::compute_element(old);
    let new_hash = DivHash::compute_element(new);

    let mut changes = ChangeCategory::none();

    // Quick path
    if old_hash == new_hash {
        let child_diffs = diff_children(old.children_builders(), new.children_builders());
        if child_diffs.iter().any(|d| !matches!(d, ChildDiff::Unchanged { .. })) {
            changes.children = true;
        }
        return DiffResult {
            changes,
            child_diffs,
            old_hash,
            new_hash,
        };
    }

    // Check element type
    if old.element_type_id() != new.element_type_id() {
        // Different types - everything changed
        changes.layout = true;
        changes.visual = true;
        changes.children = true;
        return DiffResult {
            changes,
            child_diffs: vec![],
            old_hash,
            new_hash,
        };
    }

    // Compare render props for visual changes
    let old_props = old.render_props();
    let new_props = new.render_props();
    changes.visual = !render_props_eq(&old_props, &new_props);

    // For layout changes, we'd need access to the style
    // Since ElementBuilder doesn't expose style directly, mark as layout change
    // if the element hash changed and visual didn't explain it
    if !changes.visual && old_hash != new_hash {
        changes.layout = true;
    }

    // Diff children
    let child_diffs = diff_children(old.children_builders(), new.children_builders());
    if child_diffs.iter().any(|d| !matches!(d, ChildDiff::Unchanged { .. })) {
        changes.children = true;
    }

    DiffResult {
        changes,
        child_diffs,
        old_hash,
        new_hash,
    }
}

// =============================================================================
// Change Detection Functions
// =============================================================================

/// Detect if layout-affecting properties changed.
pub fn detect_layout_changes(old: &Style, new: &Style) -> bool {
    // Display & position
    old.display != new.display
        || old.position != new.position
        || old.overflow != new.overflow
        // Flex container
        || old.flex_direction != new.flex_direction
        || old.flex_wrap != new.flex_wrap
        || old.justify_content != new.justify_content
        || old.align_items != new.align_items
        || old.align_content != new.align_content
        || old.gap != new.gap
        // Flex item
        || old.flex_grow != new.flex_grow
        || old.flex_shrink != new.flex_shrink
        || old.flex_basis != new.flex_basis
        || old.align_self != new.align_self
        // Size
        || old.size != new.size
        || old.min_size != new.min_size
        || old.max_size != new.max_size
        || old.aspect_ratio != new.aspect_ratio
        // Spacing
        || old.margin != new.margin
        || old.padding != new.padding
        || old.border != new.border
        || old.inset != new.inset
}

/// Detect if visual-only properties changed.
pub fn detect_visual_changes(old: &Div, new: &Div) -> bool {
    !brush_eq(&old.background, &new.background)
        || old.border_radius != new.border_radius
        || old.render_layer != new.render_layer
        || !material_eq(&old.material, &new.material)
        || !shadow_eq(&old.shadow, &new.shadow)
        || !transform_eq(&old.transform, &new.transform)
        || !f32_eq(old.opacity, new.opacity)
}

/// Detect if event handlers changed (by registered event types).
pub fn detect_handler_changes(old: &EventHandlers, new: &EventHandlers) -> bool {
    let old_types: HashSet<_> = old.event_types().collect();
    let new_types: HashSet<_> = new.event_types().collect();
    old_types != new_types
}

// =============================================================================
// Reconciliation Functions
// =============================================================================

/// Apply a diff result to reconcile old into new.
///
/// Returns the actions needed to update the render tree.
pub fn reconcile(
    diff: &DiffResult,
    old: &mut Div,
    new: &Div,
    node_id: Option<LayoutNodeId>,
) -> ReconcileActions {
    let mut actions = ReconcileActions::default();

    // If only visual changes, queue prop update
    if diff.changes.visual_only() {
        if let Some(id) = node_id {
            actions.prop_updates.push((id, new.render_props()));
        }
        // Apply visual changes to old
        apply_visual_changes(old, new);
        return actions;
    }

    // If layout changed, apply layout changes and mark for relayout
    if diff.changes.layout {
        apply_layout_changes(old, new);
        actions.needs_layout = true;
    }

    // Also apply visual changes if they exist
    if diff.changes.visual {
        apply_visual_changes(old, new);
    }

    // If children changed, handle child reconciliation
    if diff.changes.children {
        reconcile_children(&diff.child_diffs, old, new, node_id, &mut actions);
    }

    actions
}

/// Apply visual-only changes from new to old.
fn apply_visual_changes(old: &mut Div, new: &Div) {
    old.background = new.background.clone();
    old.border_radius = new.border_radius;
    old.render_layer = new.render_layer;
    old.material = new.material.clone();
    old.shadow = new.shadow;
    old.transform = new.transform.clone();
    old.opacity = new.opacity;
}

/// Apply layout-affecting changes from new to old.
fn apply_layout_changes(old: &mut Div, new: &Div) {
    old.style = new.style.clone();
}

/// Reconcile children based on diff results.
fn reconcile_children(
    _diffs: &[ChildDiff],
    _old: &mut Div,
    _new: &Div,
    parent_node_id: Option<LayoutNodeId>,
    actions: &mut ReconcileActions,
) {
    // For now, we trigger a subtree rebuild when children change
    // A more sophisticated implementation could reorder/update in place
    if let Some(parent_id) = parent_node_id {
        // Mark that layout needs to be recomputed
        actions.needs_layout = true;
        // The actual subtree rebuild would be queued by the caller
        // using the PendingSubtreeRebuild mechanism
        let _ = parent_id; // Suppress unused warning
    }
}

// =============================================================================
// Hash Helper Functions
// =============================================================================

/// Hash a Div's own properties (not including children).
fn hash_div_props(div: &Div, hasher: &mut impl Hasher) {
    hash_style(&div.style, hasher);
    hash_option_brush(&div.background, hasher);
    hash_corner_radius(&div.border_radius, hasher);
    hash_render_layer(&div.render_layer, hasher);
    hash_option_material(&div.material, hasher);
    hash_option_shadow(&div.shadow, hasher);
    hash_option_transform(&div.transform, hasher);
    hash_f32(div.opacity, hasher);
}

/// Hash a Div including its entire subtree.
fn hash_div_tree(div: &Div, hasher: &mut impl Hasher) {
    hash_div_props(div, hasher);

    // Hash children count and each child
    div.children.len().hash(hasher);
    for child in &div.children {
        hash_element_tree(child.as_ref(), hasher);
    }
}

/// Hash an ElementBuilder (without children).
fn hash_element(element: &dyn ElementBuilder, hasher: &mut impl Hasher) {
    // Hash element type using discriminant
    std::mem::discriminant(&element.element_type_id()).hash(hasher);

    // Hash render props
    hash_render_props(&element.render_props(), hasher);

    // Hash type-specific data
    if let Some(text_info) = element.text_render_info() {
        text_info.content.hash(hasher);
        hash_f32(text_info.font_size, hasher);
        // color is [f32; 4]
        for c in &text_info.color {
            hash_f32(*c, hasher);
        }
    }

    if let Some(svg_info) = element.svg_render_info() {
        svg_info.source.hash(hasher);
        if let Some(tint) = &svg_info.tint {
            hash_color(tint, hasher);
        }
    }

    if let Some(image_info) = element.image_render_info() {
        image_info.source.hash(hasher);
        image_info.object_fit.hash(hasher);
        hash_f32(image_info.object_position[0], hasher);
        hash_f32(image_info.object_position[1], hasher);
        hash_f32(image_info.opacity, hasher);
        hash_f32(image_info.border_radius, hasher);
    }
}

/// Hash an ElementBuilder including its entire subtree.
fn hash_element_tree(element: &dyn ElementBuilder, hasher: &mut impl Hasher) {
    hash_element(element, hasher);

    // Hash children
    let children = element.children_builders();
    children.len().hash(hasher);
    for child in children {
        hash_element_tree(child.as_ref(), hasher);
    }
}

/// Hash RenderProps.
fn hash_render_props(props: &RenderProps, hasher: &mut impl Hasher) {
    hash_option_brush(&props.background, hasher);
    hash_corner_radius(&props.border_radius, hasher);
    hash_render_layer(&props.layer, hasher);
    hash_option_material(&props.material, hasher);
    hash_option_shadow(&props.shadow, hasher);
    hash_option_transform(&props.transform, hasher);
    hash_f32(props.opacity, hasher);
    props.clips_content.hash(hasher);
}

// =============================================================================
// Type-Specific Hash Helpers
// =============================================================================

fn hash_f32(value: f32, hasher: &mut impl Hasher) {
    value.to_bits().hash(hasher);
}

fn hash_color(color: &Color, hasher: &mut impl Hasher) {
    hash_f32(color.r, hasher);
    hash_f32(color.g, hasher);
    hash_f32(color.b, hasher);
    hash_f32(color.a, hasher);
}

fn hash_corner_radius(radius: &CornerRadius, hasher: &mut impl Hasher) {
    hash_f32(radius.top_left, hasher);
    hash_f32(radius.top_right, hasher);
    hash_f32(radius.bottom_right, hasher);
    hash_f32(radius.bottom_left, hasher);
}

fn hash_render_layer(layer: &RenderLayer, hasher: &mut impl Hasher) {
    layer.hash(hasher);
}

fn hash_shadow(shadow: &Shadow, hasher: &mut impl Hasher) {
    hash_f32(shadow.offset_x, hasher);
    hash_f32(shadow.offset_y, hasher);
    hash_f32(shadow.blur, hasher);
    hash_f32(shadow.spread, hasher);
    hash_color(&shadow.color, hasher);
}

fn hash_option_shadow(shadow: &Option<Shadow>, hasher: &mut impl Hasher) {
    match shadow {
        Some(s) => {
            1u8.hash(hasher);
            hash_shadow(s, hasher);
        }
        None => {
            0u8.hash(hasher);
        }
    }
}

fn hash_glass_style(glass: &GlassStyle, hasher: &mut impl Hasher) {
    hash_f32(glass.blur, hasher);
    hash_color(&glass.tint, hasher);
    hash_f32(glass.saturation, hasher);
    hash_f32(glass.brightness, hasher);
    hash_f32(glass.noise, hasher);
    hash_f32(glass.border_thickness, hasher);
    hash_option_shadow(&glass.shadow, hasher);
}

fn hash_gradient_stop(stop: &GradientStop, hasher: &mut impl Hasher) {
    hash_f32(stop.offset, hasher);
    hash_color(&stop.color, hasher);
}

fn hash_gradient(gradient: &Gradient, hasher: &mut impl Hasher) {
    match gradient {
        Gradient::Linear { start, end, stops, space, spread } => {
            0u8.hash(hasher);
            hash_f32(start.x, hasher);
            hash_f32(start.y, hasher);
            hash_f32(end.x, hasher);
            hash_f32(end.y, hasher);
            stops.len().hash(hasher);
            for stop in stops {
                hash_gradient_stop(stop, hasher);
            }
            std::mem::discriminant(space).hash(hasher);
            std::mem::discriminant(spread).hash(hasher);
        }
        Gradient::Radial { center, radius, focal, stops, space, spread } => {
            1u8.hash(hasher);
            hash_f32(center.x, hasher);
            hash_f32(center.y, hasher);
            hash_f32(*radius, hasher);
            // Hash optional focal point
            if let Some(f) = focal {
                1u8.hash(hasher);
                hash_f32(f.x, hasher);
                hash_f32(f.y, hasher);
            } else {
                0u8.hash(hasher);
            }
            stops.len().hash(hasher);
            for stop in stops {
                hash_gradient_stop(stop, hasher);
            }
            std::mem::discriminant(space).hash(hasher);
            std::mem::discriminant(spread).hash(hasher);
        }
        Gradient::Conic { center, start_angle, stops, space } => {
            2u8.hash(hasher);
            hash_f32(center.x, hasher);
            hash_f32(center.y, hasher);
            hash_f32(*start_angle, hasher);
            stops.len().hash(hasher);
            for stop in stops {
                hash_gradient_stop(stop, hasher);
            }
            std::mem::discriminant(space).hash(hasher);
        }
    }
}

fn hash_image_brush(brush: &ImageBrush, hasher: &mut impl Hasher) {
    brush.source.hash(hasher);
    std::mem::discriminant(&brush.fit).hash(hasher);
    hash_f32(brush.position.x, hasher);
    hash_f32(brush.position.y, hasher);
    hash_f32(brush.opacity, hasher);
    hash_color(&brush.tint, hasher);
}

fn hash_brush(brush: &Brush, hasher: &mut impl Hasher) {
    match brush {
        Brush::Solid(color) => {
            0u8.hash(hasher);
            hash_color(color, hasher);
        }
        Brush::Gradient(gradient) => {
            1u8.hash(hasher);
            hash_gradient(gradient, hasher);
        }
        Brush::Glass(glass) => {
            2u8.hash(hasher);
            hash_glass_style(glass, hasher);
        }
        Brush::Image(image) => {
            3u8.hash(hasher);
            hash_image_brush(image, hasher);
        }
    }
}

fn hash_option_brush(brush: &Option<Brush>, hasher: &mut impl Hasher) {
    match brush {
        Some(b) => {
            1u8.hash(hasher);
            hash_brush(b, hasher);
        }
        None => {
            0u8.hash(hasher);
        }
    }
}

fn hash_transform(transform: &Transform, hasher: &mut impl Hasher) {
    match transform {
        Transform::Affine2D(affine) => {
            0u8.hash(hasher);
            for elem in &affine.elements {
                hash_f32(*elem, hasher);
            }
        }
        Transform::Mat4(mat) => {
            1u8.hash(hasher);
            for row in &mat.cols {
                for elem in row {
                    hash_f32(*elem, hasher);
                }
            }
        }
    }
}

fn hash_option_transform(transform: &Option<Transform>, hasher: &mut impl Hasher) {
    match transform {
        Some(t) => {
            1u8.hash(hasher);
            hash_transform(t, hasher);
        }
        None => {
            0u8.hash(hasher);
        }
    }
}

fn hash_material(material: &Material, hasher: &mut impl Hasher) {
    match material {
        Material::Glass(glass) => {
            0u8.hash(hasher);
            hash_f32(glass.blur, hasher);
            hash_color(&glass.tint, hasher);
            hash_f32(glass.saturation, hasher);
            hash_f32(glass.brightness, hasher);
            hash_f32(glass.noise, hasher);
        }
        Material::Metallic(metal) => {
            1u8.hash(hasher);
            hash_color(&metal.color, hasher);
            hash_f32(metal.roughness, hasher);
            hash_f32(metal.metallic, hasher);
            hash_f32(metal.reflection, hasher);
        }
        Material::Wood(wood) => {
            2u8.hash(hasher);
            hash_color(&wood.color, hasher);
            hash_f32(wood.grain, hasher);
            hash_f32(wood.gloss, hasher);
        }
        Material::Solid(solid) => {
            3u8.hash(hasher);
            // SolidMaterial only has shadow field
            hash_option_material_shadow(&solid.shadow, hasher);
        }
    }
}

fn hash_option_material_shadow(shadow: &Option<crate::element::MaterialShadow>, hasher: &mut impl Hasher) {
    match shadow {
        Some(s) => {
            1u8.hash(hasher);
            hash_f32(s.offset.0, hasher);
            hash_f32(s.offset.1, hasher);
            hash_f32(s.blur, hasher);
            hash_f32(s.opacity, hasher);
            hash_color(&s.color, hasher);
        }
        None => {
            0u8.hash(hasher);
        }
    }
}

fn hash_option_material(material: &Option<Material>, hasher: &mut impl Hasher) {
    match material {
        Some(m) => {
            1u8.hash(hasher);
            hash_material(m, hasher);
        }
        None => {
            0u8.hash(hasher);
        }
    }
}

fn hash_style(style: &Style, hasher: &mut impl Hasher) {
    // Display & position
    std::mem::discriminant(&style.display).hash(hasher);
    std::mem::discriminant(&style.position).hash(hasher);
    std::mem::discriminant(&style.overflow.x).hash(hasher);
    std::mem::discriminant(&style.overflow.y).hash(hasher);

    // Flex container
    std::mem::discriminant(&style.flex_direction).hash(hasher);
    std::mem::discriminant(&style.flex_wrap).hash(hasher);
    hash_option_justify(&style.justify_content, hasher);
    hash_option_align(&style.align_items, hasher);
    hash_option_align_content(&style.align_content, hasher);
    hash_taffy_size_lp(&style.gap, hasher);

    // Flex item
    hash_f32(style.flex_grow, hasher);
    hash_f32(style.flex_shrink, hasher);
    hash_dimension(&style.flex_basis, hasher);
    hash_option_align_self(&style.align_self, hasher);

    // Size
    hash_taffy_size_dim(&style.size, hasher);
    hash_taffy_size_dim(&style.min_size, hasher);
    hash_taffy_size_dim(&style.max_size, hasher);
    hash_option_f32(&style.aspect_ratio, hasher);

    // Spacing
    hash_rect_lpa(&style.margin, hasher);
    hash_rect_lp(&style.padding, hasher);
    hash_rect_lp(&style.border, hasher);
    hash_rect_lpa(&style.inset, hasher);
}

// Taffy type hashers
fn hash_dimension(dim: &taffy::Dimension, hasher: &mut impl Hasher) {
    match dim {
        taffy::Dimension::Auto => 0u8.hash(hasher),
        taffy::Dimension::Length(v) => {
            1u8.hash(hasher);
            hash_f32(*v, hasher);
        }
        taffy::Dimension::Percent(v) => {
            2u8.hash(hasher);
            hash_f32(*v, hasher);
        }
    }
}

fn hash_length_percentage(lp: &taffy::LengthPercentage, hasher: &mut impl Hasher) {
    match lp {
        taffy::LengthPercentage::Length(v) => {
            0u8.hash(hasher);
            hash_f32(*v, hasher);
        }
        taffy::LengthPercentage::Percent(v) => {
            1u8.hash(hasher);
            hash_f32(*v, hasher);
        }
    }
}

fn hash_length_percentage_auto(lpa: &taffy::LengthPercentageAuto, hasher: &mut impl Hasher) {
    match lpa {
        taffy::LengthPercentageAuto::Auto => 0u8.hash(hasher),
        taffy::LengthPercentageAuto::Length(v) => {
            1u8.hash(hasher);
            hash_f32(*v, hasher);
        }
        taffy::LengthPercentageAuto::Percent(v) => {
            2u8.hash(hasher);
            hash_f32(*v, hasher);
        }
    }
}

fn hash_taffy_size_dim(size: &taffy::Size<taffy::Dimension>, hasher: &mut impl Hasher) {
    hash_dimension(&size.width, hasher);
    hash_dimension(&size.height, hasher);
}

fn hash_taffy_size_lp(size: &taffy::Size<taffy::LengthPercentage>, hasher: &mut impl Hasher) {
    hash_length_percentage(&size.width, hasher);
    hash_length_percentage(&size.height, hasher);
}

fn hash_rect_lp(rect: &taffy::Rect<taffy::LengthPercentage>, hasher: &mut impl Hasher) {
    hash_length_percentage(&rect.left, hasher);
    hash_length_percentage(&rect.right, hasher);
    hash_length_percentage(&rect.top, hasher);
    hash_length_percentage(&rect.bottom, hasher);
}

fn hash_rect_lpa(rect: &taffy::Rect<taffy::LengthPercentageAuto>, hasher: &mut impl Hasher) {
    hash_length_percentage_auto(&rect.left, hasher);
    hash_length_percentage_auto(&rect.right, hasher);
    hash_length_percentage_auto(&rect.top, hasher);
    hash_length_percentage_auto(&rect.bottom, hasher);
}

fn hash_option_f32(opt: &Option<f32>, hasher: &mut impl Hasher) {
    match opt {
        Some(v) => {
            1u8.hash(hasher);
            hash_f32(*v, hasher);
        }
        None => 0u8.hash(hasher),
    }
}

fn hash_option_justify(opt: &Option<taffy::JustifyContent>, hasher: &mut impl Hasher) {
    match opt {
        Some(v) => {
            1u8.hash(hasher);
            std::mem::discriminant(v).hash(hasher);
        }
        None => 0u8.hash(hasher),
    }
}

fn hash_option_align(opt: &Option<taffy::AlignItems>, hasher: &mut impl Hasher) {
    match opt {
        Some(v) => {
            1u8.hash(hasher);
            std::mem::discriminant(v).hash(hasher);
        }
        None => 0u8.hash(hasher),
    }
}

fn hash_option_align_content(opt: &Option<taffy::AlignContent>, hasher: &mut impl Hasher) {
    match opt {
        Some(v) => {
            1u8.hash(hasher);
            std::mem::discriminant(v).hash(hasher);
        }
        None => 0u8.hash(hasher),
    }
}

fn hash_option_align_self(opt: &Option<taffy::AlignSelf>, hasher: &mut impl Hasher) {
    match opt {
        Some(v) => {
            1u8.hash(hasher);
            std::mem::discriminant(v).hash(hasher);
        }
        None => 0u8.hash(hasher),
    }
}

// =============================================================================
// Equality Helpers (for change detection)
// =============================================================================

fn f32_eq(a: f32, b: f32) -> bool {
    (a - b).abs() < f32::EPSILON
}

fn color_eq(a: &Color, b: &Color) -> bool {
    f32_eq(a.r, b.r) && f32_eq(a.g, b.g) && f32_eq(a.b, b.b) && f32_eq(a.a, b.a)
}

fn shadow_eq(a: &Option<Shadow>, b: &Option<Shadow>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            f32_eq(a.offset_x, b.offset_x)
                && f32_eq(a.offset_y, b.offset_y)
                && f32_eq(a.blur, b.blur)
                && f32_eq(a.spread, b.spread)
                && color_eq(&a.color, &b.color)
        }
        _ => false,
    }
}

fn transform_eq(a: &Option<Transform>, b: &Option<Transform>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(Transform::Affine2D(a)), Some(Transform::Affine2D(b))) => {
            a.elements
                .iter()
                .zip(b.elements.iter())
                .all(|(x, y)| f32_eq(*x, *y))
        }
        (Some(Transform::Mat4(a)), Some(Transform::Mat4(b))) => {
            a.cols.iter().flatten().zip(b.cols.iter().flatten()).all(|(x, y)| f32_eq(*x, *y))
        }
        _ => false,
    }
}

fn brush_eq(a: &Option<Brush>, b: &Option<Brush>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(Brush::Solid(a)), Some(Brush::Solid(b))) => color_eq(a, b),
        (Some(Brush::Glass(a)), Some(Brush::Glass(b))) => {
            f32_eq(a.blur, b.blur)
                && color_eq(&a.tint, &b.tint)
                && f32_eq(a.saturation, b.saturation)
                && f32_eq(a.brightness, b.brightness)
                && f32_eq(a.noise, b.noise)
                && f32_eq(a.border_thickness, b.border_thickness)
                && shadow_eq(&a.shadow, &b.shadow)
        }
        (Some(Brush::Image(a)), Some(Brush::Image(b))) => {
            a.source == b.source
                && a.fit == b.fit
                && a.position == b.position
                && f32_eq(a.opacity, b.opacity)
                && color_eq(&a.tint, &b.tint)
        }
        (Some(Brush::Gradient(_)), Some(Brush::Gradient(_))) => {
            // For gradients, fall back to hash comparison
            let mut ha = DefaultHasher::new();
            let mut hb = DefaultHasher::new();
            hash_brush(a.as_ref().unwrap(), &mut ha);
            hash_brush(b.as_ref().unwrap(), &mut hb);
            ha.finish() == hb.finish()
        }
        _ => false,
    }
}

fn material_eq(a: &Option<Material>, b: &Option<Material>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(Material::Glass(a)), Some(Material::Glass(b))) => {
            f32_eq(a.blur, b.blur)
                && color_eq(&a.tint, &b.tint)
                && f32_eq(a.saturation, b.saturation)
                && f32_eq(a.brightness, b.brightness)
                && f32_eq(a.noise, b.noise)
        }
        (Some(Material::Metallic(a)), Some(Material::Metallic(b))) => {
            color_eq(&a.color, &b.color)
                && f32_eq(a.roughness, b.roughness)
                && f32_eq(a.metallic, b.metallic)
                && f32_eq(a.reflection, b.reflection)
        }
        (Some(Material::Wood(a)), Some(Material::Wood(b))) => {
            color_eq(&a.color, &b.color)
                && f32_eq(a.grain, b.grain)
                && f32_eq(a.gloss, b.gloss)
        }
        (Some(Material::Solid(a)), Some(Material::Solid(b))) => {
            material_shadow_eq(&a.shadow, &b.shadow)
        }
        _ => false,
    }
}

fn material_shadow_eq(a: &Option<crate::element::MaterialShadow>, b: &Option<crate::element::MaterialShadow>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            f32_eq(a.offset.0, b.offset.0)
                && f32_eq(a.offset.1, b.offset.1)
                && f32_eq(a.blur, b.blur)
                && f32_eq(a.opacity, b.opacity)
                && color_eq(&a.color, &b.color)
        }
        _ => false,
    }
}

fn render_props_eq(a: &RenderProps, b: &RenderProps) -> bool {
    brush_eq(&a.background, &b.background)
        && a.border_radius == b.border_radius
        && a.layer == b.layer
        && material_eq(&a.material, &b.material)
        && shadow_eq(&a.shadow, &b.shadow)
        && transform_eq(&a.transform, &b.transform)
        && f32_eq(a.opacity, b.opacity)
        && a.clips_content == b.clips_content
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::div::div;

    #[test]
    fn test_hash_stability() {
        let div1 = div().w(100.0).h(50.0);
        let div2 = div().w(100.0).h(50.0);

        let hash1 = DivHash::compute(&div1);
        let hash2 = DivHash::compute(&div2);

        assert_eq!(hash1, hash2, "Same properties should produce same hash");
    }

    #[test]
    fn test_hash_different_props() {
        let div1 = div().w(100.0);
        let div2 = div().w(200.0);

        let hash1 = DivHash::compute(&div1);
        let hash2 = DivHash::compute(&div2);

        assert_ne!(hash1, hash2, "Different properties should produce different hashes");
    }

    #[test]
    fn test_change_category_visual_only() {
        let cat = ChangeCategory {
            layout: false,
            visual: true,
            children: false,
            handlers: false,
        };

        assert!(cat.visual_only());
        assert!(!cat.needs_layout());
    }

    #[test]
    fn test_change_category_needs_layout() {
        let cat = ChangeCategory {
            layout: true,
            visual: false,
            children: false,
            handlers: false,
        };

        assert!(!cat.visual_only());
        assert!(cat.needs_layout());
    }

    #[test]
    fn test_diff_unchanged() {
        let div1 = div().w(100.0).h(50.0);
        let div2 = div().w(100.0).h(50.0);

        let result = diff(&div1, &div2);

        assert!(!result.changes.any(), "Identical divs should have no changes");
    }

    #[test]
    fn test_diff_layout_change() {
        let div1 = div().w(100.0);
        let div2 = div().w(200.0);

        let result = diff(&div1, &div2);

        assert!(result.changes.layout, "Width change should be detected as layout change");
    }

    #[test]
    fn test_diff_visual_change() {
        let div1 = div().opacity(1.0);
        let div2 = div().opacity(0.5);

        let result = diff(&div1, &div2);

        assert!(result.changes.visual, "Opacity change should be detected as visual change");
    }
}
