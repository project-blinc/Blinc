//! Load and render images, masks and their clip/filter state.
use super::{
    Color, CornerRadius, DrawContext, DrawContextExt, GpuImageInstance, GpuPaintContext,
    ImageElement, LruCache, PrimitiveBatch, Rect, RenderContext, bc_eligible,
};

mod instance;
mod placeholder;

pub(super) use placeholder::{blurs_in_the_frame, has_backdrop_placeholder, is_near_view};

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
        self.image_failures
            .forget(|k| !k.is_empty() && abs_str.ends_with(k));
        for k in &dropped {
            self.image_cache.pop(k);
            self.image_load_times.remove(k);
            self.image_fade_deadlines.remove(k);
        }
        if !dropped.is_empty() {
            self.invalidate_render_cache();
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

    /// Put the images the worker has finished into the cache, and ask for a full paint when one
    /// arrived, so it is drawn. Start again the failed loads whose wait is over.
    pub(super) fn take_decoded_images(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let Some(decoder) = self.image_decode.as_mut() else {
                return;
            };
            let bc = self.renderer.has_texture_compression_bc();
            for due in self.image_failures.due() {
                if !decoder.request(&due.source, due.fade_duration_ms, due.retry, bc) {
                    self.image_failures
                        .failed(&due.source, due.fade_duration_ms, due.retry);
                }
            }
            for decoded in decoder.take() {
                let Some(image) = decoded.image else {
                    tracing::trace!("Failed to load image '{}'", decoded.source);
                    self.image_failures.failed(
                        &decoded.source,
                        decoded.fade_duration_ms,
                        decoded.retry,
                    );
                    continue;
                };
                self.image_failures.loaded(&decoded.source);
                self.image_cache.put(decoded.source.clone(), image);
                let now = web_time::Instant::now();
                self.image_load_times.insert(decoded.source.clone(), now);
                if decoded.fade_duration_ms > 0
                    && let Some(deadline) = now.checked_add(std::time::Duration::from_millis(
                        decoded.fade_duration_ms as u64,
                    ))
                {
                    self.image_fade_deadlines.insert(decoded.source, deadline);
                }
                self.images_need_repaint = true;
            }
        }
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
        let viewport = (viewport_width, viewport_height);
        self.take_decoded_images();

        // Eagerly load placeholder images for any lazy element with placeholder_type == 2
        // (so the placeholder is already in cache when we go to render it).
        // Use get() instead of contains() so cached placeholders are promoted to
        // MRU and survive eviction pressure from the main image puts below.
        for image in images {
            if image.placeholder_type == 2
                && let Some(ref placeholder_src) = image.placeholder_image
                && self.image_cache.get(placeholder_src).is_none()
            {
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

        for image in images {
            // Promote visible cache hits to MRU so scrolling evicts inactive images first.
            if self.image_cache.get(&image.source).is_some() {
                continue;
            }

            // A lazy image (loading_strategy == 1) waits until it is near what shows.
            if image.loading_strategy == 1 && !is_near_view(image, viewport, VISIBILITY_BUFFER) {
                continue;
            }

            // A failed load waits as the image's retry says.
            if !self.image_failures.may_try(&image.source) {
                continue;
            }

            #[cfg(not(target_arch = "wasm32"))]
            if let Some(decoder) = self.image_decode.as_mut() {
                let bc = self.renderer.has_texture_compression_bc();
                if !decoder.request(&image.source, image.fade_duration_ms, image.retry, bc) {
                    self.image_failures
                        .failed(&image.source, image.fade_duration_ms, image.retry);
                }
                continue;
            }

            // Try to load the image - use from_uri to handle emoji://, data:, and file paths
            let source = blinc_image::ImageSource::from_uri(&image.source);
            let image_data = match blinc_image::ImageData::load(source) {
                Ok(data) => data,
                Err(e) => {
                    tracing::trace!("Failed to load image '{}': {:?}", image.source, e);
                    self.image_failures
                        .failed(&image.source, image.fade_duration_ms, image.retry);
                    continue;
                }
            };
            self.image_failures.loaded(&image.source);

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
            if image.fade_duration_ms > 0
                && let Some(deadline) = now.checked_add(std::time::Duration::from_millis(
                    image.fade_duration_ms as u64,
                ))
            {
                self.image_fade_deadlines
                    .insert(image.source.clone(), deadline);
            }
        }
    }

    /// Prepare image-mask resources referenced by this batch.
    pub(super) fn preload_mask_images(&mut self, batch: &PrimitiveBatch) {
        use blinc_core::LayerEffect;
        for entry in batch
            .layer_commands
            .iter()
            .chain(&batch.foreground_layer_commands)
        {
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
    pub(super) fn render_images<'a>(
        &mut self,
        target: &wgpu::TextureView,
        images: impl IntoIterator<Item = &'a ImageElement>,
        viewport_width: f32,
        viewport_height: f32,
        scale_factor: f32,
    ) {
        use blinc_image::{ObjectFit, ObjectPosition, calculate_fit_rects, src_rect_to_uv};

        // Loaded images with nothing drawn between them share one pass; anything else drawn in
        // between ends the run first, so the order is kept.
        let mut run: Vec<(String, GpuImageInstance)> = Vec::new();
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
            // Fade activity is determined before the compositor cache decision, by
            // `has_pending_image_fade`.

            // If image is not loaded and has a placeholder, render placeholder
            if gpu_image.is_none() && image.placeholder_type != 0 {
                // A glass or blur placeholder of an image drawn in the frame is among the
                // frame's glass.
                if blurs_in_the_frame(image) {
                    continue;
                }
                self.flush_image_run(target, &mut run);
                let batch = self.paint_image(
                    image,
                    image.opacity,
                    (viewport_width as u32, viewport_height as u32),
                    scale_factor,
                    0,
                    false,
                );
                self.renderer.render_overlay(target, &batch);
                continue;
            }

            let Some(gpu_image) = gpu_image else {
                continue; // Skip images that failed to load
            };
            let (image_w, image_h) = (gpu_image.width(), gpu_image.height());

            let instance = Self::image_instance(image, image_w, image_h, fade_factor, scale_factor);

            run.push((image.source.clone(), instance));
        }
        self.flush_image_run(target, &mut run);
    }

    /// Draw the loaded images collected in `run` in one pass, and empty it.
    fn flush_image_run(
        &mut self,
        target: &wgpu::TextureView,
        run: &mut Vec<(String, GpuImageInstance)>,
    ) {
        if run.is_empty() {
            return;
        }
        let images: Vec<_> = run
            .iter()
            .filter_map(|(source, instance)| {
                self.image_cache
                    .peek(source)
                    .map(|image| (image, *instance))
            })
            .collect();
        let draws: Vec<_> = images
            .iter()
            .map(|(image, instance)| (image.view(), *instance))
            .collect();
        self.renderer.render_image_set(target, &draws);
        run.clear();
    }
}
