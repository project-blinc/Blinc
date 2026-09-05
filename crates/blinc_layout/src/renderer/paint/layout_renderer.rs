//! `LayoutRenderer`-trait painters.
//!
//! `render_to<R: LayoutRenderer>` is the alternate paint surface
//! for callers that want background / foreground / glass / text /
//! svg fan-out routed through their own rendering frontend instead
//! of a single `DrawContext`. Used today mostly by the layered
//! desktop test harness; the production path is `render_with_motion`
//! (see `paint/motion.rs`).
//!
//! Five `pub(crate)` methods live here:
//!
//! - `render_to` — the public entry. Splits into background and
//!   foreground passes, recurses through `render_layer_with_content`
//!   for primitives, then drives separate text/svg recursives.
//! - `render_layer_with_content` — the core layered walker. Same
//!   shape as `render_layer` but routes per-layer primitives through
//!   the renderer's `background()` / `foreground()` / `glass()`
//!   methods rather than a flat `DrawContext`.
//! - `render_text_elements` / `render_text_recursive` — text-only
//!   walker that pushes glyph runs into `renderer.text(...)` after
//!   every `LayoutRenderer.background()` and `.foreground()` pass.
//!   Skips inside-glass nodes when called from background pass.
//! - `render_svg_elements` / `render_svg_recursive` — same shape for
//!   SVG nodes via `renderer.svg(...)`.

use blinc_core::{
    Brush, ClipShape, Color, CornerRadius, DrawContext, GlassStyle, Rect, Stroke, Transform,
};

use crate::canvas::CanvasBounds;
use crate::element::{Material, RenderLayer};
use crate::tree::LayoutNodeId;

use super::super::{ElementType, LayoutRenderer, RenderTree};

