//! Build and render text, layout and motion debug overlays.
use super::{
    Color, DebugBoundsElement, Div, ElementType, GpuPrimitive, LayoutNodeId, RenderContext,
    RenderTree, TextElement,
};

impl RenderContext {
    /// Render debug visualization overlays for text elements
    ///
    /// When `BLINC_DEBUG=text` (or `1`, `all`, `true`) is set, this renders:
    /// - Cyan: Text bounding box outline
    /// - Magenta: Baseline position
    /// - Green: Top of bounding box (ascender reference)
    /// - Yellow: Bottom of bounding box (descender reference)
    pub(super) fn render_text_debug(&mut self, target: &wgpu::TextureView, texts: &[TextElement]) {
        let debug_primitives = generate_text_debug_primitives(texts);
        if !debug_primitives.is_empty() {
            self.renderer
                .render_primitives_overlay(target, &debug_primitives);
        }
    }

    /// Render debug visualization overlays for all layout elements
    ///
    /// When `BLINC_DEBUG=layout` (or `all`) is set, this renders:
    /// - Semi-transparent colored rectangles for each element's bounding box
    /// - Colors cycle based on tree depth to distinguish nested elements
    pub(super) fn render_layout_debug(
        &mut self,
        target: &wgpu::TextureView,
        tree: &RenderTree,
        scale: f32,
    ) {
        let debug_bounds = collect_debug_bounds(tree, scale);
        let debug_primitives = generate_layout_debug_primitives(&debug_bounds);
        if !debug_primitives.is_empty() {
            self.renderer
                .render_primitives_overlay(target, &debug_primitives);
        }
    }

    /// Render debug visualization for motion/animations
    ///
    /// When `BLINC_DEBUG=motion` (or `all`) is set, this renders:
    /// - Top-right corner overlay showing animation stats
    /// - Number of active visual animations, layout animations, etc.
    pub(super) fn render_motion_debug(
        &mut self,
        target: &wgpu::TextureView,
        tree: &RenderTree,
        width: u32,
        _height: u32,
    ) {
        let stats = tree.debug_stats();
        let mut debug_primitives = Vec::new();

        // Background for the debug panel
        let panel_width = 200.0;
        let panel_height = 100.0;
        let panel_x = width as f32 - panel_width - 10.0;
        let panel_y = 10.0;

        // Semi-transparent dark background
        debug_primitives.push(
            GpuPrimitive::rect(panel_x, panel_y, panel_width, panel_height)
                .with_color(0.1, 0.1, 0.15, 0.85)
                .with_corner_radius(6.0),
        );

        // Status indicator - green if any animations active
        let has_active = stats.visual_animation_count > 0
            || stats.layout_animation_count > 0
            || stats.animated_bounds_count > 0;

        let (r, g, b, a) = if has_active {
            (0.2, 0.9, 0.3, 1.0) // Green when animating
        } else {
            (0.4, 0.4, 0.5, 1.0) // Gray when idle
        };

        debug_primitives.push(
            GpuPrimitive::rect(panel_x + 10.0, panel_y + 12.0, 10.0, 10.0)
                .with_color(r, g, b, a)
                .with_corner_radius(5.0),
        );

        // Visual bars showing animation counts
        let bar_x = panel_x + 12.0;
        let bar_width = panel_width - 24.0;
        let bar_height = 6.0;

        // Visual animations bar (cyan)
        let visual_ratio = (stats.visual_animation_count as f32).min(10.0) / 10.0;
        if visual_ratio > 0.0 {
            debug_primitives.push(
                GpuPrimitive::rect(bar_x, panel_y + 35.0, bar_width * visual_ratio, bar_height)
                    .with_color(0.0, 0.8, 0.9, 0.9)
                    .with_corner_radius(3.0),
            );
        }

        // Layout animations bar (magenta)
        let layout_ratio = (stats.layout_animation_count as f32).min(10.0) / 10.0;
        if layout_ratio > 0.0 {
            debug_primitives.push(
                GpuPrimitive::rect(bar_x, panel_y + 50.0, bar_width * layout_ratio, bar_height)
                    .with_color(0.9, 0.2, 0.8, 0.9)
                    .with_corner_radius(3.0),
            );
        }

        // Animated bounds bar (yellow)
        let bounds_ratio = (stats.animated_bounds_count as f32).min(50.0) / 50.0;
        if bounds_ratio > 0.0 {
            debug_primitives.push(
                GpuPrimitive::rect(bar_x, panel_y + 65.0, bar_width * bounds_ratio, bar_height)
                    .with_color(0.95, 0.85, 0.2, 0.9)
                    .with_corner_radius(3.0),
            );
        }

        // Scroll physics indicator (orange dots)
        let scroll_count = stats.scroll_physics_count.min(8);
        for i in 0..scroll_count {
            debug_primitives.push(
                GpuPrimitive::rect(bar_x + (i as f32 * 14.0), panel_y + 80.0, 8.0, 8.0)
                    .with_color(1.0, 0.6, 0.2, 0.9)
                    .with_corner_radius(4.0),
            );
        }

        if !debug_primitives.is_empty() {
            self.renderer
                .render_primitives_overlay(target, &debug_primitives);
        }
    }
}

