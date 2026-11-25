//! Render tessellated and atlas-backed SVG elements.
use super::{
    Brush, Color, DefaultHasher, DrawContext, GpuImageInstance, GpuPaintContext, Hash, Hasher,
    RasterizedSvg, Rect, RenderContext, Stroke, SvgDocument, SvgElement,
};

mod raster;

impl RenderContext {
    /// Draw a tessellated SVG with its inherited clip, tint and opacity.
    pub(super) fn render_svg_element(&mut self, ctx: &mut GpuPaintContext, svg: &SvgElement) {
        // Skip completely transparent SVGs
        if svg.motion_opacity <= 0.001 {
            return;
        }

        // Skip SVGs completely outside their clip bounds
        if let Some([clip_x, clip_y, clip_w, clip_h]) = svg.clip_bounds {
            let svg_right = svg.x + svg.width;
            let svg_bottom = svg.y + svg.height;
            let clip_right = clip_x + clip_w;
            let clip_bottom = clip_y + clip_h;

            // Check if SVG is completely outside clip bounds
            if svg.x >= clip_right
                || svg_right <= clip_x
                || svg.y >= clip_bottom
                || svg_bottom <= clip_y
            {
                return;
            }
        }

        // Hash the SVG source for cache lookup (faster than using string as key)
        let svg_hash = {
            let mut hasher = DefaultHasher::new();
            svg.source.hash(&mut hasher);
            hasher.finish()
        };

        // Try cache lookup first, parse only on miss
        let doc = if let Some(cached) = self.svg_cache.get(&svg_hash) {
            cached.clone()
        } else {
            let Ok(parsed) = SvgDocument::from_str(&svg.source) else {
                return;
            };
            self.svg_cache.put(svg_hash, parsed.clone());
            parsed
        };

        // Apply clipping if present
        if let Some([clip_x, clip_y, clip_w, clip_h]) = svg.clip_bounds {
            ctx.push_clip(blinc_core::ClipShape::rect(Rect::new(
                clip_x, clip_y, clip_w, clip_h,
            )));
        }

        // Apply opacity if not fully opaque
        if svg.motion_opacity < 1.0 {
            ctx.push_opacity(svg.motion_opacity);
        }

        // Render the SVG with optional CSS overrides
        let has_css_overrides = svg.tint.is_some()
            || svg.fill.is_some()
            || svg.stroke.is_some()
            || svg.stroke_width.is_some();
        if has_css_overrides {
            self.render_svg_with_overrides(
                ctx,
                &doc,
                svg.x,
                svg.y,
                svg.width,
                svg.height,
                svg.tint,
                svg.fill,
                svg.stroke,
                svg.stroke_width,
            );
        } else {
            doc.render_fit(ctx, Rect::new(svg.x, svg.y, svg.width, svg.height));
        }

        // Pop opacity if applied
        if svg.motion_opacity < 1.0 {
            ctx.pop_opacity();
        }

        // Pop clip if applied
        if svg.clip_bounds.is_some() {
            ctx.pop_clip();
        }
    }

    /// Render an SVG with CSS overrides for fill, stroke, stroke-width, and tint
    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_svg_with_overrides(
        &self,
        ctx: &mut GpuPaintContext,
        doc: &SvgDocument,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        tint: Option<blinc_core::Color>,
        fill: Option<blinc_core::Color>,
        stroke: Option<blinc_core::Color>,
        stroke_width: Option<f32>,
    ) {
        use blinc_svg::SvgDrawCommand;

        // Calculate scale to fit within bounds while maintaining aspect ratio
        let scale_x = width / doc.width;
        let scale_y = height / doc.height;
        let scale = scale_x.min(scale_y);

        // Center within bounds
        let scaled_width = doc.width * scale;
        let scaled_height = doc.height * scale;
        let offset_x = x + (width - scaled_width) / 2.0;
        let offset_y = y + (height - scaled_height) / 2.0;

        let commands = doc.commands();

        for cmd in commands {
            match cmd {
                SvgDrawCommand::FillPath { path, brush } => {
                    let scaled = scale_and_translate_path(&path, offset_x, offset_y, scale);
                    // Priority: fill > tint > original brush
                    let fill_brush = if let Some(f) = fill {
                        Brush::Solid(f)
                    } else if let Some(t) = tint {
                        Brush::Solid(t)
                    } else {
                        brush.clone()
                    };
                    ctx.fill_path(&scaled, fill_brush);
                }
                SvgDrawCommand::StrokePath {
                    path,
                    stroke: orig_stroke,
                    brush,
                } => {
                    let scaled = scale_and_translate_path(&path, offset_x, offset_y, scale);
                    // Apply stroke-width override or scale original
                    let sw = stroke_width.unwrap_or(orig_stroke.width) * scale;
                    let scaled_stroke = Stroke::new(sw)
                        .with_cap(orig_stroke.cap)
                        .with_join(orig_stroke.join);
                    // Priority: stroke > tint > original brush
                    let stroke_brush = if let Some(s) = stroke {
                        Brush::Solid(s)
                    } else if let Some(t) = tint {
                        Brush::Solid(t)
                    } else {
                        brush.clone()
                    };
                    ctx.stroke_path(&scaled, &scaled_stroke, stroke_brush);
                }
            }
        }
    }