impl RenderTree {
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
                    0,
                    (0.0, 0.0),
                );
            }

            // Pass 2: Glass elements (to background context)
            {
                let ctx = renderer.background();
                self.render_layer_with_content(
                    ctx,
                    root,
                    (0.0, 0.0),
                    RenderLayer::Glass,
                    0,
                    (0.0, 0.0),
                );
            }

            // Pass 3: Foreground elements (including glass children)
            {
                let ctx = renderer.foreground();
                self.render_layer_with_content(
                    ctx,
                    root,
                    (0.0, 0.0),
                    RenderLayer::Foreground,
                    0,
                    (0.0, 0.0),
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
        glass_depth: u32,
        cumulative_scroll: (f32, f32),
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

        // Track glass nesting depth for children
        let children_glass_depth = if is_glass {
            glass_depth + 1
        } else {
            glass_depth
        };

        // Push clip BEFORE rendering content if this element clips its children
        // Clip to content area (inset by border width so children don't render over border)
        // This matches CSS overflow:hidden behavior which clips to the padding box
        let clips_content = render_node.props.clips_content;
        if clips_content {
            // Inset clip by border width to exclude border area from clipping region
            let border_width = render_node.props.border_width;
            let radius = render_node.props.border_radius;
            let clip_rect = Rect::new(
                border_width,
                border_width,
                (bounds.width - border_width * 2.0).max(0.0),
                (bounds.height - border_width * 2.0).max(0.0),
            );
            // Adjust corner radius for inset - reduce by border width
            let inset_radius = if radius.is_uniform() && radius.top_left > border_width {
                CornerRadius::uniform((radius.top_left - border_width).max(0.0))
            } else {
                CornerRadius::default()
            };
            // Set overflow fade before pushing clip
            if !render_node.props.overflow_fade.is_none() {
                ctx.set_overflow_fade(render_node.props.overflow_fade.to_array());
            }
            let clip_shape = if inset_radius.top_left > 0.0 {
                ClipShape::rounded_rect(clip_rect, inset_radius)
            } else {
                ClipShape::rect(clip_rect)
            };
            ctx.push_clip(clip_shape);
        }

        // Determine the effective layer for this node
        let effective_layer = if (glass_depth > 0 && !is_glass)
            || render_node.props.layer == RenderLayer::Foreground
        {
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
                            // GlassStyle still carries a single shadow.
                            shadow: render_node.props.shadow.first().copied(),
                            simple: glass.simple,
                            depth: glass_depth,
                            border_color: render_node.props.border_color,
                        });
                        ctx.fill_rect(rect, radius, glass_brush);
                    } else {
                        // Draw the shadow stack back-to-front (ambient first, key on top).
                        for shadow in render_node.props.shadow.iter().rev() {
                            ctx.draw_shadow(rect, radius, *shadow);
                        }

                        // Merge fill + border into a single SDF primitive when possible.
                        let sides = &render_node.props.border_sides;
                        let has_per_side = sides.has_any();
                        let has_uniform = !has_per_side
                            && render_node.props.border_width > 0.0
                            && render_node.props.border_color.is_some();

                        if has_per_side {
                            let uw = render_node.props.border_width;
                            let uc = render_node.props.border_color.unwrap_or(Color::TRANSPARENT);
                            let top = sides
                                .top
                                .as_ref()
                                .map(|b| (b.width, b.color))
                                .unwrap_or((uw, uc));
                            let right = sides
                                .right
                                .as_ref()
                                .map(|b| (b.width, b.color))
                                .unwrap_or((uw, uc));
                            let bottom = sides
                                .bottom
                                .as_ref()
                                .map(|b| (b.width, b.color))
                                .unwrap_or((uw, uc));
                            let left = sides
                                .left
                                .as_ref()
                                .map(|b| (b.width, b.color))
                                .unwrap_or((uw, uc));
                            let all_same =
                                top.1 == right.1 && right.1 == bottom.1 && bottom.1 == left.1;

                            if all_same {
                                let brush = render_node
                                    .props
                                    .background
                                    .clone()
                                    .unwrap_or(Brush::Solid(Color::TRANSPARENT));
                                ctx.fill_rect_with_per_side_border(
                                    rect,
                                    radius,
                                    brush,
                                    [top.0, right.0, bottom.0, left.0],
                                    top.1,
                                );
                            } else {
                                // Different colors — draw fill then 4x fill_rect
                                if let Some(ref bg) = render_node.props.background {
                                    ctx.fill_rect(rect, radius, bg.clone());
                                }
                                let has_radius = radius.top_left > 0.0
                                    || radius.top_right > 0.0
                                    || radius.bottom_left > 0.0
                                    || radius.bottom_right > 0.0;
                                if has_radius {
                                    ctx.push_clip(ClipShape::rounded_rect(rect, radius));
                                }
                                if let Some(ref b) = sides.left {
                                    if b.is_visible() {
                                        ctx.fill_rect(
                                            Rect::new(0.0, 0.0, b.width, rect.height()),
                                            CornerRadius::default(),
                                            Brush::Solid(b.color),
                                        );
                                    }
                                }
                                if let Some(ref b) = sides.right {
                                    if b.is_visible() {
                                        ctx.fill_rect(
                                            Rect::new(
                                                rect.width() - b.width,
                                                0.0,
                                                b.width,
                                                rect.height(),
                                            ),
                                            CornerRadius::default(),
                                            Brush::Solid(b.color),
                                        );
                                    }
                                }
                                if let Some(ref b) = sides.top {
                                    if b.is_visible() {
                                        ctx.fill_rect(
                                            Rect::new(0.0, 0.0, rect.width(), b.width),
                                            CornerRadius::default(),
                                            Brush::Solid(b.color),
                                        );
                                    }
                                }
                                if let Some(ref b) = sides.bottom {
                                    if b.is_visible() {
                                        ctx.fill_rect(
                                            Rect::new(
                                                0.0,
                                                rect.height() - b.width,
                                                rect.width(),
                                                b.width,
                                            ),
                                            CornerRadius::default(),
                                            Brush::Solid(b.color),
                                        );
                                    }
                                }
                                if has_radius {
                                    ctx.pop_clip();
                                }
                            }
                        } else if has_uniform {
                            let bw = render_node.props.border_width;
                            let bc = *render_node.props.border_color.as_ref().unwrap();
                            let brush = render_node
                                .props
                                .background
                                .clone()
                                .unwrap_or(Brush::Solid(Color::TRANSPARENT));
                            ctx.fill_rect_with_per_side_border(
                                rect,
                                radius,
                                brush,
                                [bw, bw, bw, bw],
                                bc,
                            );
                        } else if let Some(ref bg) = render_node.props.background {
                            ctx.fill_rect(rect, radius, bg.clone());
                        }
                    }

                    // Only glass needs foreground borders.
                    let border_in_foreground = is_glass;
                    if border_in_foreground {
                        ctx.set_foreground_layer(true);
                    }

                    // Draw outline
                    if render_node.props.outline_width > 0.0 {
                        if let Some(ref outline_color) = render_node.props.outline_color {
                            let ow = render_node.props.outline_width;
                            let offset = render_node.props.outline_offset;
                            let expand = offset + ow / 2.0;
                            let outline_rect = Rect::new(
                                -expand,
                                -expand,
                                bounds.width + expand * 2.0,
                                bounds.height + expand * 2.0,
                            );
                            let outline_radius = CornerRadius {
                                top_left: (radius.top_left + expand).max(0.0),
                                top_right: (radius.top_right + expand).max(0.0),
                                bottom_right: (radius.bottom_right + expand).max(0.0),
                                bottom_left: (radius.bottom_left + expand).max(0.0),
                            };
                            let stroke = Stroke::new(ow);
                            ctx.stroke_rect(
                                outline_rect,
                                outline_radius,
                                &stroke,
                                Brush::Solid(*outline_color),
                            );
                        }
                    }

                    // Restore foreground layer state after border/outline rendering
                    if border_in_foreground {
                        ctx.set_foreground_layer(false);
                    }
                }
                ElementType::Canvas(canvas_data) => {
                    // Canvas element: invoke the render callback with DrawContext
                    // Push clip to ensure canvas content respects parent bounds (e.g., scroll containers)
                    if let Some(render_fn) = &canvas_data.render_fn {
                        // Push clip for canvas bounds - this ensures content doesn't render outside
                        let clip_rect = Rect::new(0.0, 0.0, bounds.width, bounds.height);
                        ctx.push_clip(ClipShape::rect(clip_rect));

                        let canvas_bounds = crate::canvas::CanvasBounds {
                            x: 0.0,
                            y: 0.0,
                            width: bounds.width,
                            height: bounds.height,
                        };
                        render_fn(ctx, canvas_bounds);

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

        // Update cumulative scroll for children
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

        // Traverse children
        for child_id in self.layout_tree.children(node) {
            let child_render = self.render_nodes.get(&child_id);
            let child_is_fixed = child_render.map(|n| n.props.is_fixed).unwrap_or(false);
            let child_is_sticky = child_render.map(|n| n.props.is_sticky).unwrap_or(false);

            // Fixed: push counter-scroll to cancel ALL accumulated scroll
            let has_fixed_counter = child_is_fixed
                && (new_cumulative_scroll.0.abs() > 0.001 || new_cumulative_scroll.1.abs() > 0.001);
            if has_fixed_counter {
                ctx.push_transform(Transform::translate(
                    -new_cumulative_scroll.0,
                    -new_cumulative_scroll.1,
                ));
            }

            // Sticky: compute corrective offset when element would scroll past threshold
            let mut has_sticky_correction = false;
            if child_is_sticky {
                if let Some(threshold) = child_render.and_then(|n| n.props.sticky_top) {
                    if let Some(cb) = self.layout_tree.get_bounds(child_id, (0.0, 0.0)) {
                        let visual_y = cb.y + new_cumulative_scroll.1;
                        if visual_y < threshold {
                            let correction = threshold - visual_y;
                            ctx.push_transform(Transform::translate(0.0, correction));
                            has_sticky_correction = true;
                        }
                    }
                }
            }

            let child_cumulative = if child_is_fixed {
                (0.0, 0.0)
            } else {
                new_cumulative_scroll
            };

            self.render_layer_with_content(
                ctx,
                child_id,
                (0.0, 0.0),
                target_layer,
                children_glass_depth,
                child_cumulative,
            );

            // Pop sticky correction
            if has_sticky_correction {
                ctx.pop_transform();
            }
            // Pop fixed counter-scroll
            if has_fixed_counter {
                ctx.pop_transform();
            }
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
            self.render_text_recursive(renderer, root, (0.0, 0.0), 0, false, (0.0, 0.0));
        }
    }

    /// Recursively render text elements
    fn render_text_recursive<R: LayoutRenderer>(
        &self,
        renderer: &mut R,
        node: LayoutNodeId,
        parent_offset: (f32, f32),
        glass_depth: u32,
        inside_foreground: bool,
        cumulative_scroll: (f32, f32),
    ) {
        // Use get_render_bounds to get animated bounds if layout animation is active
        // This ensures text respects layout animations (FLIP-style bounds animation)
        let Some(bounds) = self.get_render_bounds(node, (0.0, 0.0)) else {
            return;
        };

        let Some(render_node) = self.render_nodes.get(&node) else {
            return;
        };

        // CSS visibility: hidden — skip text rendering but preserve layout space
        if !render_node.props.visible {
            return;
        }

        let is_glass = matches!(render_node.props.material, Some(Material::Glass(_)));
        let children_glass_depth = if is_glass {
            glass_depth + 1
        } else {
            glass_depth
        };

        // Track foreground inheritance
        let is_foreground = render_node.props.layer == RenderLayer::Foreground;
        let children_inside_foreground = inside_foreground || is_foreground;

        // Text inside glass or foreground goes to foreground layer
        let to_foreground = children_glass_depth > 0 || children_inside_foreground;

        if let ElementType::Text(text_data) = &render_node.element_type {
            // Absolute position for text
            let abs_x = parent_offset.0 + bounds.x;
            let abs_y = parent_offset.1 + bounds.y;

            // Use animated/overridden text color, font size and weight if available
            let color = render_node.props.text_color.unwrap_or(text_data.color);
            let font_size = render_node.props.font_size.unwrap_or(text_data.font_size);
            let weight = render_node.props.font_weight.unwrap_or(text_data.weight);

            // Render normal text
            if to_foreground {
                renderer.render_text_foreground(
                    &text_data.content,
                    abs_x,
                    abs_y,
                    bounds.width,
                    bounds.height,
                    font_size,
                    color,
                    text_data.align,
                    weight,
                );
            } else {
                renderer.render_text_background(
                    &text_data.content,
                    abs_x,
                    abs_y,
                    bounds.width,
                    bounds.height,
                    font_size,
                    color,
                    text_data.align,
                    weight,
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
                // Cancel all accumulated scroll from the offset
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

            self.render_text_recursive(
                renderer,
                child_id,
                child_offset,
                children_glass_depth,
                children_inside_foreground,
                child_cumulative,
            );
        }
    }

    /// Render all SVG elements via the LayoutRenderer
    fn render_svg_elements<R: LayoutRenderer>(&self, renderer: &mut R) {
        if let Some(root) = self.root {
            self.render_svg_recursive(renderer, root, (0.0, 0.0), 0, false, (0.0, 0.0));
        }
    }

    /// Recursively render SVG elements
    fn render_svg_recursive<R: LayoutRenderer>(
        &self,
        renderer: &mut R,
        node: LayoutNodeId,
        parent_offset: (f32, f32),
        glass_depth: u32,
        inside_foreground: bool,
        cumulative_scroll: (f32, f32),
    ) {
        // Use get_render_bounds to get animated bounds if layout animation is active
        // This ensures SVG respects layout animations (FLIP-style bounds animation)
        let Some(bounds) = self.get_render_bounds(node, (0.0, 0.0)) else {
            return;
        };

        let Some(render_node) = self.render_nodes.get(&node) else {
            return;
        };

        let is_glass = matches!(render_node.props.material, Some(Material::Glass(_)));
        let children_glass_depth = if is_glass {
            glass_depth + 1
        } else {
            glass_depth
        };

        // Track foreground inheritance
        let is_foreground = render_node.props.layer == RenderLayer::Foreground;
        let children_inside_foreground = inside_foreground || is_foreground;

        // SVG inside glass or foreground goes to foreground layer
        let to_foreground = children_glass_depth > 0 || children_inside_foreground;

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

            self.render_svg_recursive(
                renderer,
                child_id,
                child_offset,
                children_glass_depth,
                children_inside_foreground,
                child_cumulative,
            );
        }
    }
}
