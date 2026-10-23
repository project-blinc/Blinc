//! Dispatch static, glass and layered content from prepared frame streams.
use super::{
    GpuPrimitive, GpuRenderer, ImageElement, PrimitiveBatch, RenderContext, RenderLayer,
    RenderTree, SvgElement, TextElement, cached_text, generate_text_decoration_primitives_by_layer,
};

pub(super) struct FrameContent<'a> {
    pub(super) batch: &'a PrimitiveBatch,
    pub(super) texts: &'a [TextElement],
    pub(super) images: &'a [ImageElement],
    pub(super) svgs: &'a [SvgElement],
    pub(super) glyphs_by_layer: &'a std::collections::BTreeMap<u32, cached_text::TextBatch>,
    pub(super) fg_glyphs: &'a cached_text::TextBatch,
    pub(super) fg_decorations_by_layer: &'a std::collections::HashMap<u32, Vec<GpuPrimitive>>,
}

impl RenderContext {
    pub(super) fn render_frame_content(
        &mut self,
        tree: &RenderTree,
        target: &wgpu::TextureView,
        viewport: (u32, u32),
        scale_factor: f32,
        content: FrameContent<'_>,
    ) {
        let (width, height) = viewport;
        let FrameContent {
            batch,
            texts,
            images,
            svgs,
            glyphs_by_layer,
            fg_glyphs,
            fg_decorations_by_layer,
        } = content;
        let has_glass = batch.glass_count() > 0;
        let has_layer_effects_in_batch = batch.has_layer_effects();
        let retained_group = tree.had_retained_canvas_painted()
            || tree
                .composite_bindings()
                .values()
                .any(|meta| !meta.dynamic_batch && !meta.primitive_range.is_empty());
        let clear = [0.0, 0.0, 0.0, self.clear_alpha as f64];
        let render_background = |renderer: &mut GpuRenderer, batch: &PrimitiveBatch| {
            if retained_group && batch.requires_ordered_rendering() {
                // Retained content can nest groups in the static owner. Replay
                // them together through the ordered single-sample compositor.
                renderer.clear_target(
                    target,
                    wgpu::Color {
                        r: clear[0],
                        g: clear[1],
                        b: clear[2],
                        a: clear[3],
                    },
                );
                renderer.render_overlay_msaa(target, batch, 1);
            } else {
                renderer.render_with_clear(target, batch, clear);
            }
        };

        // Only allocate glass textures when glass is actually used
        if has_glass {
            self.ensure_glass_textures(width, height);
        }
        let use_msaa_overlay = self.sample_count > 1;

        if has_glass {
            // Glass path with layer effects support
            let (bg_images, fg_images): (Vec<_>, Vec<_>) = images
                .iter()
                .partition(|img| img.layer == RenderLayer::Background);

            // Pre-render background images to both backdrop and target so glass can blur them
            let has_bg_images = !bg_images.is_empty();
            if has_bg_images {
                let backdrop_tex = self.backdrop_texture.take().unwrap();
                self.renderer
                    .clear_target(&backdrop_tex.view, wgpu::Color::TRANSPARENT);
                self.renderer.clear_target(
                    target,
                    wgpu::Color {
                        r: 0.0,
                        g: 0.0,
                        b: 0.0,
                        a: self.clear_alpha as f64,
                    },
                );
                self.render_images_ref(&backdrop_tex.view, &bg_images);
                self.render_images_ref(target, &bg_images);
                self.backdrop_texture = Some(backdrop_tex);
            }

            if has_layer_effects_in_batch {
                // Render the backdrop, grouped content, images and glass in composition
                // order.
                {
                    let backdrop = self.backdrop_texture.as_ref().unwrap();
                    self.renderer.render_to_backdrop(
                        &backdrop.view,
                        (backdrop.width, backdrop.height),
                        batch,
                        has_bg_images,
                    );
                }

                // Then use render_with_clear which handles layer effects
                render_background(&mut self.renderer, batch);

                // Render dynamic images (video frames from draw_rgba_pixels)
                if !batch.dynamic_images.is_empty() {
                    self.renderer
                        .render_unordered_dynamic_images(target, &batch.dynamic_images);
                }

                // Render background images to target after clear (so they're visible behind glass)
                if has_bg_images {
                    self.render_images_ref(target, &bg_images);
                }

                // Finally render glass primitives on top
                if batch.glass_count() > 0 {
                    let backdrop = self.backdrop_texture.as_ref().unwrap();
                    self.renderer.render_glass(target, &backdrop.view, batch);
                }
            } else {
                // No layer effects, use optimized glass frame rendering
                let backdrop = self.backdrop_texture.as_ref().unwrap();
                self.renderer.render_glass_frame(
                    target,
                    &backdrop.view,
                    (backdrop.width, backdrop.height),
                    batch,
                    has_bg_images,
                );
            }

            // Render paths. MSAA when sample_count > 1; non-MSAA
            // dispatch when not so 1×-sample targets (web) still emit
            // fill_path / stroke_path content.
            if batch.has_paths() && !batch.has_ordered_draws() {
                if use_msaa_overlay {
                    self.renderer
                        .render_paths_overlay_msaa(target, batch, self.sample_count);
                } else {
                    self.renderer.render_paths_overlay(target, batch);
                }
            }

            // Render remaining bg images (only if not already pre-rendered for glass)
            if !has_bg_images {
                self.render_images_ref(target, &bg_images);
            }
            self.render_images_ref(target, &fg_images);

            if has_layer_effects_in_batch {
                self.renderer.update_aux_data_for_batch(batch);
                self.renderer.render_foreground(target, batch);
            }

            // Interleaved z-layer rendering for proper text z-ordering in glass path
            let max_z = batch.max_z_layer();
            let max_text_z = glyphs_by_layer.keys().cloned().max().unwrap_or(0);
            let decorations_by_layer = generate_text_decoration_primitives_by_layer(texts);
            let max_decoration_z = decorations_by_layer.keys().cloned().max().unwrap_or(0);
            let max_glass_layer = max_z.max(max_text_z).max(max_decoration_z);

            // Render z=0 text first (before any z>0 primitives)
            if let Some(glyphs) = glyphs_by_layer.get(&0) {
                self.render_text(target, glyphs);
            }
            self.render_text_decorations_for_layer(target, &decorations_by_layer, 0);

            if max_glass_layer > 0 {
                let effect_indices = if batch.has_ordered_draws() {
                    (0..batch.primitives.len()).collect()
                } else {
                    batch.effect_layer_indices()
                };
                for z in 1..=max_glass_layer {
                    // Render primitives for this layer
                    let layer_primitives = if effect_indices.is_empty() {
                        batch.primitives_for_layer(z)
                    } else {
                        batch.primitives_for_layer_excluding_effects(z, &effect_indices)
                    };
                    if !layer_primitives.is_empty() {
                        self.renderer.update_aux_data_for_batch(batch);
                        self.renderer
                            .render_primitives_overlay(target, &layer_primitives);
                    }

                    // Render text for this layer (interleaved for proper z-order)
                    if let Some(glyphs) = glyphs_by_layer.get(&z) {
                        self.render_text(target, glyphs);
                    }
                    self.render_text_decorations_for_layer(target, &decorations_by_layer, z);
                }
            }

            // Render SVGs as rasterized images for high-quality anti-aliasing
            if !svgs.is_empty() {
                self.render_rasterized_svgs(target, svgs, scale_factor);
            }

            // Render foreground text (inside foreground-layer elements, after everything else)
            if !fg_glyphs.is_empty() {
                self.render_text(target, fg_glyphs);
            }
            // Render foreground text decorations (strikethrough / underline)
            // for every z-layer present in the foreground decoration index.
            for &z in fg_decorations_by_layer.keys() {
                self.render_text_decorations_for_layer(target, fg_decorations_by_layer, z);
            }
        } else {
            // Simple path (no glass)
            // Pre-generate text decorations grouped by layer for interleaved rendering
            let decorations_by_layer = generate_text_decoration_primitives_by_layer(texts);

            let max_z = batch.max_z_layer();
            let max_text_z = glyphs_by_layer.keys().cloned().max().unwrap_or(0);
            let max_decoration_z = decorations_by_layer.keys().cloned().max().unwrap_or(0);
            let max_layer = max_z.max(max_text_z).max(max_decoration_z);
            let has_layer_effects = batch.requires_ordered_rendering();

            if max_layer > 0 && !has_layer_effects {
                // Interleaved z-layer rendering for proper Stack z-ordering
                // Group images by z_index for interleaved rendering
                let mut images_by_layer: std::collections::BTreeMap<u32, Vec<&ImageElement>> =
                    std::collections::BTreeMap::new();
                for img in images {
                    images_by_layer.entry(img.z_index).or_default().push(img);
                }
                let max_image_z = images_by_layer.keys().cloned().max().unwrap_or(0);
                let max_layer = max_layer.max(max_image_z);

                // First pass: render z_layer=0 primitives with clear
                let z0_primitives = batch.primitives_for_layer(0);
                // Create a temporary batch for z=0 (include paths - they don't have z-layer support)
                let mut z0_batch = PrimitiveBatch::new();
                z0_batch.primitives = z0_primitives;
                z0_batch.paths = batch.paths.clone();
                // Preserve the shared auxiliary rows for primitive overlays and the
                // foreground streams drawn after glass.
                z0_batch.aux_data = batch.aux_data.clone();
                self.renderer.render_with_clear(
                    target,
                    &z0_batch,
                    [0.0, 0.0, 0.0, self.clear_alpha as f64],
                );

                // Render dynamic images (video frames)
                if !batch.dynamic_images.is_empty() {
                    self.renderer
                        .render_unordered_dynamic_images(target, &batch.dynamic_images);
                }

                // Render paths. MSAA/non-MSAA split so 1×-sample
                // targets still dispatch fill_path / stroke_path.
                if z0_batch.has_paths() {
                    if use_msaa_overlay {
                        self.renderer.render_paths_overlay_msaa(
                            target,
                            &z0_batch,
                            self.sample_count,
                        );
                    } else {
                        self.renderer.render_paths_overlay(target, &z0_batch);
                    }
                }

                // Render z=0 images
                if let Some(z0_images) = images_by_layer.get(&0) {
                    self.render_images_ref(target, z0_images);
                }

                // Render z=0 text (must render before z=1 primitives for proper z-ordering)
                if let Some(glyphs) = glyphs_by_layer.get(&0) {
                    if !glyphs.is_empty() {
                        self.render_text(target, glyphs);
                    }
                }
                self.render_text_decorations_for_layer(target, &decorations_by_layer, 0);

                // Render subsequent layers interleaved (primitives, images, text per layer)
                for z in 1..=max_layer {
                    // Draw this layer's primitives with the configured sample count.
                    let layer_primitives = batch.primitives_for_layer(z);
                    if !layer_primitives.is_empty() {
                        self.renderer.update_aux_data_for_batch(batch);
                        if use_msaa_overlay {
                            self.renderer.render_primitives_overlay_msaa(
                                target,
                                &layer_primitives,
                                self.sample_count,
                            );
                        } else {
                            self.renderer
                                .render_primitives_overlay(target, &layer_primitives);
                        }
                    }

                    // Render images for this layer
                    if let Some(layer_images) = images_by_layer.get(&z) {
                        self.render_images_ref(target, layer_images);
                    }

                    // Render text for this layer (interleaved with primitives for proper z-order)
                    if let Some(glyphs) = glyphs_by_layer.get(&z) {
                        if !glyphs.is_empty() {
                            self.render_text(target, glyphs);
                        }
                    }
                    self.render_text_decorations_for_layer(target, &decorations_by_layer, z);
                }

                // Render SVGs as rasterized images for high-quality anti-aliasing
                if !svgs.is_empty() {
                    self.render_rasterized_svgs(target, svgs, scale_factor);
                }

                // Render foreground primitives (e.g. borders on top)
                if batch.has_foreground_content() {
                    self.renderer.render_foreground(target, batch);
                }

                // Render foreground text (inside foreground-layer elements, after foreground primitives)
                if !fg_glyphs.is_empty() {
                    self.render_text(target, fg_glyphs);
                }
                for &z in fg_decorations_by_layer.keys() {
                    self.render_text_decorations_for_layer(target, fg_decorations_by_layer, z);
                }
            } else {
                // Dispatch the complete batch so group commands and child ranges remain
                // together.
                render_background(&mut self.renderer, batch);

                // Render dynamic images (video frames)
                if !batch.dynamic_images.is_empty() {
                    self.renderer
                        .render_unordered_dynamic_images(target, &batch.dynamic_images);
                }

                // Dispatch unordered paths separately with the configured sample count.
                if batch.has_paths() && !batch.has_ordered_draws() {
                    if self.sample_count > 1 {
                        self.renderer
                            .render_paths_overlay_msaa(target, batch, self.sample_count);
                    } else {
                        self.renderer.render_paths_overlay(target, batch);
                    }
                }

                self.render_images(target, images, width as f32, height as f32, scale_factor);

                // Render foreground primitives (e.g. borders on top)
                if batch.has_foreground_content() {
                    self.renderer.render_foreground(target, batch);
                }

                // Render SVGs as rasterized images for high-quality anti-aliasing
                if !svgs.is_empty() {
                    self.render_rasterized_svgs(target, svgs, scale_factor);
                }

                // Interleaved z-layer rendering for proper text z-ordering
                // Render z=0 text before any z>0 primitive overlays
                if let Some(glyphs) = glyphs_by_layer.get(&0) {
                    if !glyphs.is_empty() {
                        self.render_text(target, glyphs);
                    }
                }
                self.render_text_decorations_for_layer(target, &decorations_by_layer, 0);

                if max_layer > 0 {
                    let effect_indices = if batch.has_ordered_draws() {
                        (0..batch.primitives.len()).collect()
                    } else {
                        batch.effect_layer_indices()
                    };
                    for z in 1..=max_layer {
                        // Render primitives for this z-layer
                        let layer_primitives = if effect_indices.is_empty() {
                            batch.primitives_for_layer(z)
                        } else {
                            batch.primitives_for_layer_excluding_effects(z, &effect_indices)
                        };
                        if !layer_primitives.is_empty() {
                            self.renderer.update_aux_data_for_batch(batch);
                            self.renderer
                                .render_primitives_overlay(target, &layer_primitives);
                        }

                        // Render text for this z-layer (interleaved for proper z-order)
                        if let Some(glyphs) = glyphs_by_layer.get(&z) {
                            if !glyphs.is_empty() {
                                self.render_text(target, glyphs);
                            }
                        }
                        self.render_text_decorations_for_layer(target, &decorations_by_layer, z);
                    }
                }

                // Render foreground text (inside foreground-layer elements, after all z-layers)
                if !fg_glyphs.is_empty() {
                    self.render_text(target, fg_glyphs);
                }
                for &z in fg_decorations_by_layer.keys() {
                    self.render_text_decorations_for_layer(target, fg_decorations_by_layer, z);
                }
            }
        }
    }
}