/// Generate debug primitives for text elements
///
/// Creates visual overlays showing:
/// - Bounding box outline (cyan)
/// - Baseline position (magenta line)
/// - Ascender line (green, at top of bounding box)
/// - Descender line (yellow, at bottom of bounding box)
fn generate_text_debug_primitives(texts: &[TextElement]) -> Vec<GpuPrimitive> {
    let mut primitives = Vec::new();

    for text in texts {
        // Use the measured text width unless wrapping constrains it to the layout box.
        let debug_width = if text.wrap && text.measured_width > text.width {
            // Text is wrapping - use container width
            text.width
        } else {
            // Single line - use actual measured width (clamped to layout width)
            text.measured_width.min(text.width)
        };

        // Bounding box outline (cyan, semi-transparent)
        let bbox = GpuPrimitive::rect(text.x, text.y, debug_width, text.height)
            .with_color(0.0, 0.0, 0.0, 0.0) // Transparent fill
            .with_border(1.0, 0.0, 1.0, 1.0, 0.7); // Cyan border
        primitives.push(bbox);

        // Baseline indicator (magenta horizontal line)
        // The baseline is at y + ascender
        let baseline_y = text.y + text.half_leading + text.ascender;
        let baseline = GpuPrimitive::rect(text.x, baseline_y - 0.5, debug_width, 1.0)
            .with_color(1.0, 0.0, 1.0, 0.6); // Magenta
        primitives.push(baseline);

        // Ascender line indicator (green, at top of text)
        // For v_baseline texts, this shows where the ascender sits
        let ascender_line = GpuPrimitive::rect(text.x, text.y - 0.5, debug_width, 1.0)
            .with_color(0.0, 1.0, 0.0, 0.4); // Green, more transparent
        primitives.push(ascender_line);

        // Descender line (yellow, at bottom of bounding box)
        let descender_y = text.y + text.height;
        let descender_line = GpuPrimitive::rect(text.x, descender_y - 0.5, debug_width, 1.0)
            .with_color(1.0, 1.0, 0.0, 0.4); // Yellow
        primitives.push(descender_line);
    }

    primitives
}

/// Collect all element bounds from the render tree for debug visualization
fn collect_debug_bounds(tree: &RenderTree, scale: f32) -> Vec<DebugBoundsElement> {
    let mut bounds = Vec::new();

    if let Some(root) = tree.root() {
        collect_debug_bounds_recursive(tree, root, (0.0, 0.0), 0, scale, &mut bounds);
    }

    bounds
}

/// Recursively collect bounds from all nodes
fn collect_debug_bounds_recursive(
    tree: &RenderTree,
    node: LayoutNodeId,
    parent_offset: (f32, f32),
    depth: u32,
    scale: f32,
    bounds: &mut Vec<DebugBoundsElement>,
) {
    use blinc_layout::renderer::ElementType;

    let Some(node_bounds) = tree.layout().get_bounds(node, parent_offset) else {
        return;
    };

    // Determine element type name
    let element_type = tree
        .get_render_node(node)
        .map(|n| match &n.element_type {
            ElementType::Div => "Div".to_string(),
            ElementType::Text(_) => "Text".to_string(),
            ElementType::StyledText(_) => "StyledText".to_string(),
            ElementType::Image(_) => "Image".to_string(),
            ElementType::Svg(_) => "Svg".to_string(),
            ElementType::Canvas(_) => "Canvas".to_string(),
        })
        .unwrap_or_else(|| "Unknown".to_string());

    // Add this element's bounds (with DPI scaling)
    bounds.push(DebugBoundsElement {
        x: node_bounds.x * scale,
        y: node_bounds.y * scale,
        width: node_bounds.width * scale,
        height: node_bounds.height * scale,
        element_type,
        depth,
    });

    // Get scroll offset for this node (scroll containers offset their children)
    let scroll_offset = tree.get_scroll_offset(node);

    // Calculate new offset for children (including scroll offset)
    let new_offset = (
        node_bounds.x + scroll_offset.0,
        node_bounds.y + scroll_offset.1,
    );

    // Recurse into children
    for child in tree.layout().children(node) {
        collect_debug_bounds_recursive(tree, child, new_offset, depth + 1, scale, bounds);
    }
}

/// Generate debug primitives for layout element bounds
///
/// Creates visual overlays showing:
/// - Colored outlines for each element's bounding box
/// - Colors cycle based on tree depth (red, green, blue, yellow, cyan, magenta)
fn generate_layout_debug_primitives(bounds: &[DebugBoundsElement]) -> Vec<GpuPrimitive> {
    let mut primitives = Vec::new();

    // Color palette for different depths (cycling)
    let colors: [(f32, f32, f32); 6] = [
        (1.0, 0.3, 0.3), // Red
        (0.3, 1.0, 0.3), // Green
        (0.3, 0.3, 1.0), // Blue
        (1.0, 1.0, 0.3), // Yellow
        (0.3, 1.0, 1.0), // Cyan
        (1.0, 0.3, 1.0), // Magenta
    ];

    for elem in bounds {
        // Skip very small elements (likely invisible)
        if elem.width < 1.0 || elem.height < 1.0 {
            continue;
        }

        let (r, g, b) = colors[(elem.depth as usize) % colors.len()];
        let alpha = 0.5; // Semi-transparent outline

        // Draw outline only (transparent fill with colored border)
        let rect = GpuPrimitive::rect(elem.x, elem.y, elem.width, elem.height)
            .with_color(0.0, 0.0, 0.0, 0.0) // Transparent fill
            .with_border(1.0, r, g, b, alpha); // Colored border

        primitives.push(rect);
    }

    primitives
}
