//! Layer effects: blur, shadow, colour matrix, drop shadow, glow and mask image, and applying a layer's effect list.

use super::{GpuRenderer, LayerTexture};
use crate::primitives::{
    BlurUniforms, ColorMatrixUniforms, DropShadowUniforms, GlowUniforms, MaskImageUniforms,
    Uniforms,
};
use wgpu::util::DeviceExt;

impl GpuRenderer {
    /// Apply a single Kawase blur pass
    ///
    /// Renders from `input` to `output` using the blur shader with the specified
    /// radius and iteration index.
    ///
    /// `blur_alpha`: if true, blurs both RGB and alpha (for soft shadow edges);
    ///               if false, preserves alpha while blurring RGB (for element blur)
    /// Apply multi-pass Kawase blur, batched into a single GPU submission.
    ///
    /// Uses ping-pong rendering between two textures. All passes share one
    /// command encoder for minimal GPU synchronization overhead.
    ///
    /// `blur_alpha`: if true, blurs both RGB and alpha (for soft shadow edges);
    ///               if false, preserves alpha while blurring RGB (for element blur)
    ///
    /// Returns the final output texture (caller should release temp textures).
    pub fn apply_blur_with_alpha(
        &mut self,
        input: &LayerTexture,
        radius: f32,
        passes: u32,
        blur_alpha: bool,
    ) -> LayerTexture {
        self.ensure_blur_pipeline();

        if passes == 0 {
            // No blur needed, return a copy
            let output = self
                .layer_texture_cache
                .acquire_exact(&self.device, input.size, false);
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Blur Copy Encoder"),
                });
            encoder.copy_texture_to_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &input.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyTextureInfo {
                    texture: &output.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::Extent3d {
                    width: input.size.0,
                    height: input.size.1,
                    depth_or_array_layers: 1,
                },
            );
            self.queue.submit(std::iter::once(encoder.finish()));
            return output;
        }

        let size = input.size;
        let blur_alpha_u32: u32 = if blur_alpha { 1 } else { 0 };

        // Write per-pass uniforms to pre-allocated buffer pool (no allocation)
        let blur_pool = self.buffers.blur_uniforms_pool.as_ref().unwrap();
        for i in 0..passes {
            self.queue.write_buffer(
                &blur_pool[i as usize],
                0,
                bytemuck::bytes_of(&BlurUniforms {
                    texel_size: [1.0 / size.0 as f32, 1.0 / size.1 as f32],
                    radius,
                    iteration: i,
                    blur_alpha: blur_alpha_u32,
                    _pad1: 0.0,
                    _pad2: 0.0,
                    _pad3: 0.0,
                }),
            );
        }

        // For ping-pong we need two temp textures
        let temp_a = self
            .layer_texture_cache
            .acquire_exact(&self.device, size, false);
        let temp_b = self
            .layer_texture_cache
            .acquire_exact(&self.device, size, false);

        // Pre-create bind groups: pass 0 reads input, subsequent passes alternate temp_a/temp_b
        let bind_groups: Vec<wgpu::BindGroup> = (0..passes)
            .map(|i| {
                let input_view = if i == 0 {
                    &input.view
                } else if i % 2 == 1 {
                    &temp_a.view
                } else {
                    &temp_b.view
                };
                let blur_pool = self.buffers.blur_uniforms_pool.as_ref().unwrap();
                self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Blur Effect Bind Group"),
                    layout: &self.bind_group_layouts.blur,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: blur_pool[i as usize].as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(input_view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::Sampler(&self.path_image_sampler),
                        },
                    ],
                })
            })
            .collect();

        // Single command encoder for all passes
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Blur Multi-Pass Encoder"),
            });

        for i in 0..passes {
            let output_view = if i % 2 == 0 {
                &temp_a.view
            } else {
                &temp_b.view
            };

            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Blur Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: output_view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(self.effect_pipelines.blur.as_ref().unwrap());
            render_pass.set_bind_group(0, &bind_groups[i as usize], &[]);
            render_pass.draw(0..6, 0..1);
        }

        // Single GPU submission for all blur passes
        self.queue.submit(std::iter::once(encoder.finish()));

        // Determine which texture has the final blurred result
        let (result, unused) = if passes % 2 == 1 {
            (temp_a, temp_b)
        } else {
            (temp_b, temp_a)
        };
        self.layer_texture_cache.release(unused);

        result
    }

    /// Apply multi-pass Kawase blur (CSS filter blur)
    ///
    /// Blurs both RGB and alpha channels, producing soft edges.
    pub fn apply_blur(&mut self, input: &LayerTexture, radius: f32, passes: u32) -> LayerTexture {
        self.apply_blur_with_alpha(input, radius, passes, false)
    }

    /// Apply multi-pass Kawase blur (shadow blur - blurs alpha for soft edges)
    ///
    /// Used for drop shadow and glow effects where we need soft alpha falloff.
    pub fn apply_shadow_blur(
        &mut self,
        input: &LayerTexture,
        radius: f32,
        passes: u32,
    ) -> LayerTexture {
        self.apply_blur_with_alpha(input, radius, passes, true)
    }

    /// Apply color matrix transformation
    ///
    /// Transforms colors using a 4x5 matrix (4x4 matrix + offset column).
    /// Useful for grayscale, sepia, saturation, brightness, contrast, etc.
    pub fn apply_color_matrix(
        &mut self,
        input: &wgpu::TextureView,
        output: &wgpu::TextureView,
        matrix: &[f32; 20],
    ) {
        self.ensure_color_matrix_pipeline();

        let uniforms = ColorMatrixUniforms::from_matrix(matrix);

        // Use cached buffer instead of creating per-pass
        let cm_buf = self.buffers.color_matrix_uniforms.as_ref().unwrap();
        self.queue
            .write_buffer(cm_buf, 0, bytemuck::bytes_of(&uniforms));

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Color Matrix Effect Bind Group"),
            layout: &self.bind_group_layouts.color_matrix,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: cm_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(input),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.path_image_sampler),
                },
            ],
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Color Matrix Pass Encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Color Matrix Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: output,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(self.effect_pipelines.color_matrix.as_ref().unwrap());
            render_pass.set_bind_group(0, &bind_group, &[]);
            render_pass.draw(0..6, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Apply drop shadow effect
    ///
    /// Takes a pre-blurred texture (for shadow shape) and the original texture (for compositing).
    /// The blurred texture's alpha is used to create the shadow, which is then colored and
    /// composited behind the original content.
    #[allow(clippy::too_many_arguments)]
    pub fn apply_drop_shadow(
        &mut self,
        blurred_input: &wgpu::TextureView,
        original_input: &wgpu::TextureView,
        output: &wgpu::TextureView,
        size: (u32, u32),
        offset: (f32, f32),
        blur_radius: f32,
        spread: f32,
        color: [f32; 4],
    ) {
        self.ensure_drop_shadow_pipeline();

        let uniforms = DropShadowUniforms {
            offset: [offset.0, offset.1],
            blur_radius,
            spread,
            color,
            texel_size: [1.0 / size.0 as f32, 1.0 / size.1 as f32],
            _pad: [0.0, 0.0],
        };

        // Use cached buffer instead of creating per-pass
        let ds_buf = self.buffers.drop_shadow_uniforms.as_ref().unwrap();
        self.queue
            .write_buffer(ds_buf, 0, bytemuck::bytes_of(&uniforms));

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Drop Shadow Effect Bind Group"),
            layout: &self.bind_group_layouts.drop_shadow,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: ds_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(blurred_input),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.path_image_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(original_input),
                },
            ],
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Drop Shadow Pass Encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Drop Shadow Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: output,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(self.effect_pipelines.drop_shadow.as_ref().unwrap());
            render_pass.set_bind_group(0, &bind_group, &[]);
            render_pass.draw(0..6, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Apply glow effect to a texture
    ///
    /// Creates a radial glow around the shape by finding distance to nearest opaque pixels
    /// and applying a smooth falloff based on blur and range parameters.
    #[allow(clippy::too_many_arguments)]
    pub fn apply_glow(
        &mut self,
        input: &wgpu::TextureView,
        output: &wgpu::TextureView,
        size: (u32, u32),
        color: [f32; 4],
        blur: f32,
        range: f32,
        opacity: f32,
    ) {
        self.ensure_glow_pipeline();

        let uniforms = GlowUniforms {
            color,
            blur,
            range,
            opacity,
            _pad0: 0.0,
            texel_size: [1.0 / size.0 as f32, 1.0 / size.1 as f32],
            _pad1: [0.0, 0.0],
        };

        // Use cached buffer instead of creating per-pass
        let glow_buf = self.buffers.glow_uniforms.as_ref().unwrap();
        self.queue
            .write_buffer(glow_buf, 0, bytemuck::bytes_of(&uniforms));

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Glow Effect Bind Group"),
            layout: &self.bind_group_layouts.glow,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: glow_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(input),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.path_image_sampler),
                },
            ],
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Glow Pass Encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Glow Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: output,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(self.effect_pipelines.glow.as_ref().unwrap());
            render_pass.set_bind_group(0, &bind_group, &[]);
            render_pass.draw(0..6, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Helper to create common color matrices
    pub fn grayscale_matrix() -> [f32; 20] {
        // Luminance weights (ITU-R BT.709)
        let r = 0.2126;
        let g = 0.7152;
        let b = 0.0722;
        [
            r, g, b, 0.0, 0.0, r, g, b, 0.0, 0.0, r, g, b, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0,
        ]
    }

    /// Create sepia tone color matrix
    pub fn sepia_matrix() -> [f32; 20] {
        [
            0.393, 0.769, 0.189, 0.0, 0.0, 0.349, 0.686, 0.168, 0.0, 0.0, 0.272, 0.534, 0.131, 0.0,
            0.0, 0.0, 0.0, 0.0, 1.0, 0.0,
        ]
    }

    /// Create saturation adjustment matrix
    pub fn saturation_matrix(saturation: f32) -> [f32; 20] {
        let s = saturation;
        let r = 0.2126;
        let g = 0.7152;
        let b = 0.0722;
        let sr = (1.0 - s) * r;
        let sg = (1.0 - s) * g;
        let sb = (1.0 - s) * b;
        [
            sr + s,
            sg,
            sb,
            0.0,
            0.0,
            sr,
            sg + s,
            sb,
            0.0,
            0.0,
            sr,
            sg,
            sb + s,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
            0.0,
        ]
    }

    /// Create brightness adjustment matrix
    pub fn brightness_matrix(brightness: f32) -> [f32; 20] {
        let b = brightness - 1.0; // 0 = no change, positive = brighter
        [
            1.0, 0.0, 0.0, 0.0, b, 0.0, 1.0, 0.0, 0.0, b, 0.0, 0.0, 1.0, 0.0, b, 0.0, 0.0, 0.0,
            1.0, 0.0,
        ]
    }

    /// Create contrast adjustment matrix
    pub fn contrast_matrix(contrast: f32) -> [f32; 20] {
        let c = contrast;
        let t = (1.0 - c) / 2.0;
        [
            c, 0.0, 0.0, 0.0, t, 0.0, c, 0.0, 0.0, t, 0.0, 0.0, c, 0.0, t, 0.0, 0.0, 0.0, 1.0, 0.0,
        ]
    }

    /// Create invert color matrix
    pub fn invert_matrix() -> [f32; 20] {
        [
            -1.0, 0.0, 0.0, 0.0, 1.0, 0.0, -1.0, 0.0, 0.0, 1.0, 0.0, 0.0, -1.0, 0.0, 1.0, 0.0, 0.0,
            0.0, 1.0, 0.0,
        ]
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // Layer Command Processing
    // ─────────────────────────────────────────────────────────────────────────────

    /// Calculate how much layer effects extend beyond the original content bounds.
    ///
    /// Returns (left, top, right, bottom) expansion in pixels.
    /// Blur expands bounds so the soft-edge falloff has room to render.
    pub(super) fn calculate_effect_expansion(
        effects: &[blinc_core::LayerEffect],
    ) -> (f32, f32, f32, f32) {
        use blinc_core::LayerEffect;

        let mut left = 0.0f32;
        let mut top = 0.0f32;
        let mut right = 0.0f32;
        let mut bottom = 0.0f32;

        for effect in effects {
            match effect {
                LayerEffect::Blur { radius, .. } => {
                    // Blur softens edges, which extends beyond original bounds.
                    // ~2x radius covers the visible falloff of Kawase blur.
                    let expand = radius * 2.0;
                    left = left.max(expand);
                    top = top.max(expand);
                    right = right.max(expand);
                    bottom = bottom.max(expand);
                }
                LayerEffect::DropShadow {
                    offset_x,
                    offset_y,
                    blur,
                    spread,
                    ..
                } => {
                    // Shadow expands based on blur, spread, and offset
                    let blur_expand = blur * 2.0; // 2x blur radius is enough
                    let spread_expand = spread.max(0.0);
                    let total_expand = blur_expand + spread_expand;

                    // Left/top expansion: when offset is negative, shadow goes that direction
                    left = left.max(total_expand + (-offset_x).max(0.0));
                    top = top.max(total_expand + (-offset_y).max(0.0));
                    // Right/bottom expansion: when offset is positive, shadow goes that direction
                    right = right.max(total_expand + offset_x.max(0.0));
                    bottom = bottom.max(total_expand + offset_y.max(0.0));
                }
                LayerEffect::Glow { blur, range, .. } => {
                    // Glow expands equally in all directions
                    let expand = (blur + range) * 2.0; // Account for range
                    left = left.max(expand);
                    top = top.max(expand);
                    right = right.max(expand);
                    bottom = bottom.max(expand);
                }
                LayerEffect::ColorMatrix { .. } | LayerEffect::MaskImage { .. } => {
                    // These don't expand bounds
                }
            }
        }

        (left, top, right, bottom)
    }

    /// No-op: mask images must be pre-loaded via `load_mask_image_rgba()`.
    pub(super) fn load_mask_image(&mut self, _url: &str) {
        // Mask images are loaded externally (in blinc_app context) and cached
        // via load_mask_image_rgba() before the render pass begins.
    }

    /// Pre-load a mask image from RGBA pixel data.
    /// Call this before rendering to ensure mask textures are available.
    pub fn load_mask_image_rgba(&mut self, url: &str, pixels: &[u8], width: u32, height: u32) {
        if self.mask_image_cache.contains_key(url) {
            return;
        }
        // Mask images are loaded once and sampled every frame the
        // masked element is visible — a textbook case for BC. The
        // auto encoder picks BC1 when the mask is effectively
        // opaque (rare for masks but cheap to check) and BC3
        // otherwise to preserve the alpha channel the mask depends
        // on. Falls back to uncompressed upload on devices without
        // BC support or in builds without the `bc-encode` feature.
        //
        // The 256-px floor matches the 2D image cache's
        // `bc_eligible` heuristic: BC's 4×4 block quantization
        // puts visible banding into small alpha ramps, which is
        // exactly the signal a mask carries. Large masks
        // (full-viewport gradient overlays, photo-style alpha
        // cutouts) still compress.
        // Same alignment + size floor as the `bc_eligible` helper in
        // blinc_app: BC formats need multiple-of-4 dimensions (wgpu
        // validation), and sub-256 masks produce visible block
        // banding in alpha ramps.
        let bc_ok = self.has_texture_compression_bc
            && width.is_multiple_of(4)
            && height.is_multiple_of(4)
            && width >= 256
            && height >= 256;
        let label = format!("mask:{}", url);
        let gpu_img = crate::image::GpuImage::from_rgba_maybe_compressed(
            &self.device,
            &self.queue,
            pixels,
            width,
            height,
            false,
            bc_ok,
            Some(&label),
        );
        self.mask_image_cache.insert(url.to_string(), gpu_img);
    }

    /// Check if a mask image is already loaded in cache
    pub fn has_mask_image(&self, url: &str) -> bool {
        self.mask_image_cache.contains_key(url)
    }

    /// Apply mask image effect: multiplies element alpha by mask value
    pub(super) fn apply_mask_image_effect(
        &mut self,
        input: &wgpu::TextureView,
        output: &wgpu::TextureView,
        image_url: &str,
        mask_mode: u32,
    ) {
        self.ensure_mask_image_pipeline();
        let mask_img = match self.mask_image_cache.get(image_url) {
            Some(img) => img,
            None => return,
        };

        let uniforms = MaskImageUniforms {
            mask_mode,
            _pad: [0.0; 7],
        };

        let uniform_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Mask Image Uniforms"),
                contents: bytemuck::bytes_of(&uniforms),
                usage: wgpu::BufferUsages::UNIFORM,
            });

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Mask Image Effect Bind Group"),
            layout: &self.bind_group_layouts.mask_image,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(input),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.path_image_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(mask_img.view()),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&self.path_image_sampler),
                },
            ],
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Mask Image Pass Encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Mask Image Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: output,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(self.effect_pipelines.mask_image.as_ref().unwrap());
            render_pass.set_bind_group(0, &bind_group, &[]);
            render_pass.draw(0..6, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Apply layer effects to a texture
    ///
    /// Processes a list of LayerEffects in order and returns the final result.
    /// The input texture is not modified; a new texture with effects applied is returned.
    pub fn apply_layer_effects(
        &mut self,
        input: &LayerTexture,
        effects: &[blinc_core::LayerEffect],
    ) -> LayerTexture {
        use blinc_core::LayerEffect;

        if effects.is_empty() {
            // No effects, just return a copy
            let output = self
                .layer_texture_cache
                .acquire_exact(&self.device, input.size, false);
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Layer Effect Copy Encoder"),
                });
            encoder.copy_texture_to_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &input.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyTextureInfo {
                    texture: &output.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::Extent3d {
                    width: input.size.0,
                    height: input.size.1,
                    depth_or_array_layers: 1,
                },
            );
            self.queue.submit(std::iter::once(encoder.finish()));
            return output;
        }

        let size = input.size;
        // Track ownership: effects that produce a new texture pass ownership here.
        // We avoid a redundant copy by using the input directly for the first effect
        // and only copying when a non-blur effect needs a mutable working texture.
        let mut current: Option<LayerTexture> = None;

        for effect in effects {
            // Get the current working texture or the original input
            let working = current.as_ref().unwrap_or(input);

            match effect {
                LayerEffect::Blur { radius, quality: _ } => {
                    // Blur reads from working and produces a new texture (no copy needed)
                    let passes = ((*radius / 2.0).ceil().max(2.0) as u32).min(8);
                    let blurred = self.apply_blur(working, *radius, passes);
                    if let Some(prev) = current.take() {
                        self.layer_texture_cache.release(prev);
                    }
                    current = Some(blurred);
                }

                LayerEffect::DropShadow {
                    offset_x,
                    offset_y,
                    blur,
                    spread,
                    color,
                } => {
                    let temp = self
                        .layer_texture_cache
                        .acquire_exact(&self.device, size, false);
                    self.apply_drop_shadow(
                        &working.view,
                        &working.view,
                        &temp.view,
                        size,
                        (*offset_x, *offset_y),
                        *blur,
                        *spread,
                        [color.r, color.g, color.b, color.a],
                    );
                    if let Some(prev) = current.take() {
                        self.layer_texture_cache.release(prev);
                    }
                    current = Some(temp);
                }

                LayerEffect::Glow {
                    color,
                    blur,
                    range,
                    opacity,
                } => {
                    let temp = self
                        .layer_texture_cache
                        .acquire_exact(&self.device, size, false);
                    self.apply_glow(
                        &working.view,
                        &temp.view,
                        size,
                        [color.r, color.g, color.b, color.a],
                        *blur,
                        *range,
                        *opacity,
                    );
                    if let Some(prev) = current.take() {
                        self.layer_texture_cache.release(prev);
                    }
                    current = Some(temp);
                }

                LayerEffect::ColorMatrix { matrix } => {
                    let temp = self
                        .layer_texture_cache
                        .acquire_exact(&self.device, size, false);
                    self.apply_color_matrix(&working.view, &temp.view, matrix);
                    if let Some(prev) = current.take() {
                        self.layer_texture_cache.release(prev);
                    }
                    current = Some(temp);
                }

                LayerEffect::MaskImage {
                    image_url,
                    mask_mode,
                } => {
                    // Load mask image if not cached
                    self.load_mask_image(image_url);
                    // Apply mask if the texture was loaded successfully
                    if self.mask_image_cache.contains_key(image_url.as_str()) {
                        let temp =
                            self.layer_texture_cache
                                .acquire_exact(&self.device, size, false);
                        let mode_val = match mask_mode {
                            blinc_core::MaskMode::Alpha => 0u32,
                            blinc_core::MaskMode::Luminance => 1u32,
                        };
                        self.apply_mask_image_effect(
                            &working.view,
                            &temp.view,
                            image_url,
                            mode_val,
                        );
                        if let Some(prev) = current.take() {
                            self.layer_texture_cache.release(prev);
                        }
                        current = Some(temp);
                    }
                }
            }
        }

        // If no effect produced a new texture (shouldn't happen since effects is non-empty),
        // fall back to a copy
        current.unwrap_or_else(|| {
            let output = self
                .layer_texture_cache
                .acquire_exact(&self.device, input.size, false);
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Layer Effect Fallback Copy"),
                });
            encoder.copy_texture_to_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &input.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyTextureInfo {
                    texture: &output.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::Extent3d {
                    width: input.size.0,
                    height: input.size.1,
                    depth_or_array_layers: 1,
                },
            );
            self.queue.submit(std::iter::once(encoder.finish()));
            output
        })
    }
}
