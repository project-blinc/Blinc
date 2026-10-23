//! Load and render images, masks and their clip/filter state.
use super::{
    Color, CornerRadius, DrawContext, DrawContextExt, GpuImageInstance, GpuPaintContext,
    ImageElement, LruCache, PrimitiveBatch, Rect, RenderContext, bc_eligible,
};

impl RenderContext {
    /// Evict matching file-backed images and invalidate source-keyed SVG caches.
    pub fn invalidate_asset_path(&mut self, abs_path: &std::path::Path) {
        let abs_str = abs_path.to_string_lossy();
        // Match relative or absolute file paths against cache keys; keep URL entries.
        let dropped: Vec<String> = self
            .image_cache
            .iter()
            .filter_map(|(k, _)| {
                if !k.is_empty() && abs_str.ends_with(k.as_str()) {
                    Some(k.clone())
                } else {
                    None
                }
            })
            .collect();
        for k in &dropped {
            self.image_cache.pop(k);
            self.image_load_times.remove(k);
            self.image_fade_deadlines.remove(k);
        }
        if !dropped.is_empty() {
            tracing::info!(
                "hot-reload: dropped {} image cache entries for {}",
                dropped.len(),
                abs_path.display()
            );
        }

        // SVG caches have no path index, so clear their source-keyed entries together.
        let svg_n = self.svg_cache.len();
        if svg_n > 0 {
            self.svg_cache.clear();
            self.svg_atlas.clear();
            tracing::info!(
                "hot-reload: cleared {} SVG doc cache entries + atlas",
                svg_n
            );
        }
    }

    /// Render images to the backdrop texture (for images that should be blurred by glass)
    pub(super) fn render_images_to_backdrop(&mut self, images: &[&ImageElement]) {
        let Some(ref backdrop) = self.backdrop_texture else {
            return;
        };
        // Create a new view to avoid borrow conflicts
        let target = backdrop
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        self.render_images_ref(&target, images);
    }

    /// Load image sources and populate the bounded GPU cache before dispatch.
    pub(super) fn preload_images(
        &mut self,
        images: &[ImageElement],
        viewport_width: f32,
        viewport_height: f32,
    ) {
        // Buffer zone: load images that are within 100px of becoming visible
        const VISIBILITY_BUFFER: f32 = 100.0;

        // Eagerly load placeholder images for any lazy element with placeholder_type == 2
        // (so the placeholder is already in cache when we go to render it).
        // Use get() instead of contains() so cached placeholders are promoted to
        // MRU and survive eviction pressure from the main image puts below.
        for image in images {
            if image.placeholder_type == 2 {
                if let Some(ref placeholder_src) = image.placeholder_image {
                    if self.image_cache.get(placeholder_src).is_none() {
                        let source = blinc_image::ImageSource::from_uri(placeholder_src);
                        if let Ok(data) = blinc_image::ImageData::load(source) {
                            // Keep the image pipeline's linear-byte sampling convention.
                            let has_bc = self.renderer.has_texture_compression_bc()
                                && bc_eligible(placeholder_src, data.width(), data.height());
                            let gpu_image = self.image_ctx.create_image_maybe_compressed(
                                data.pixels(),
                                data.width(),
                                data.height(),
                                false,
                                has_bc,
                                placeholder_src,
                            );
                            self.image_cache.put(placeholder_src.clone(), gpu_image);
                        }
                    }
                }
            }
        }

        for image in images {
            // Promote visible cache hits to MRU so scrolling evicts inactive images first.
            if self.image_cache.get(&image.source).is_some() {
                continue;
            }

            // Check if lazy loading is enabled (loading_strategy == 1)
            if image.loading_strategy == 1 {
                // If image has clip bounds from a scroll container, use those for visibility check
                // The clip bounds represent the visible area of the parent scroll container
                let is_visible = if let Some([clip_x, clip_y, clip_w, clip_h]) = image.clip_bounds {
                    // Check if image intersects with its clip region (+ buffer for prefetching)
                    let clip_left = clip_x - VISIBILITY_BUFFER;
                    let clip_top = clip_y - VISIBILITY_BUFFER;
                    let clip_right = clip_x + clip_w + VISIBILITY_BUFFER;
                    let clip_bottom = clip_y + clip_h + VISIBILITY_BUFFER;

                    let image_right = image.x + image.width;
                    let image_bottom = image.y + image.height;

                    image.x < clip_right
                        && image_right > clip_left
                        && image.y < clip_bottom
                        && image_bottom > clip_top
                } else {
                    // No clip bounds - check against viewport
                    let viewport_left = -VISIBILITY_BUFFER;
                    let viewport_top = -VISIBILITY_BUFFER;
                    let viewport_right = viewport_width + VISIBILITY_BUFFER;
                    let viewport_bottom = viewport_height + VISIBILITY_BUFFER;

                    let image_right = image.x + image.width;
                    let image_bottom = image.y + image.height;

                    image.x < viewport_right
                        && image_right > viewport_left
                        && image.y < viewport_bottom
                        && image_bottom > viewport_top
                };

                if !is_visible {
                    // Skip loading - image is not yet visible
                    continue;
                }
            }

            // Try to load the image - use from_uri to handle emoji://, data:, and file paths
            let source = blinc_image::ImageSource::from_uri(&image.source);
            let image_data = match blinc_image::ImageData::load(source) {
                Ok(data) => data,
                Err(e) => {
                    tracing::trace!("Failed to load image '{}': {:?}", image.source, e);
                    continue; // Skip images that fail to load
                }
            };

            // Compress eligible large images when supported; keep small icons and emoji
            // lossless.
            let has_bc = self.renderer.has_texture_compression_bc()
                && bc_eligible(&image.source, image_data.width(), image_data.height());
            let gpu_image = self.image_ctx.create_image_maybe_compressed(
                image_data.pixels(),
                image_data.width(),
                image_data.height(),
                false,
                has_bc,
                &image.source,
            );

            // LruCache::put evicts oldest entry if at capacity
            self.image_cache.put(image.source.clone(), gpu_image);
            // Record load time for fade-in animation
            let now = web_time::Instant::now();
            self.image_load_times.insert(image.source.clone(), now);
            // Store the fade deadline so redraw stops when the fade completes.
            if image.fade_duration_ms > 0 {
                if let Some(deadline) = now.checked_add(std::time::Duration::from_millis(
                    image.fade_duration_ms as u64,
                )) {
                    self.image_fade_deadlines
                        .insert(image.source.clone(), deadline);
                }
            }
        }
    }

