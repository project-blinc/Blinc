//! Basic painters: the simple `render(ctx)` walker that doesn't
//! apply motion-pre-replay or per-layer separation, plus
//! `render_layered_simple` which drives `render_layer` for the
//! background-then-foreground passes onto a single context.
//!
//! Three `pub(crate)` / `pub` methods:
//!
//! - `render` — top-level public entry. Pulls the root and dispatches
//!   to `render_node`.
//! - `render_node` — recursive walker that paints a single node and
//!   its children. Resolves scroll offsets, motion bindings, and
//!   per-layer transforms; emits primitives via `DrawContext`.
//!   Used by `render` directly (single-context path) and by
//!   `render_with_motion` after the motion-pre-replay pass settles.
//! - `render_layered_simple` — sequenced background → glass →
//!   foreground passes on the same `DrawContext`. Glass material is
//!   rendered as `Brush::Glass`; the GPU renderer collects those into
//!   the glass-primitive batch.

use blinc_core::{
    Brush, ClipShape, Color, CornerRadius, DrawContext, GlassStyle, Rect, Stroke, Transform,
};

use crate::element::{Material, RenderLayer};
use crate::tree::LayoutNodeId;

use super::super::RenderTree;

impl RenderTree {
    /// Render the entire tree to a DrawContext
    pub fn render(&self, ctx: &mut dyn DrawContext) {
        tracing::trace!(
            "render: motion_bindings count = {}",
            self.motion_bindings.len()
        );
        if let Some(root) = self.root {
            self.render_node(ctx, root, (0.0, 0.0), (0.0, 0.0));
        }
    }

