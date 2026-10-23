//! Render tessellated and atlas-backed SVG elements.
use super::{
    Brush, Color, DefaultHasher, DrawContext, GpuImageInstance, GpuPaintContext, Hash, Hasher,
    RasterizedSvg, Rect, RenderContext, Stroke, SvgDocument, SvgElement,
};

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

            // Skip SVGs completely outside their clip bounds
            if let Some([clip_x, clip_y, clip_w, clip_h]) = svg.clip_bounds {
                let svg_right = svg.x + svg.width;
                let svg_bottom = svg.y + svg.height;
                let clip_right = clip_x + clip_w;
                let clip_bottom = clip_y + clip_h;

                if svg.x >= clip_right
                    || svg_right <= clip_x
                    || svg.y >= clip_bottom
                    || svg_bottom <= clip_y
                {
                    continue;
                }
            }

            // SVG dimensions are already in physical pixels. Rasterize at that resolution
            // without another DPI scale.
            let raster_width = (svg.width.ceil() as u32).max(1);
            let raster_height = (svg.height.ceil() as u32).max(1);

            // Detect tintable SVGs: simple currentColor icons that can use shader tinting
            // instead of CPU re-rasterization per color variant.
            // Tintable = has tint, no other overrides, source uses currentColor.
            let is_tintable = svg.tint.is_some()
                && svg.fill.is_none()
                && svg.stroke.is_none()
                && svg.stroke_width.is_none()
                && svg.stroke_dasharray.is_none()
                && svg.stroke_dashoffset.is_none()
                && svg.svg_path_data.is_none()
                && svg.tag_overrides.is_empty()
                && svg.source.contains("currentColor");

            // Compute cache key: hash of (svg_source, width, height, scale, tint, fill, stroke, stroke_width)
            // For tintable SVGs, exclude tint from hash so all color variants share one texture.
            let cache_key = {
                let mut hasher = DefaultHasher::new();
                svg.source.hash(&mut hasher);
                raster_width.hash(&mut hasher);
                raster_height.hash(&mut hasher);
                if is_tintable {
                    // Sentinel byte to distinguish from non-tintable hashes
                    255u8.hash(&mut hasher);
                } else if let Some(tint) = &svg.tint {
                    tint.r.to_bits().hash(&mut hasher);
                    tint.g.to_bits().hash(&mut hasher);
                    tint.b.to_bits().hash(&mut hasher);
                    tint.a.to_bits().hash(&mut hasher);
                }
                if let Some(fill) = &svg.fill {
                    1u8.hash(&mut hasher);
                    fill.r.to_bits().hash(&mut hasher);
                    fill.g.to_bits().hash(&mut hasher);
                    fill.b.to_bits().hash(&mut hasher);
                    fill.a.to_bits().hash(&mut hasher);
                }
                if let Some(stroke) = &svg.stroke {
                    2u8.hash(&mut hasher);
                    stroke.r.to_bits().hash(&mut hasher);
                    stroke.g.to_bits().hash(&mut hasher);
                    stroke.b.to_bits().hash(&mut hasher);
                    stroke.a.to_bits().hash(&mut hasher);
                }
                if let Some(sw) = &svg.stroke_width {
                    3u8.hash(&mut hasher);
                    sw.to_bits().hash(&mut hasher);
                }
                if let Some(ref da) = svg.stroke_dasharray {
                    4u8.hash(&mut hasher);
                    for v in da {
                        v.to_bits().hash(&mut hasher);
                    }
                }
                if let Some(offset) = &svg.stroke_dashoffset {
                    5u8.hash(&mut hasher);
                    offset.to_bits().hash(&mut hasher);
                }
                if let Some(ref path_data) = svg.svg_path_data {
                    6u8.hash(&mut hasher);
                    path_data.hash(&mut hasher);
                }
                // Hash per-tag style overrides
                if !svg.tag_overrides.is_empty() {
                    7u8.hash(&mut hasher);
                    // Sort keys for deterministic hashing
                    let mut keys: Vec<&String> = svg.tag_overrides.keys().collect();
                    keys.sort();
                    for key in keys {
                        key.hash(&mut hasher);
                        if let Some(ts) = svg.tag_overrides.get(key) {
                            if let Some(f) = &ts.fill {
                                for v in f {
                                    v.to_bits().hash(&mut hasher);
                                }
                            }
                            if let Some(s) = &ts.stroke {
                                for v in s {
                                    v.to_bits().hash(&mut hasher);
                                }
                            }
                            if let Some(sw) = &ts.stroke_width {
                                sw.to_bits().hash(&mut hasher);
                            }
                            if let Some(op) = &ts.opacity {
                                op.to_bits().hash(&mut hasher);
                            }
                        }
                    }
                }
                hasher.finish()
            };

            // Check atlas first — skip string manipulation entirely on cache hit
            if self.svg_atlas.get(cache_key).is_none() {
                // Cache miss: build SVG source with inline attribute overrides
                let has_overrides = svg.tint.is_some()
                    || svg.fill.is_some()
                    || svg.stroke.is_some()
                    || svg.stroke_width.is_some()
                    || svg.stroke_dasharray.is_some()
                    || svg.stroke_dashoffset.is_some()
                    || svg.svg_path_data.is_some()
                    || !svg.tag_overrides.is_empty();

                fn color_val(c: blinc_core::Color) -> String {
                    if c.a < 1.0 {
                        format!(
                            "rgba({},{},{},{})",
                            (c.r * 255.0) as u8,
                            (c.g * 255.0) as u8,
                            (c.b * 255.0) as u8,
                            c.a
                        )
                    } else {
                        format!(
                            "#{:02x}{:02x}{:02x}",
                            (c.r * 255.0) as u8,
                            (c.g * 255.0) as u8,
                            (c.b * 255.0) as u8
                        )
                    }
                }

                let effective_source = if has_overrides {
                    // Build attribute string to inject into the root <svg> tag
                    let mut svg_attrs = String::new();
                    if let Some(fill) = svg.fill {
                        svg_attrs.push_str(&format!(r#" fill="{}""#, color_val(fill)));
                    }
                    if let Some(stroke) = svg.stroke {
                        svg_attrs.push_str(&format!(r#" stroke="{}""#, color_val(stroke)));
                    }
                    if let Some(sw) = svg.stroke_width {
                        svg_attrs.push_str(&format!(r#" stroke-width="{}""#, sw));
                    }
                    if let Some(ref da) = svg.stroke_dasharray {
                        let da_str = da
                            .iter()
                            .map(|v| v.to_string())
                            .collect::<Vec<_>>()
                            .join(",");
                        svg_attrs.push_str(&format!(r#" stroke-dasharray="{}""#, da_str));
                    }
                    if let Some(offset) = svg.stroke_dashoffset {
                        svg_attrs.push_str(&format!(r#" stroke-dashoffset="{}""#, offset));
                    }

                    // Strip existing attribute from a tag region in the SVG string.
                    fn strip_attr(s: &mut String, tag_start: usize, tag_end: usize, attr: &str) {
                        let region = &s[tag_start..tag_end];
                        let attr_eq = format!("{}=", attr);
                        if let Some(attr_offset) = region.find(&attr_eq) {
                            let abs_attr = tag_start + attr_offset;
                            let after_eq = abs_attr + attr.len() + 1;
                            if after_eq < s.len() {
                                let quote = s.as_bytes()[after_eq];
                                if quote == b'"' || quote == b'\'' {
                                    if let Some(end_quote) = s[after_eq + 1..].find(quote as char) {
                                        let remove_end = after_eq + 1 + end_quote + 1;
                                        let remove_start =
                                            if abs_attr > 0 && s.as_bytes()[abs_attr - 1] == b' ' {
                                                abs_attr - 1
                                            } else {
                                                abs_attr
                                            };
                                        s.replace_range(remove_start..remove_end, "");
                                    }
                                }
                            }
                        }
                    }

                    let mut modified = String::from(&*svg.source);

                    // Strip existing attributes from the <svg> tag
                    if let Some(svg_close) = modified.find('>') {
                        if svg.stroke.is_some() {
                            strip_attr(&mut modified, 0, svg_close, "stroke");
                        }
                        if svg.fill.is_some() {
                            let svg_close = modified.find('>').unwrap_or(0);
                            strip_attr(&mut modified, 0, svg_close, "fill");
                        }
                        if svg.stroke_width.is_some() {
                            let svg_close = modified.find('>').unwrap_or(0);
                            strip_attr(&mut modified, 0, svg_close, "stroke-width");
                        }
                        if svg.stroke_dasharray.is_some() {
                            let svg_close = modified.find('>').unwrap_or(0);
                            strip_attr(&mut modified, 0, svg_close, "stroke-dasharray");
                        }
                        if svg.stroke_dashoffset.is_some() {
                            let svg_close = modified.find('>').unwrap_or(0);
                            strip_attr(&mut modified, 0, svg_close, "stroke-dashoffset");
                        }
                    }

                    // Insert new attributes into the opening <svg tag
                    if !svg_attrs.is_empty() {
                        if let Some(pos) = modified.find('>') {
                            let insert_pos = if pos > 0 && modified.as_bytes()[pos - 1] == b'/' {
                                pos - 1
                            } else {
                                pos
                            };
                            modified.insert_str(insert_pos, &svg_attrs);
                        }
                    }

                    // Override fill/stroke on individual shape elements
                    let shape_tags = [
                        "<path",
                        "<circle",
                        "<rect",
                        "<polygon",
                        "<line",
                        "<ellipse",
                        "<polyline",
                    ];
                    for tag in &shape_tags {
                        let tag_name = tag.trim_start_matches('<');
                        let tag_style = svg.tag_overrides.get(tag_name);

                        // Per-tag overrides take priority over global element-level overrides
                        let effective_fill: Option<blinc_core::Color> = tag_style
                            .and_then(|ts| ts.fill)
                            .map(|c| blinc_core::Color::rgba(c[0], c[1], c[2], c[3]))
                            .or(svg.fill);
                        let effective_stroke: Option<blinc_core::Color> = tag_style
                            .and_then(|ts| ts.stroke)
                            .map(|c| blinc_core::Color::rgba(c[0], c[1], c[2], c[3]))
                            .or(svg.stroke);
                        let effective_stroke_width: Option<f32> = tag_style
                            .and_then(|ts| ts.stroke_width)
                            .or(svg.stroke_width);
                        let effective_dasharray: Option<Vec<f32>> = tag_style
                            .and_then(|ts| ts.stroke_dasharray.clone())
                            .or_else(|| svg.stroke_dasharray.clone());
                        let effective_dashoffset: Option<f32> = tag_style
                            .and_then(|ts| ts.stroke_dashoffset)
                            .or(svg.stroke_dashoffset);
                        let effective_opacity: Option<f32> = tag_style.and_then(|ts| ts.opacity);

                        let mut search_from = 0;
                        while let Some(tag_start) = modified[search_from..].find(tag) {
                            let abs_tag = search_from + tag_start;
                            let abs_start = abs_tag + tag.len();
                            if let Some(close) = modified[abs_start..].find('>') {
                                let abs_close = abs_start + close;

                                if effective_stroke.is_some() {
                                    strip_attr(&mut modified, abs_tag, abs_close, "stroke-width");
                                    let new_close = abs_start
                                        + modified[abs_start..].find('>').unwrap_or(close);
                                    strip_attr(&mut modified, abs_tag, new_close, "stroke");
                                }
                                if effective_fill.is_some() {
                                    let new_close = abs_start
                                        + modified[abs_start..].find('>').unwrap_or(close);
                                    strip_attr(&mut modified, abs_tag, new_close, "fill");
                                }
                                if effective_stroke_width.is_some() {
                                    let new_close = abs_start
                                        + modified[abs_start..].find('>').unwrap_or(close);
                                    strip_attr(&mut modified, abs_tag, new_close, "stroke-width");
                                }
                                if effective_dasharray.is_some() {
                                    let new_close = abs_start
                                        + modified[abs_start..].find('>').unwrap_or(close);
                                    strip_attr(
                                        &mut modified,
                                        abs_tag,
                                        new_close,
                                        "stroke-dasharray",
                                    );
                                }
                                if effective_dashoffset.is_some() {
                                    let new_close = abs_start
                                        + modified[abs_start..].find('>').unwrap_or(close);
                                    strip_attr(
                                        &mut modified,
                                        abs_tag,
                                        new_close,
                                        "stroke-dashoffset",
                                    );
                                }
                                if effective_opacity.is_some() {
                                    let new_close = abs_start
                                        + modified[abs_start..].find('>').unwrap_or(close);
                                    strip_attr(&mut modified, abs_tag, new_close, "opacity");
                                }
                                if svg.svg_path_data.is_some() && *tag == "<path" {
                                    let new_close = abs_start
                                        + modified[abs_start..].find('>').unwrap_or(close);
                                    strip_attr(&mut modified, abs_tag, new_close, "d");
                                }

                                // Recompute close position after stripping
                                let abs_close =
                                    abs_start + modified[abs_start..].find('>').unwrap_or(0);
                                let is_self_close =
                                    abs_close > 0 && modified.as_bytes()[abs_close - 1] == b'/';
                                let insert_at = if is_self_close {
                                    abs_close - 1
                                } else {
                                    abs_close
                                };
                                let mut elem_attrs = String::new();
                                if let Some(fill) = effective_fill {
                                    elem_attrs.push_str(&format!(r#" fill="{}""#, color_val(fill)));
                                }
                                if let Some(stroke) = effective_stroke {
                                    elem_attrs
                                        .push_str(&format!(r#" stroke="{}""#, color_val(stroke)));
                                }
                                if let Some(sw) = effective_stroke_width {
                                    elem_attrs.push_str(&format!(r#" stroke-width="{}""#, sw));
                                }
                                if let Some(ref da) = effective_dasharray {
                                    let da_str = da
                                        .iter()
                                        .map(|v| v.to_string())
                                        .collect::<Vec<_>>()
                                        .join(",");
                                    elem_attrs
                                        .push_str(&format!(r#" stroke-dasharray="{}""#, da_str));
                                }
                                if let Some(offset) = effective_dashoffset {
                                    elem_attrs
                                        .push_str(&format!(r#" stroke-dashoffset="{}""#, offset));
                                }
                                if let Some(opacity) = effective_opacity {
                                    elem_attrs.push_str(&format!(r#" opacity="{}""#, opacity));
                                }
                                if let Some(ref path_data) = svg.svg_path_data {
                                    if *tag == "<path" {
                                        elem_attrs.push_str(&format!(r#" d="{}""#, path_data));
                                    }
                                }
                                modified.insert_str(insert_at, &elem_attrs);
                                search_from = insert_at + elem_attrs.len() + 1;
                            } else {
                                break;
                            }
                        }
                    }

                    std::borrow::Cow::Owned(modified)
                } else {
                    std::borrow::Cow::Borrowed(&*svg.source)
                };

                // Rasterize tintable currentColor content as white for shader tinting;
                // otherwise resolve the color on the CPU.
                let has_current_color = effective_source.contains("currentColor");
                let needs_post_raster_tint =
                    !is_tintable && svg.tint.is_some() && !has_current_color;
                let final_source = if is_tintable {
                    std::borrow::Cow::Owned(effective_source.replace("currentColor", "#ffffff"))
                } else if let Some(tint) = svg.tint {
                    if has_current_color {
                        std::borrow::Cow::Owned(
                            effective_source.replace("currentColor", &color_val(tint)),
                        )
                    } else {
                        effective_source
                    }
                } else {
                    effective_source
                };

                let rasterized =
                    RasterizedSvg::from_str(&final_source, raster_width, raster_height);

                let mut rasterized = match rasterized {
                    Ok(r) => r,
                    Err(e) => {
                        tracing::warn!("Failed to rasterize SVG: {}", e);
                        continue;
                    }
                };

                // Apply explicit tint to authored fills while preserving alpha.
                if needs_post_raster_tint {
                    rasterized.apply_tint(svg.tint.unwrap());
                }

                // Insert into atlas (handles grow/clear internally)
                if self
                    .svg_atlas
                    .insert(
                        cache_key,
                        rasterized.width,
                        rasterized.height,
                        rasterized.data(),
                        &self.device,
                    )
                    .is_none()
                {
                    tracing::warn!(
                        "SVG atlas full, could not allocate {}x{}",
                        raster_width,
                        raster_height
                    );
                    continue;
                }
            }

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
            if is_tintable {
                if let Some(tint) = svg.tint {
                    instance = instance.with_tint(tint.r, tint.g, tint.b, tint.a);
                }
            }

            // Apply clip bounds if specified
            if let Some([clip_x, clip_y, clip_w, clip_h]) = svg.clip_bounds {
                instance = instance.with_clip_rect(clip_x, clip_y, clip_w, clip_h);
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