    /// Prepare image-mask resources referenced by this batch.
    pub(super) fn preload_mask_images(&mut self, batch: &PrimitiveBatch) {
        use blinc_core::LayerEffect;
        for entry in &batch.layer_commands {
            if let blinc_gpu::primitives::LayerCommand::Push { config } = &entry.command {
                for effect in &config.effects {
                    if let LayerEffect::MaskImage { image_url, .. } = effect {
                        if self.renderer.has_mask_image(image_url) {
                            continue;
                        }
                        let source = blinc_image::ImageSource::from_uri(image_url);
                        if let Ok(data) = blinc_image::ImageData::load(source) {
                            self.renderer.load_mask_image_rgba(
                                image_url,
                                data.pixels(),
                                data.width(),
                                data.height(),
                            );
                        }
                    }
                }
            }
        }
    }

    /// Convert a CssFilter into filter_a/filter_b arrays for the image shader.
    /// Returns (filter_a, filter_b) where identity = (`[0,0,0,0]`, `[1,1,1,0]`).
    /// Extract mask gradient params and info from a MaskImage gradient.
    /// Returns (`mask_params`, `mask_info`) or zero arrays if not a gradient.
    pub(super) fn mask_image_to_arrays(
        mask: Option<&blinc_core::MaskImage>,
    ) -> ([f32; 4], [f32; 4]) {
        match mask {
            Some(blinc_core::MaskImage::Gradient(gradient)) => match gradient {
                blinc_core::Gradient::Linear {
                    start, end, stops, ..
                } => {
                    let (sa, ea) = Self::extract_mask_alphas_from_stops(stops);
                    ([start.x, start.y, end.x, end.y], [1.0, sa, ea, 0.0])
                }
                blinc_core::Gradient::Radial {
                    center,
                    radius,
                    stops,
                    ..
                } => {
                    let (sa, ea) = Self::extract_mask_alphas_from_stops(stops);
                    ([center.x, center.y, *radius, 0.0], [2.0, sa, ea, 0.0])
                }
                blinc_core::Gradient::Conic { center, stops, .. } => {
                    let (sa, ea) = Self::extract_mask_alphas_from_stops(stops);
                    ([center.x, center.y, 0.5, 0.0], [2.0, sa, ea, 0.0])
                }
            },
            _ => ([0.0; 4], [0.0; 4]),
        }
    }

