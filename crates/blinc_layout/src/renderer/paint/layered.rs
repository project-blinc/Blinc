//! Layer-aware painters: route per-element render layer
//! (`Background` / `Foreground`) to its own `DrawContext`, while
//! tracking glass children + push/pop a glass-layer effect onto
//! foreground when needed.
//!
//! - `render_layered` — public entry. Takes three contexts
//!   (background, glass, foreground) and walks the tree once per
//!   pass, gating per-node visit on whether the current pass should
//!   paint that material.
//! - `render_to_layer` — variant that skips the glass split and
//!   renders only the elements assigned to a single requested layer
//!   into one context. Mostly debug surface.
//! - `render_layer` — the recursive walker. Mirrors the structure of
//!   `render_node` (basic walker) but adds layer-gating and the glass
//!   push/pop dance. Called by `render_layered_simple` (basic.rs)
//!   and `render_layered` here.

use blinc_core::{
    BlendMode, Brush, ClipShape, Color, CornerRadius, DrawContext, LayerConfig, LayerEffect, Point,
    Rect, Size, Stroke, Transform,
};

use crate::canvas::CanvasBounds;
use crate::element::RenderLayer;
use crate::tree::LayoutNodeId;

use super::super::{ElementType, RenderTree};

impl RenderTree {
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
                0,
                false,
                (0.0, 0.0),
            );

            // Pass 2: Glass - render as Brush::Glass
            self.render_layer(
                glass_ctx,
                root,
                (0.0, 0.0),
                RenderLayer::Glass,
                0,
                false,
                (0.0, 0.0),
            );

            // Pass 3: Foreground (includes children of glass elements)
            self.render_layer(
                foreground_ctx,
                root,
                (0.0, 0.0),
                RenderLayer::Foreground,
                0,
                false,
                (0.0, 0.0),
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
        let _tree_scope = self.layout_tree.enter_scope();
        if let Some(root) = self.root {
            // Apply DPI scale factor if set (for HiDPI display support)
            let has_scale = self.scale_factor != 1.0;
            if has_scale {
                ctx.push_transform(Transform::scale(self.scale_factor, self.scale_factor));
            }

            self.render_layer(ctx, root, (0.0, 0.0), target_layer, 0, false, (0.0, 0.0));

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
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn render_layer(
        &self,
        ctx: &mut dyn DrawContext,
        node: LayoutNodeId,
        parent_offset: (f32, f32),
        target_layer: RenderLayer,
        glass_depth: u32,
        inside_foreground: bool,
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
        let is_glass = render_node.props.is_glass();
        let children_glass_depth = if is_glass {
            glass_depth + 1
        } else {
            glass_depth
        };

        // Track if children should be considered inside foreground
        // Once inside foreground, stay inside foreground for all descendants
        let is_foreground = render_node.props.layer == RenderLayer::Foreground;
        let children_inside_foreground = inside_foreground || is_foreground;

        // Compute the effective layer for the layer-effect push gate
        // below — children inside glass / foreground render through the
        // foreground layer regardless of this node's authored setting.
        // (Same precedence the per-node render gate uses further down.)
        let effective_layer_for_push = if (glass_depth > 0 && !is_glass) || inside_foreground {
            RenderLayer::Foreground
        } else if is_glass {
            RenderLayer::Glass
        } else {
            render_node.props.layer
        };

        // Push a Blinc layer for any node that authored
        // `layer_effects` — `Div::blur` / `Div::layer_effect` and
        // anything reading `props.layer_effects`. Without this push
        // the effect entries on this node ride into the batch as
        // dead data and `apply_layer_effects` never runs (no
        // `LayerCommand::Push { effects: !empty }` is queued).
        // Symmetric with `render_layer_with_motion`'s richer push,
        // minus the motion-opacity / blend-mode / 3D plumbing this
        // simpler path doesn't track. Effect radii are scaled by
        // the DPI factor so CSS px line up with physical px in the
        // GPU effect kernels.
        let has_layer_effects_node = !render_node.props.layer_effects.is_empty();
        let should_push_layer = has_layer_effects_node && effective_layer_for_push == target_layer;
        if should_push_layer {
            let scaled_effects: Vec<blinc_core::LayerEffect> = render_node
                .props
                .layer_effects
                .iter()
                .map(|e| match e {
                    blinc_core::LayerEffect::Blur { radius, quality } => {
                        blinc_core::LayerEffect::Blur {
                            radius: radius * self.scale_factor,
                            quality: *quality,
                        }
                    }
                    blinc_core::LayerEffect::DropShadow {
                        offset_x,
                        offset_y,
                        blur,
                        spread,
                        color,
                    } => blinc_core::LayerEffect::DropShadow {
                        offset_x: offset_x * self.scale_factor,
                        offset_y: offset_y * self.scale_factor,
                        blur: blur * self.scale_factor,
                        spread: spread * self.scale_factor,
                        color: *color,
                    },
                    other => other.clone(),
                })
                .collect();
            ctx.push_layer(blinc_core::LayerConfig {
                id: None,
                position: Some(blinc_core::Point::new(bounds.x, bounds.y)),
                size: Some(blinc_core::Size::new(bounds.width, bounds.height)),
                blend_mode: blinc_core::BlendMode::Normal,
                opacity: 1.0,
                depth: false,
                effects: scaled_effects,
                transform_3d: None,
            });
        }

        // Resolve corner shape FIRST so the overflow:clip below can
        // follow the same curve the parent's fill will use. Tolerate
        // an uninitialised ThemeState (snapshot / GPU integration
        // tests render through this path without calling
        // `ThemeState::init_*` first). See basic.rs for the same
        // fall-back rationale.
        let (theme_shape_n, radius_full_n) = match blinc_theme::ThemeState::try_get() {
            Some(theme) => (theme.shape(), theme.radii().radius_full),
            None => (blinc_theme::ShapeTokens::default(), 9999.0),
        };
        let resolved_corner_shape_n = super::helpers::resolve_corner_shape(
            render_node.props.corner_shape,
            render_node.props.border_radius,
            (bounds.width, bounds.height),
            &theme_shape_n,
            radius_full_n,
            render_node.props.corner_shape_locked,
        );

        // Push clip BEFORE rendering content if this element clips its children
        // Clip to content area (inset by border width so children don't render over border)
        // This matches CSS overflow:hidden behavior which clips to the padding box
        let clips_content = render_node.props.clips_content;
        if clips_content {
            // Calculate border insets from either uniform border or per-side borders
            let sides = &render_node.props.border_sides;
            let uniform_border = render_node.props.border_width;
            let radius = render_node.props.border_radius;

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
                ClipShape::rounded_rect_shaped(clip_rect, inset_radius, resolved_corner_shape_n)
            } else {
                ClipShape::rect(clip_rect)
            };
            ctx.push_clip(clip_shape);
        }

        // Determine the effective layer for this node:
        // - If we're inside a glass element, children render as foreground
        // - If we're inside a foreground element, children also render as foreground
        // - Otherwise, use the node's explicit layer setting
        let effective_layer = if (glass_depth > 0 && !is_glass) || inside_foreground {
            RenderLayer::Foreground
        } else if is_glass {
            RenderLayer::Glass
        } else {
            render_node.props.layer
        };
        let has_corner_shape_n = !resolved_corner_shape_n.is_round();
        if has_corner_shape_n {
            ctx.set_corner_shape(resolved_corner_shape_n.to_array());
        }

        // Only render if this node matches the target layer
        if effective_layer == target_layer {
            let rect = Rect::new(0.0, 0.0, bounds.width, bounds.height);
            let radius = render_node.props.border_radius;

            if let Some(glass) = render_node.props.glass_style(glass_depth) {
                ctx.fill_rect(rect, radius, Brush::Glass(glass));
            } else {
                // Draw the shadow stack back-to-front (ambient first, key on top).
                for shadow in render_node.props.shadow.iter().rev() {
                    ctx.draw_shadow(rect, radius, *shadow);
                }

                // Pre-resolve border info for merging with fill
                let has_per_side_n = render_node.props.border_sides.has_any();
                let has_uniform_n = !has_per_side_n
                    && render_node.props.border_width > 0.0
                    && render_node.props.border_color.is_some();

                // Merge fill + border into single SDF primitive to avoid AA fringe at corners
                if let Some(ref bg) = render_node.props.background {
                    if has_per_side_n {
                        let sides = &render_node.props.border_sides;
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
                            let widths = [top.0, right.0, bottom.0, left.0];
                            ctx.fill_rect_with_per_side_border(
                                rect,
                                radius,
                                bg.clone(),
                                widths,
                                top.1,
                            );
                        } else {
                            // Different colors — draw fill separately, borders as foreground later
                            ctx.fill_rect(rect, radius, bg.clone());
                        }
                    } else if has_uniform_n {
                        let bw = render_node.props.border_width;
                        let bc = *render_node.props.border_color.as_ref().unwrap();
                        ctx.fill_rect_with_per_side_border(
                            rect,
                            radius,
                            bg.clone(),
                            [bw, bw, bw, bw],
                            bc,
                        );
                    } else {
                        ctx.fill_rect(rect, radius, bg.clone());
                    }
                } else if has_per_side_n {
                    // No background but has border — transparent fill with border
                    let sides = &render_node.props.border_sides;
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
                        let widths = [top.0, right.0, bottom.0, left.0];
                        ctx.fill_rect_with_per_side_border(
                            rect,
                            radius,
                            Brush::Solid(Color::TRANSPARENT),
                            widths,
                            top.1,
                        );
                    }
                    // Different colors with no bg: handled below in foreground section
                } else if has_uniform_n {
                    let bw = render_node.props.border_width;
                    let bc = *render_node.props.border_color.as_ref().unwrap();
                    ctx.fill_rect_with_per_side_border(
                        rect,
                        radius,
                        Brush::Solid(Color::TRANSPARENT),
                        [bw, bw, bw, bw],
                        bc,
                    );
                }
            }

            // Only glass needs foreground borders (special compositing).
            // clips_content elements have children clipped to padding box (inset clip),
            // so the merged border is never covered.
            let border_in_foreground = is_glass;
            if border_in_foreground {
                ctx.set_foreground_layer(true);
            }

            // Draw borders that weren't merged with the fill above.
            // For non-glass: only different-color per-side borders need separate rendering.
            // For glass: all borders render as foreground.
            let has_per_side = render_node.props.border_sides.has_any();
            let has_uniform = !has_per_side
                && render_node.props.border_width > 0.0
                && render_node.props.border_color.is_some();
            let has_border = has_per_side || has_uniform;

            // Non-glass different-color per-side: need separate rendering
            let needs_separate_per_side = has_per_side && !border_in_foreground && {
                let sides = &render_node.props.border_sides;
                let uw = render_node.props.border_width;
                let uc = render_node.props.border_color.unwrap_or(Color::TRANSPARENT);
                let top = sides.top.as_ref().map(|b| b.color).unwrap_or(uc);
                let right = sides.right.as_ref().map(|b| b.color).unwrap_or(uc);
                let bottom = sides.bottom.as_ref().map(|b| b.color).unwrap_or(uc);
                let left = sides.left.as_ref().map(|b| b.color).unwrap_or(uc);
                !(top == right && right == bottom && bottom == left)
            };

            if needs_separate_per_side {
                // Different-color per-side borders — 4x fill_rect with clip
                let sides = &render_node.props.border_sides;
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
                let has_radius = radius.top_left > 0.0
                    || radius.top_right > 0.0
                    || radius.bottom_left > 0.0
                    || radius.bottom_right > 0.0;
                if has_radius {
                    ctx.push_clip(ClipShape::rounded_rect(rect, radius));
                }
                if left.0 > 0.0 {
                    ctx.fill_rect(
                        Rect::new(0.0, 0.0, left.0, rect.height()),
                        CornerRadius::default(),
                        Brush::Solid(left.1),
                    );
                }
                if right.0 > 0.0 {
                    ctx.fill_rect(
                        Rect::new(rect.width() - right.0, 0.0, right.0, rect.height()),
                        CornerRadius::default(),
                        Brush::Solid(right.1),
                    );
                }
                if top.0 > 0.0 {
                    ctx.fill_rect(
                        Rect::new(0.0, 0.0, rect.width(), top.0),
                        CornerRadius::default(),
                        Brush::Solid(top.1),
                    );
                }
                if bottom.0 > 0.0 {
                    ctx.fill_rect(
                        Rect::new(0.0, rect.height() - bottom.0, rect.width(), bottom.0),
                        CornerRadius::default(),
                        Brush::Solid(bottom.1),
                    );
                }
                if has_radius {
                    ctx.pop_clip();
                }
            } else if has_border && border_in_foreground {
                // Glass foreground border — drawn separately on top of glass compositing
                if has_per_side {
                    let sides = &render_node.props.border_sides;
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
                        let widths = [top.0, right.0, bottom.0, left.0];
                        ctx.fill_rect_with_per_side_border(
                            rect,
                            radius,
                            Brush::Solid(Color::TRANSPARENT),
                            widths,
                            top.1,
                        );
                    } else {
                        let has_radius = radius.top_left > 0.0
                            || radius.top_right > 0.0
                            || radius.bottom_left > 0.0
                            || radius.bottom_right > 0.0;
                        if has_radius {
                            ctx.push_clip(ClipShape::rounded_rect(rect, radius));
                        }
                        if left.0 > 0.0 {
                            ctx.fill_rect(
                                Rect::new(0.0, 0.0, left.0, rect.height()),
                                CornerRadius::default(),
                                Brush::Solid(left.1),
                            );
                        }
                        if right.0 > 0.0 {
                            ctx.fill_rect(
                                Rect::new(rect.width() - right.0, 0.0, right.0, rect.height()),
                                CornerRadius::default(),
                                Brush::Solid(right.1),
                            );
                        }
                        if top.0 > 0.0 {
                            ctx.fill_rect(
                                Rect::new(0.0, 0.0, rect.width(), top.0),
                                CornerRadius::default(),
                                Brush::Solid(top.1),
                            );
                        }
                        if bottom.0 > 0.0 {
                            ctx.fill_rect(
                                Rect::new(0.0, rect.height() - bottom.0, rect.width(), bottom.0),
                                CornerRadius::default(),
                                Brush::Solid(bottom.1),
                            );
                        }
                        if has_radius {
                            ctx.pop_clip();
                        }
                    }
                } else if has_uniform {
                    let bw = render_node.props.border_width;
                    let bc = *render_node.props.border_color.as_ref().unwrap();
                    ctx.fill_rect_with_per_side_border(
                        rect,
                        radius,
                        Brush::Solid(Color::TRANSPARENT),
                        [bw, bw, bw, bw],
                        bc,
                    );
                }
            }

            // Draw outline outside the border
            if render_node.props.outline_width > 0.0
                && let Some(ref outline_color) = render_node.props.outline_color
            {
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

            // Restore foreground layer state after border/outline rendering
            if border_in_foreground {
                ctx.set_foreground_layer(false);
            }

            // Handle canvas element rendering
            // Push clip to ensure canvas content respects parent bounds (e.g., scroll containers)
            if let ElementType::Canvas(canvas_data) = &render_node.element_type
                && let Some(render_fn) = &canvas_data.render_fn
            {
                // Push clip for canvas bounds - this ensures content doesn't render outside
                let clip_rect = Rect::new(0.0, 0.0, bounds.width, bounds.height);
                ctx.push_clip(ClipShape::rect(clip_rect));

                // `bounds.x` / `bounds.y` are already translated
                // onto the DrawContext by the `push_transform` at
                // the top of `render_node`, so in canvas-local
                // space the origin is (0, 0). Surfacing the
                // pre-translate offset to the callback is a
                // diagnostic breadcrumb, not a correction; forward
                // zero for x/y so `Rect::new(bounds.x, bounds.y,
                // …)` in callback code resolves to the canvas's
                // actual origin without double-offsetting.
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

        // Check if this node has a scroll offset and apply it to children
        let scroll_offset = self.get_scroll_offset(node);
        let has_scroll = scroll_offset.0.abs() > 0.001 || scroll_offset.1.abs() > 0.001;

        if has_scroll {
            // Apply scroll offset as a transform
            // Positive offset_y = scrolled down = content moves up = negative translation
            ctx.push_transform(Transform::translate(scroll_offset.0, scroll_offset.1));
        }

        // Clear corner shape before rendering children — not inherited
        if has_corner_shape_n {
            ctx.clear_corner_shape();
        }

        // Traverse children (they inherit our transform and layer inheritance)
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
            if child_is_sticky
                && let Some(threshold) = child_render.and_then(|n| n.props.sticky_top)
                && let Some(cb) = self.get_render_bounds(child_id, (0.0, 0.0))
            {
                let visual_y = cb.y + new_cumulative.1;
                if visual_y < threshold {
                    let correction = threshold - visual_y;
                    ctx.push_transform(Transform::translate(0.0, correction));
                    has_sticky_correction = true;
                }
            }

            let child_cum = if child_is_fixed {
                (0.0, 0.0)
            } else {
                new_cumulative
            };
            self.render_layer(
                ctx,
                child_id,
                (0.0, 0.0),
                target_layer,
                children_glass_depth,
                children_inside_foreground,
                child_cum,
            );
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

        // Pop clip if we pushed one
        if clips_content {
            ctx.pop_clip();
        }

        // Pop the layer-effects layer (must be after the clip pop so
        // primitives clipped by `clips_content` still land inside the
        // layer's offscreen, but before the element-transform pop so
        // the GPU effect bounds calc reads the right transform stack).
        if should_push_layer {
            ctx.pop_layer();
        }

        // Pop element-specific transforms if we pushed them (3 transforms for centering)
        if has_element_transform {
            ctx.pop_transform(); // pop translate(-center_x, -center_y)
            ctx.pop_transform(); // pop the actual transform
            ctx.pop_transform(); // pop translate(center_x, center_y)
        }

        ctx.pop_transform();
    }
}