    /// Render a single node and its children
    fn render_node(
        &self,
        ctx: &mut dyn DrawContext,
        node: LayoutNodeId,
        parent_offset: (f32, f32),
        cumulative_scroll: (f32, f32),
    ) {
        let Some(bounds) = self.layout_tree.get_bounds(node, parent_offset) else {
            return;
        };

        let Some(render_node) = self.render_nodes.get(&node) else {
            return;
        };

        // CSS visibility: hidden — skip rendering but preserve layout space
        if !render_node.props.visible {
            return;
        }

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
        let is_glass = matches!(render_node.props.material, Some(Material::Glass(_)));

        // Corner shape setup — must run before draw_shadow so shadows
        // match the fill shape. We resolve through the theme's
        // ShapeTokens so Universal HID variants auto-substitute their
        // squircle `n` on rounded corners that pass the threshold
        // check; explicit per-element overrides (`.squircle()`, CSS
        // `corner-shape`) keep precedence. Themes that don't opt in
        // return the trait's default off-state and the element stays
        // circular.
        // Tolerate an uninitialised ThemeState (snapshot / GPU
        // integration tests render through this path without calling
        // `ThemeState::init_*` first). Default ShapeTokens is the
        // "off" state — no squircle substitution — and 9999.0 is the
        // sentinel `radius_full` value used by themes whose ladder
        // tops out at "fully circular".
        let (theme_shape, radius_full) = match blinc_theme::ThemeState::try_get() {
            Some(theme) => (theme.shape(), theme.radii().radius_full),
            None => (blinc_theme::ShapeTokens::default(), 9999.0),
        };
        let resolved_corner_shape = super::helpers::resolve_corner_shape(
            render_node.props.corner_shape,
            radius,
            (bounds.width, bounds.height),
            &theme_shape,
            radius_full,
            render_node.props.corner_shape_locked,
        );
        let has_corner_shape_l = !resolved_corner_shape.is_round();
        if has_corner_shape_l {
            ctx.set_corner_shape(resolved_corner_shape.to_array());
        }

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
                // GlassStyle currently carries a single shadow; pass the
                // topmost layer of the stack.
                shadow: render_node.props.shadow.first().copied(),
                simple: glass.simple,
                depth: 0,
                border_color: render_node.props.border_color,
            });
            ctx.fill_rect(rect, radius, glass_brush);
        } else {
            // For non-glass elements, draw shadow stack first (renders
            // behind the element). Iterate back-to-front so the outermost
            // ambient layer paints first and the tight key-light layer
            // lands on top.
            for shadow in render_node.props.shadow.iter().rev() {
                ctx.draw_shadow(rect, radius, *shadow);
            }

            // Merge fill + border into a single SDF primitive when possible.
            // This avoids AA fringe from overlapping fill + border at corners.
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
                let all_same = top.1 == right.1 && right.1 == bottom.1 && bottom.1 == left.1;

                if all_same {
                    // Same color — single merged primitive
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
                                Rect::new(rect.width() - b.width, 0.0, b.width, rect.height()),
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
                                Rect::new(0.0, rect.height() - b.width, rect.width(), b.width),
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
                // Uniform border — merge with fill
                let bw = render_node.props.border_width;
                let bc = *render_node.props.border_color.as_ref().unwrap();
                let brush = render_node
                    .props
                    .background
                    .clone()
                    .unwrap_or(Brush::Solid(Color::TRANSPARENT));
                ctx.fill_rect_with_per_side_border(rect, radius, brush, [bw, bw, bw, bw], bc);
            } else {
                // No border — just fill
                if let Some(ref bg) = render_node.props.background {
                    ctx.fill_rect(rect, radius, bg.clone());
                }
            }

            // Inset shadows, after the background. CSS paints these over
            // the background and under the border; clipping to the PADDING
            // box gets the same picture without splitting the merged
            // fill+border primitive, which is what keeps corners free of
            // AA fringe. The shadow's geometry still comes from the border
            // box, so its falloff starts where CSS says it does — the clip
            // only stops it reaching the border band.
            if !render_node.props.inner_shadow.is_empty() {
                let bw = render_node.props.border_width.max(0.0);
                let pad = Rect {
                    origin: blinc_core::Point::new(rect.origin.x + bw, rect.origin.y + bw),
                    size: blinc_core::Size::new(
                        (rect.size.width - bw * 2.0).max(0.0),
                        (rect.size.height - bw * 2.0).max(0.0),
                    ),
                };
                let inner_radius = CornerRadius {
                    top_left: (radius.top_left - bw).max(0.0),
                    top_right: (radius.top_right - bw).max(0.0),
                    bottom_right: (radius.bottom_right - bw).max(0.0),
                    bottom_left: (radius.bottom_left - bw).max(0.0),
                };
                if pad.size.width > 0.0 && pad.size.height > 0.0 {
                    ctx.push_clip(ClipShape::rounded_rect(pad, inner_radius));
                    for shadow in render_node.props.inner_shadow.iter().rev() {
                        ctx.draw_inner_shadow(rect, radius, *shadow);
                    }
                    ctx.pop_clip();
                }
            }
        }

        // Only glass needs foreground borders (special compositing).
        let border_in_foreground = is_glass;
        if border_in_foreground {
            ctx.set_foreground_layer(true);
        }

        // Draw outline outside the border (CSS outlines don't affect layout)
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
                // Expand corner radius to follow the outline curve
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

        // Push clip if this element clips its children (e.g., scroll containers)
        // Clip to content area (inset by border width so children don't render over border)
        // This matches CSS overflow:hidden behavior which clips to the padding box
        let clips_content = render_node.props.clips_content;
        if clips_content {
            // Calculate border insets from either uniform border or per-side borders
            let sides = &render_node.props.border_sides;
            let uniform_border = render_node.props.border_width;

            let left_inset = sides
                .left
                .as_ref()
                .map(|b| b.width)
                .unwrap_or(uniform_border);
            let right_inset = sides
                .right
                .as_ref()
                .map(|b| b.width)
                .unwrap_or(uniform_border);
            let top_inset = sides
                .top
                .as_ref()
                .map(|b| b.width)
                .unwrap_or(uniform_border);
            let bottom_inset = sides
                .bottom
                .as_ref()
                .map(|b| b.width)
                .unwrap_or(uniform_border);

            // Inset clip by border width to exclude border area from clipping region
            let clip_rect = Rect::new(
                left_inset,
                top_inset,
                (bounds.width - left_inset - right_inset).max(0.0),
                (bounds.height - top_inset - bottom_inset).max(0.0),
            );
            // Adjust corner radius for inset - use max border width for corner adjustment
            let max_border = left_inset.max(right_inset).max(top_inset).max(bottom_inset);
            let inset_radius = if radius.is_uniform() && radius.top_left > max_border {
                CornerRadius::uniform((radius.top_left - max_border).max(0.0))
            } else {
                CornerRadius::default()
            };
            // Set overflow fade before pushing clip
            if !render_node.props.overflow_fade.is_none() {
                ctx.set_overflow_fade(render_node.props.overflow_fade.to_array());
            }
            let clip_shape = if inset_radius.top_left > 0.0 {
                // Pass the parent's resolved corner_shape so
                // overflow:clip on a squircle / scoop / bevel parent
                // follows the same curve as the parent's fill —
                // otherwise the diagonal-corner pixels between the
                // parent's outline and a circular clip leak the
                // parent's bg through where children would paint.
                ClipShape::rounded_rect_shaped(clip_rect, inset_radius, resolved_corner_shape)
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

        // Clear corner shape before rendering children — not inherited
        if has_corner_shape_l {
            ctx.clear_corner_shape();
        }

        // Render children (relative to this node's transform + scroll offset)
        // Reset cumulative scroll when entering a scroll container.
        let is_scroll_container = self.scroll_physics.contains_key(&node);
        let new_cumulative = if is_scroll_container {
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

            let has_counter = child_is_fixed
                && (new_cumulative.0.abs() > 0.001 || new_cumulative.1.abs() > 0.001);
            if has_counter {
                ctx.push_transform(Transform::translate(-new_cumulative.0, -new_cumulative.1));
            }

            // Sticky: compute corrective offset when element would scroll past threshold
            let mut has_sticky_correction = false;
            if child_is_sticky {
                if let Some(threshold) = child_render.and_then(|n| n.props.sticky_top) {
                    if let Some(cb) = self.get_render_bounds(child_id, (0.0, 0.0)) {
                        let visual_y = cb.y + new_cumulative.1;
                        if visual_y < threshold {
                            let correction = threshold - visual_y;
                            ctx.push_transform(Transform::translate(0.0, correction));
                            has_sticky_correction = true;
                        }
                    }
                }
            }

            let child_cum = if child_is_fixed {
                (0.0, 0.0)
            } else {
                new_cumulative
            };
            self.render_node(ctx, child_id, (0.0, 0.0), child_cum);
            if has_sticky_correction {
                ctx.pop_transform();
            }
            if has_counter {
                ctx.pop_transform();
            }
        }

        // Pop scroll transform if we pushed one
        if has_scroll {
            ctx.pop_transform();
        }

        // Render scrollbar overlay if this is a scroll container with visible scrollbar
        if let Some(physics) = self.scroll_physics.get(&node) {
            if let Ok(p) = physics.try_lock() {
                let info = p.scrollbar_render_info();
                tracing::trace!(
                    "Scrollbar: opacity={:.2}, show_v={}, show_h={}, state={:?}, content_h={:.0}, viewport_h={:.0}",
                    info.opacity,
                    info.show_vertical,
                    info.show_horizontal,
                    info.state,
                    p.content_height,
                    p.viewport_height
                );
                // Only render if scrollbar is visible (opacity > 0)
                if info.opacity > 0.01 {
                    tracing::trace!("Rendering scrollbar with opacity {:.2}", info.opacity);
                    self.render_scrollbar(ctx, bounds.width, bounds.height, &info);
                }
            }
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
            self.render_layer(
                ctx,
                root,
                (0.0, 0.0),
                RenderLayer::Background,
                0,
                false,
                (0.0, 0.0),
            );

            // Pass 2: Glass - these render as Brush::Glass which becomes glass primitives
            self.render_layer(
                ctx,
                root,
                (0.0, 0.0),
                RenderLayer::Glass,
                0,
                false,
                (0.0, 0.0),
            );

            // Pass 3: Foreground (includes children of glass elements, rendered after glass)
            ctx.set_foreground_layer(true);
            self.render_layer(
                ctx,
                root,
                (0.0, 0.0),
                RenderLayer::Foreground,
                0,
                false,
                (0.0, 0.0),
            );
            ctx.set_foreground_layer(false);
        }
    }
}