    pub(super) fn extract_mask_alphas_from_stops(stops: &[blinc_core::GradientStop]) -> (f32, f32) {
        if stops.is_empty() {
            return (1.0, 0.0);
        }
        (stops[0].color.a, stops[stops.len() - 1].color.a)
    }

    pub(super) fn css_filter_to_arrays(
        filter: &blinc_layout::element_style::CssFilter,
    ) -> ([f32; 4], [f32; 4]) {
        (
            [
                filter.grayscale,
                filter.invert,
                filter.sepia,
                filter.hue_rotate.to_radians(),
            ],
            [filter.brightness, filter.contrast, filter.saturate, 0.0],
        )
    }

    /// Transform clip bounds and radii by a CSS affine.
    /// When a parent div has a CSS transform (e.g. `scale(1.08)` on hover), the image
    /// clip must follow the same transform so the image fills the visually-scaled parent.
    pub(super) fn transform_clip_by_affine(
        clip: [f32; 4],
        clip_radius: [f32; 4],
        affine: [f32; 6],
        scale_factor: f32,
    ) -> ([f32; 4], [f32; 4]) {
        let [a, b, c, d, tx, ty] = affine;
        let tx_s = tx * scale_factor;
        let ty_s = ty * scale_factor;
        // Transform clip center through the affine
        let ccx = clip[0] + clip[2] * 0.5;
        let ccy = clip[1] + clip[3] * 0.5;
        let new_cx = a * ccx + c * ccy + tx_s;
        let new_cy = b * ccx + d * ccy + ty_s;
        // Uniform scale for dimensions
        let s = (a * d - b * c).abs().sqrt().max(1e-6);
        let new_clip = [
            new_cx - clip[2] * s * 0.5,
            new_cy - clip[3] * s * 0.5,
            clip[2] * s,
            clip[3] * s,
        ];
        let new_radius = [
            clip_radius[0] * s,
            clip_radius[1] * s,
            clip_radius[2] * s,
            clip_radius[3] * s,
        ];
        (new_clip, new_radius)
    }

    /// Decompose a CSS affine `[a,b,c,d,tx,ty]` into position and 2x2 transform for image rendering.
    /// Input: original rect (already DPI-scaled), affine (layout coords), scale_factor.
    /// Returns: (draw_x, draw_y, draw_w, draw_h, transform_a, transform_b, transform_c, transform_d)
    /// The 2x2 matrix `[a, b, c, d]` is passed to the shader for full affine support (rotation, scale, skew).
    pub(super) fn decompose_image_affine(
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        affine: [f32; 6],
        scale_factor: f32,
    ) -> (f32, f32, f32, f32, f32, f32, f32, f32) {
        let [a, b, c, d, tx, ty] = affine;
        // DPI-scale the translation components
        let tx_s = tx * scale_factor;
        let ty_s = ty * scale_factor;
        // Transform center through the affine (positions are already in screen space)
        let cx = x + w * 0.5;
        let cy = y + h * 0.5;
        let new_cx = a * cx + c * cy + tx_s;
        let new_cy = b * cx + d * cy + ty_s;
        // Pass original bounds — the 2x2 transform is applied in the shader around the center
        (new_cx - w * 0.5, new_cy - h * 0.5, w, h, a, b, c, d)
    }