    /// Rasterize and batch SVG atlas draws in physical pixels.
    pub(super) fn render_rasterized_svgs(
        &mut self,
        target: &wgpu::TextureView,
        svgs: &[SvgElement],
        scale_factor: f32,
    ) {
        // Evict before building instances so atlas repacking cannot invalidate this pass's
        // UVs.
        self.svg_atlas.begin_frame(&self.device);

        // Collect all instances for a single batched draw call
        let mut instances: Vec<GpuImageInstance> = Vec::with_capacity(svgs.len());

        for svg in svgs {
            // Skip completely transparent SVGs
            if svg.motion_opacity <= 0.001 {
                continue;
            }

            // Skip SVGs completely outside their clip bounds. The test is on where the SVG is
            // drawn: rows that move it fold the move into its transform, not its rect.
            if let Some([clip_x, clip_y, clip_w, clip_h]) = svg.clip_bounds {
                let [left, top, right, bottom] = drawn_bounds(svg, scale_factor);
                if left >= clip_x + clip_w
                    || right <= clip_x
                    || top >= clip_y + clip_h
                    || bottom <= clip_y
                {
                    continue;
                }
            }

            // SVG dimensions are already in physical pixels. Rasterize at that resolution
            // without another DPI scale.
            let Some((cache_key, is_tintable)) = self.prepare_svg_raster(svg) else {
                continue;
            };

            // Get the atlas region for this SVG
            let Some(region) = self.svg_atlas.get(cache_key) else {
                continue;
            };
            let src_uv = region.uv_bounds(self.svg_atlas.width(), self.svg_atlas.height());
            self.svg_atlas.mark_used(cache_key);

            // Apply CSS affine transform to SVG bounds if present.
            // Pass full 2x2 affine to shader for rotation, scale, and skew support.
            let (draw_x, draw_y, draw_w, draw_h, ta, tb, tc, td) =
                if let Some([a, b, c, d, tx, ty]) = svg.css_affine {
                    // DPI-scale the translation components
                    let tx_s = tx * scale_factor;
                    let ty_s = ty * scale_factor;

                    // Transform center through the affine (in screen space)
                    let cx = svg.x + svg.width * 0.5;
                    let cy = svg.y + svg.height * 0.5;
                    let new_cx = a * cx + c * cy + tx_s;
                    let new_cy = b * cx + d * cy + ty_s;

                    // Pass original bounds — the 2x2 transform is applied in the shader
                    (
                        new_cx - svg.width * 0.5,
                        new_cy - svg.height * 0.5,
                        svg.width,
                        svg.height,
                        a,
                        b,
                        c,
                        d,
                    )
                } else {
                    // Snap untransformed icons to physical pixels; transformed icons keep
                    // subpixel positions.
                    (
                        svg.x.round(),
                        svg.y.round(),
                        svg.width,
                        svg.height,
                        1.0,
                        0.0,
                        0.0,
                        1.0,
                    )
                };

            // Create instance with atlas UV coordinates
            let mut instance = GpuImageInstance::new(draw_x, draw_y, draw_w, draw_h)
                .with_src_uv(src_uv[0], src_uv[1], src_uv[2], src_uv[3])
                .with_opacity(svg.motion_opacity)
                .with_transform(ta, tb, tc, td);

            // For tintable SVGs, apply color via shader tint multiplication
            // (white texture * tint = correctly colored output)
            if is_tintable && let Some(tint) = svg.tint {
                instance = instance.with_tint(tint.r, tint.g, tint.b, tint.a);
            }

            // Apply clip bounds if specified
            if let Some([clip_x, clip_y, clip_w, clip_h]) = svg.clip_bounds {
                instance = instance.with_clip_rect(clip_x, clip_y, clip_w, clip_h);
            }
            if let Some([x, y, w, h]) = svg.scroll_clip {
                instance = instance.with_clip2_rect(x, y, w, h);
            }

            if let Some((rect, fade)) = svg.fade
                && let Some((params, info)) = fade_mask(rect, fade, drawn_bounds(svg, scale_factor))
            {
                instance.mask_params = params;
                instance.mask_info = info;
            }

            instances.push(instance);
        }

        // Upload atlas to GPU if dirty, then batch-render all SVG instances.
        // The atlas is lazily allocated on first insert; if we have
        // instances we must have inserted at least one this frame, so the
        // view is guaranteed to exist by the time we read it.
        if !instances.is_empty() {
            self.svg_atlas.upload(&self.queue);
            if let Some(view) = self.svg_atlas.view() {
                self.renderer.render_images(target, view, &instances);
            }
        }
    }
}

