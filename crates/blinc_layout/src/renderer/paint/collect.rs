//! Tree walkers that collect render artefacts without painting.
//!
//! All three walkers visit the layout tree DFS, accumulating
//! per-node data into a flat `Vec` for downstream consumers
//! (mostly external test/debug surfaces and the deprecated
//! glass-panel API). They share the same scroll-offset accumulation
//! logic the painters use, so positions returned are absolute in
//! window space.
//!
//! - `collect_glass_panels` / `_recursive` — deprecated; the glass
//!   pipeline went through `Brush::Glass` years ago. Kept for
//!   external callers that still depend on the surface.
//! - `text_elements` / `collect_text_elements` — every text node
//!   with its absolute bounds. Used by external tooling that needs
//!   to inspect rendered text without spinning up a `LayoutRenderer`.
//! - `svg_elements` / `collect_svg_elements` — same thing for SVGs.

#[allow(deprecated)]
use super::super::GlassPanel;
use super::super::{ElementType, RenderTree, SvgData, TextData};
use crate::element::{ElementBounds, Material};
use crate::tree::LayoutNodeId;

impl RenderTree {
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
            self.collect_text_elements(root, (0.0, 0.0), &mut result, (0.0, 0.0));
        }
        result
    }

    fn collect_text_elements(
        &self,
        node: LayoutNodeId,
        parent_offset: (f32, f32),
        result: &mut Vec<(TextData, ElementBounds)>,
        cumulative_scroll: (f32, f32),
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
        // Reset cumulative scroll when entering a scroll container.
        // Sticky/fixed positioning is relative to the nearest scroll ancestor, not all ancestors.
        let is_scroll_container = self.scroll_physics.contains_key(&node);
        let new_cumulative_scroll = if is_scroll_container {
            (scroll_offset.0, scroll_offset.1)
        } else {
            (
                cumulative_scroll.0 + scroll_offset.0,
                cumulative_scroll.1 + scroll_offset.1,
            )
        };
        for child_id in self.layout_tree.children(node) {
            let child_render = self.render_nodes.get(&child_id);
            let child_is_fixed = child_render.map(|n| n.props.is_fixed).unwrap_or(false);
            let child_is_sticky = child_render.map(|n| n.props.is_sticky).unwrap_or(false);

            let mut child_offset = new_offset;

            let child_cumulative = if child_is_fixed {
                child_offset.0 -= new_cumulative_scroll.0;
                child_offset.1 -= new_cumulative_scroll.1;
                (0.0, 0.0)
            } else if child_is_sticky {
                if let Some(threshold) = child_render.and_then(|n| n.props.sticky_top) {
                    if let Some(cb) = self.layout_tree.get_bounds(child_id, (0.0, 0.0)) {
                        let visual_y = cb.y + new_cumulative_scroll.1;
                        if visual_y < threshold {
                            let correction = threshold - visual_y;
                            child_offset.1 += correction;
                        }
                    }
                }
                new_cumulative_scroll
            } else {
                new_cumulative_scroll
            };

            self.collect_text_elements(child_id, child_offset, result, child_cumulative);
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
            self.collect_svg_elements(root, (0.0, 0.0), &mut result, (0.0, 0.0));
        }
        result
    }

    fn collect_svg_elements(
        &self,
        node: LayoutNodeId,
        parent_offset: (f32, f32),
        result: &mut Vec<(SvgData, ElementBounds)>,
        cumulative_scroll: (f32, f32),
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
        // Reset cumulative scroll when entering a scroll container.
        // Sticky/fixed positioning is relative to the nearest scroll ancestor, not all ancestors.
        let is_scroll_container = self.scroll_physics.contains_key(&node);
        let new_cumulative_scroll = if is_scroll_container {
            (scroll_offset.0, scroll_offset.1)
        } else {
            (
                cumulative_scroll.0 + scroll_offset.0,
                cumulative_scroll.1 + scroll_offset.1,
            )
        };
        for child_id in self.layout_tree.children(node) {
            let child_render = self.render_nodes.get(&child_id);
            let child_is_fixed = child_render.map(|n| n.props.is_fixed).unwrap_or(false);
            let child_is_sticky = child_render.map(|n| n.props.is_sticky).unwrap_or(false);

            let mut child_offset = new_offset;

            let child_cumulative = if child_is_fixed {
                child_offset.0 -= new_cumulative_scroll.0;
                child_offset.1 -= new_cumulative_scroll.1;
                (0.0, 0.0)
            } else if child_is_sticky {
                if let Some(threshold) = child_render.and_then(|n| n.props.sticky_top) {
                    if let Some(cb) = self.layout_tree.get_bounds(child_id, (0.0, 0.0)) {
                        let visual_y = cb.y + new_cumulative_scroll.1;
                        if visual_y < threshold {
                            let correction = threshold - visual_y;
                            child_offset.1 += correction;
                        }
                    }
                }
                new_cumulative_scroll
            } else {
                new_cumulative_scroll
            };

            self.collect_svg_elements(child_id, child_offset, result, child_cumulative);
        }
    }
}