    /// Render collected images with their transforms, clips, masks and filters.
    pub(super) fn render_images(
        &mut self,
        target: &wgpu::TextureView,
        images: &[ImageElement],
        viewport_width: f32,
        viewport_height: f32,
        scale_factor: f32,
    ) {
        use blinc_image::{ObjectFit, ObjectPosition, calculate_fit_rects, src_rect_to_uv};

        for image in images {
            // Get cached GPU image
            let gpu_image = self.image_cache.get(&image.source);

            // Compute fade-in opacity multiplier from load time + duration
            // Returns 1.0 if no fade configured or fade complete; <1.0 during fade
            let fade_factor = if image.fade_duration_ms > 0 && gpu_image.is_some() {
                if let Some(loaded_at) = self.image_load_times.get(&image.source) {
                    let elapsed_ms = loaded_at.elapsed().as_secs_f32() * 1000.0;
                    (elapsed_ms / image.fade_duration_ms as f32).clamp(0.0, 1.0)
                } else {
                    1.0
                }
            } else {
                1.0
            };
            // Fade activity is determined before the compositor cache decision.

            // If image is not loaded and has a placeholder, render placeholder
            if gpu_image.is_none() && image.placeholder_type != 0 {
                match image.placeholder_type {
                    // Type 1: Solid color
                    1 => {
                        let color = blinc_core::Color::rgba(
                            image.placeholder_color[0],
                            image.placeholder_color[1],
                            image.placeholder_color[2],
                            image.placeholder_color[3],
                        );
                        let mut ctx = GpuPaintContext::new(viewport_width, viewport_height);
                        let rect =
                            blinc_core::Rect::new(image.x, image.y, image.width, image.height);
                        ctx.fill_rounded_rect(
                            rect,
                            blinc_core::CornerRadius::uniform(image.border_radius),
                            color,
                        );
                        let batch = ctx.take_batch();
                        self.renderer.render_overlay(target, &batch);
                    }
                    // Type 2: Image placeholder (e.g., low-res thumbnail or blur hash)
                    2 => {
                        if let Some(ref placeholder_src) = image.placeholder_image {
                            if let Some(placeholder_gpu) = self.image_cache.get(placeholder_src) {
                                let (src_rect, dst_rect) = calculate_fit_rects(
                                    placeholder_gpu.width(),
                                    placeholder_gpu.height(),
                                    image.width,
                                    image.height,
                                    ObjectFit::Cover,
                                    ObjectPosition::new(0.5, 0.5),
                                );
                                let src_uv = src_rect_to_uv(
                                    src_rect,
                                    placeholder_gpu.width(),
                                    placeholder_gpu.height(),
                                );
                                let instance = GpuImageInstance::new(
                                    image.x + dst_rect[0],
                                    image.y + dst_rect[1],
                                    dst_rect[2],
                                    dst_rect[3],
                                )
                                .with_src_uv(src_uv[0], src_uv[1], src_uv[2], src_uv[3])
                                .with_border_radius(image.border_radius)
                                .with_opacity(image.opacity);
                                self.renderer.render_images(
                                    target,
                                    placeholder_gpu.view(),
                                    &[instance],
                                );
                            }
                        }
                    }
                    // Type 3: Skeleton shimmer (animated gradient sweep)
                    3 => {
                        let t =
                            self.frame_count.saturating_mul(16).rem_euclid(2400) as f32 / 2400.0;
                        let base_a = image.placeholder_color[3].max(0.4);
                        let base = blinc_core::Color::rgba(
                            image.placeholder_color[0],
                            image.placeholder_color[1],
                            image.placeholder_color[2],
                            base_a,
                        );
                        let highlight_a = (base_a + 0.25).min(1.0);
                        let highlight = blinc_core::Color::rgba(
                            (image.placeholder_color[0] + 0.15).min(1.0),
                            (image.placeholder_color[1] + 0.15).min(1.0),
                            (image.placeholder_color[2] + 0.15).min(1.0),
                            highlight_a,
                        );
                        let mut ctx = GpuPaintContext::new(viewport_width, viewport_height);
                        let rect =
                            blinc_core::Rect::new(image.x, image.y, image.width, image.height);
                        // Base background
                        ctx.fill_rounded_rect(
                            rect,
                            blinc_core::CornerRadius::uniform(image.border_radius),
                            base,
                        );
                        // Shimmer band — narrow vertical strip swept horizontally
                        let band_w = (image.width * 0.25).max(40.0);
                        let band_x = image.x + (image.width + band_w) * t - band_w;
                        let band_rect =
                            blinc_core::Rect::new(band_x, image.y, band_w, image.height);
                        ctx.fill_rounded_rect(
                            band_rect,
                            blinc_core::CornerRadius::uniform(image.border_radius),
                            highlight,
                        );
                        let batch = ctx.take_batch();
                        self.renderer.render_overlay(target, &batch);
                        // Mark frame as needing redraw for animation
                        self.has_active_flows = true;
                    }
                    _ => {}
                }
                continue;
            }

            let Some(gpu_image) = gpu_image else {
                continue; // Skip images that failed to load
            };

            // Convert object_fit byte to ObjectFit enum
            let object_fit = match image.object_fit {
                0 => ObjectFit::Cover,
                1 => ObjectFit::Contain,
                2 => ObjectFit::Fill,
                3 => ObjectFit::ScaleDown,
                4 => ObjectFit::None,
                _ => ObjectFit::Cover,
            };

            // Create ObjectPosition from array
            let object_position =
                ObjectPosition::new(image.object_position[0], image.object_position[1]);

            // Calculate fit rectangles
            let (src_rect, dst_rect) = calculate_fit_rects(
                gpu_image.width(),
                gpu_image.height(),
                image.width,
                image.height,
                object_fit,
                object_position,
            );

            // Convert src_rect to UV coordinates
            let src_uv = src_rect_to_uv(src_rect, gpu_image.width(), gpu_image.height());

            // Apply CSS affine transform if present
            let base_x = image.x + dst_rect[0];
            let base_y = image.y + dst_rect[1];
            let base_w = dst_rect[2];
            let base_h = dst_rect[3];

            let (draw_x, draw_y, draw_w, draw_h, ta, tb, tc, td) = if let Some(affine) =
                image.css_affine
            {
                Self::decompose_image_affine(base_x, base_y, base_w, base_h, affine, scale_factor)
            } else {
                (base_x, base_y, base_w, base_h, 1.0, 0.0, 0.0, 1.0)
            };

            // Pre-compute effective clip (transformed by CSS affine if present)
            let effective_clip = image.clip_bounds.map(|clip| {
                if let Some(affine) = image.css_affine {
                    Self::transform_clip_by_affine(clip, image.clip_radius, affine, scale_factor)
                } else {
                    (clip, image.clip_radius)
                }
            });

            // Render shadow before image if present
            if let Some(ref shadow) = image.shadow {
                let mut shadow_ctx = GpuPaintContext::new(viewport_width, viewport_height);
                // Push scroll/parent clip so shadow doesn't escape the container
                if let Some(clip) = image.clip_bounds {
                    shadow_ctx.push_clip(blinc_core::ClipShape::RoundedRect {
                        rect: blinc_core::Rect::new(clip[0], clip[1], clip[2], clip[3]),
                        corner_radius: blinc_core::CornerRadius {
                            top_left: image.clip_radius[0],
                            top_right: image.clip_radius[1],
                            bottom_right: image.clip_radius[2],
                            bottom_left: image.clip_radius[3],
                        },
                        corner_shape: blinc_core::CornerShape::ROUND,
                    });
                }
                let shadow_rect =
                    blinc_core::Rect::new(image.x, image.y, image.width, image.height);
                let shadow_radius = blinc_core::CornerRadius::uniform(image.border_radius);
                shadow_ctx.draw_shadow(shadow_rect, shadow_radius, *shadow);
                let shadow_batch = shadow_ctx.take_batch();
                self.renderer.render_overlay(target, &shadow_batch);
            }

            // Create GPU instance with proper positioning
            let mut instance = GpuImageInstance::new(draw_x, draw_y, draw_w, draw_h)
                .with_src_uv(src_uv[0], src_uv[1], src_uv[2], src_uv[3])
                .with_tint(image.tint[0], image.tint[1], image.tint[2], image.tint[3])
                .with_border_radius(image.border_radius)
                .with_opacity(image.opacity * fade_factor)
                .with_transform(ta, tb, tc, td)
                .with_filter(image.filter_a, image.filter_b);

            // Render border inside the image shader (same SDF, perfect transform alignment)
            if image.border_width > 0.0 {
                instance = instance.with_image_border(
                    image.border_width,
                    image.border_color.r,
                    image.border_color.g,
                    image.border_color.b,
                    image.border_color.a,
                );
            }

            // Apply mask gradient
            if image.mask_info[0] > 0.5 {
                instance.mask_params = image.mask_params;
                instance.mask_info = image.mask_info;
            }

            // Apply clip bounds (primary rounded clip)
            if let Some((clip, clip_r)) = effective_clip {
                instance = instance.with_clip_rounded_rect_corners(
                    clip[0], clip[1], clip[2], clip[3], clip_r[0], clip_r[1], clip_r[2], clip_r[3],
                );
            }
            // Apply secondary scroll clip (sharp rect)
            if let Some(sc) = image.scroll_clip {
                instance = instance.with_clip2_rect(sc[0], sc[1], sc[2], sc[3]);
            }

            // Render the image
            self.renderer
                .render_images(target, gpu_image.view(), &[instance]);
        }
    }