/// The bounds `svg` covers once drawn, as left, top, right, bottom: its rect, or with a CSS
/// transform the box around the transformed rect, placed as the instance below places it.
fn drawn_bounds(svg: &SvgElement, scale_factor: f32) -> [f32; 4] {
    let Some([a, b, c, d, tx, ty]) = svg.css_affine else {
        return [svg.x, svg.y, svg.x + svg.width, svg.y + svg.height];
    };
    let (cx, cy) = (svg.x + svg.width * 0.5, svg.y + svg.height * 0.5);
    let x = a * cx + c * cy + tx * scale_factor;
    let y = b * cx + d * cy + ty * scale_factor;
    let half_w = (a.abs() * svg.width + c.abs() * svg.height) * 0.5;
    let half_h = (b.abs() * svg.width + d.abs() * svg.height) * 0.5;
    [x - half_w, y - half_h, x + half_w, y + half_h]
}

/// The linear mask that fades an SVG drawn over `bounds` (left, top, right, bottom) at the edges
/// of `rect`, as (`mask_params`, `mask_info`) in the SVG's own 0 to 1 space. `None` where no fade
/// reaches it. An SVG two fades reach takes the one that hides more of its middle.
fn fade_mask(
    rect: [f32; 4],
    [top, right, bottom, left]: [f32; 4],
    [x0, y0, x1, y1]: [f32; 4],
) -> Option<([f32; 4], [f32; 4])> {
    let (w, h) = (x1 - x0, y1 - y0);
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    let [rx, ry, rw, rh] = rect;
    // Each edge: where its ramp starts, hidden, and ends, shown, along the SVG; and its axis.
    let ramps = [
        (top, (ry - y0) / h, (ry + top - y0) / h, false),
        (right, (rx + rw - x0) / w, (rx + rw - right - x0) / w, true),
        (
            bottom,
            (ry + rh - y0) / h,
            (ry + rh - bottom - y0) / h,
            false,
        ),
        (left, (rx - x0) / w, (rx + left - x0) / w, true),
    ];
    let reaches = |hidden: f32, shown: f32| {
        if hidden < shown {
            shown > 0.0
        } else {
            shown < 1.0
        }
    };
    let middle = |hidden: f32, shown: f32| ((0.5 - hidden) / (shown - hidden)).clamp(0.0, 1.0);
    let (_, hidden, shown, across) = ramps
        .into_iter()
        .filter(|&(distance, hidden, shown, _)| distance > 0.0 && reaches(hidden, shown))
        .min_by(|a, b| middle(a.1, a.2).total_cmp(&middle(b.1, b.2)))?;
    let params = if across {
        [hidden, 0.0, shown, 0.0]
    } else {
        [0.0, hidden, 0.0, shown]
    };
    Some((params, [1.0, 0.0, 1.0, 0.0]))
}

/// Scale and translate a path for SVG rendering with tint
fn scale_and_translate_path(
    path: &blinc_core::Path,
    x: f32,
    y: f32,
    scale: f32,
) -> blinc_core::Path {
    use blinc_core::{PathCommand, Point, Vec2};

    if scale == 1.0 && x == 0.0 && y == 0.0 {
        return path.clone();
    }

    let transform_point = |p: Point| -> Point { Point::new(p.x * scale + x, p.y * scale + y) };

    let new_commands: Vec<PathCommand> = path
        .commands()
        .iter()
        .map(|cmd| match cmd {
            PathCommand::MoveTo(p) => PathCommand::MoveTo(transform_point(*p)),
            PathCommand::LineTo(p) => PathCommand::LineTo(transform_point(*p)),
            PathCommand::QuadTo { control, end } => PathCommand::QuadTo {
                control: transform_point(*control),
                end: transform_point(*end),
            },
            PathCommand::CubicTo {
                control1,
                control2,
                end,
            } => PathCommand::CubicTo {
                control1: transform_point(*control1),
                control2: transform_point(*control2),
                end: transform_point(*end),
            },
            PathCommand::ArcTo {
                radii,
                rotation,
                large_arc,
                sweep,
                end,
            } => PathCommand::ArcTo {
                radii: Vec2::new(radii.x * scale, radii.y * scale),
                rotation: *rotation,
                large_arc: *large_arc,
                sweep: *sweep,
                end: transform_point(*end),
            },
            PathCommand::Close => PathCommand::Close,
        })
        .collect();

    blinc_core::Path::from_commands(new_commands)
}