    /// Render borrowed image elements through the standard image path.
    pub(super) fn render_images_ref(
        &mut self,
        target: &wgpu::TextureView,
        images: &[&ImageElement],
    ) {
        use blinc_image::{ObjectFit, ObjectPosition, calculate_fit_rects, src_rect_to_uv};

        for image in images {
            // Get cached GPU image
            let Some(gpu_image) = self.image_cache.get(&image.source) else {
                continue; // Skip images that failed to load
            };

            // Compute fade-in opacity multiplier
            let fade_factor = if image.fade_duration_ms > 0 {
                if let Some(loaded_at) = self.image_load_times.get(&image.source) {
                    let elapsed_ms = loaded_at.elapsed().as_secs_f32() * 1000.0;
                    (elapsed_ms / image.fade_duration_ms as f32).clamp(0.0, 1.0)
                } else {
                    1.0
                }
            } else {
                1.0
            };
            // Fade signal lives on `has_pending_image_fade`, set
            // at the top of `try_render_with_compositor` from
            // `image_load_times`. No per-dispatch set needed here.

            // Convert object_fit byte to ObjectFit enum
            let object_fit = match image.object_fit {
                0 => ObjectFit::Cover,
                1 => ObjectFit::Contain,
                2 => ObjectFit::Fill,
                3 => ObjectFit::ScaleDown,
                4 => ObjectFit::None,
                _ => ObjectFit::Cover,
            };

            // Create ObjectPosition from array
            let object_position =
                ObjectPosition::new(image.object_position[0], image.object_position[1]);

            // Calculate fit rectangles
            let (src_rect, dst_rect) = calculate_fit_rects(
                gpu_image.width(),
                gpu_image.height(),
                image.width,
                image.height,
                object_fit,
                object_position,
            );

            // Convert src_rect to UV coordinates
            let src_uv = src_rect_to_uv(src_rect, gpu_image.width(), gpu_image.height());

            // Apply CSS affine transform if present
            let base_x = image.x + dst_rect[0];
            let base_y = image.y + dst_rect[1];
            let base_w = dst_rect[2];
            let base_h = dst_rect[3];

            // render_images_ref is called for backdrop images; no scale_factor available,
            // but affine translation is already in screen coords for backdrop path
            let (draw_x, draw_y, draw_w, draw_h, ta, tb, tc, td) =
                if let Some(affine) = image.css_affine {
                    Self::decompose_image_affine(base_x, base_y, base_w, base_h, affine, 1.0)
                } else {
                    (base_x, base_y, base_w, base_h, 1.0, 0.0, 0.0, 1.0)
                };

            // Pre-compute effective clip (transformed by CSS affine if present)
            let effective_clip = image.clip_bounds.map(|clip| {
                if let Some(affine) = image.css_affine {
                    Self::transform_clip_by_affine(clip, image.clip_radius, affine, 1.0)
                } else {
                    (clip, image.clip_radius)
                }
            });

            // Create GPU instance with proper positioning
            let mut instance = GpuImageInstance::new(draw_x, draw_y, draw_w, draw_h)
                .with_src_uv(src_uv[0], src_uv[1], src_uv[2], src_uv[3])
                .with_tint(image.tint[0], image.tint[1], image.tint[2], image.tint[3])
                .with_border_radius(image.border_radius)
                .with_opacity(image.opacity * fade_factor)
                .with_transform(ta, tb, tc, td)
                .with_filter(image.filter_a, image.filter_b);

            // Render border inside the image shader (same SDF, perfect transform alignment)
            if image.border_width > 0.0 {
                instance = instance.with_image_border(
                    image.border_width,
                    image.border_color.r,
                    image.border_color.g,
                    image.border_color.b,
                    image.border_color.a,
                );
            }

            // Apply mask gradient
            if image.mask_info[0] > 0.5 {
                instance.mask_params = image.mask_params;
                instance.mask_info = image.mask_info;
            }

            // Apply clip bounds (primary rounded clip)
            if let Some((clip, clip_r)) = effective_clip {
                instance = instance.with_clip_rounded_rect_corners(
                    clip[0], clip[1], clip[2], clip[3], clip_r[0], clip_r[1], clip_r[2], clip_r[3],
                );
            }
            // Apply secondary scroll clip (sharp rect)
            if let Some(sc) = image.scroll_clip {
                instance = instance.with_clip2_rect(sc[0], sc[1], sc[2], sc[3]);
            }

            // Render the image
            self.renderer
                .render_images(target, gpu_image.view(), &[instance]);
        }
    }
}
