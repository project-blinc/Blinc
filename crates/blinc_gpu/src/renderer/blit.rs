//! Blitting textures and regions to a target, with and without a clip.

use super::GpuRenderer;
use crate::primitives::Uniforms;
use crate::shaders::LAYER_COMPOSITE_SHADER;
use wgpu::util::DeviceExt;

impl GpuRenderer {
    /// Blit a tight texture to the target at the correct position
    #[allow(clippy::too_many_arguments)]
    pub fn blit_tight_texture_to_target(
        &mut self,
        source: &wgpu::TextureView,
        source_size: (u32, u32),
        target: &wgpu::TextureView,
        dest_pos: (f32, f32),
        dest_size: (f32, f32),
        opacity: f32,
        blend_mode: blinc_core::BlendMode,
        clip: Option<([f32; 4], [f32; 4])>, // (clip_bounds, clip_radius)
        transform_3d: Option<blinc_core::Transform3DParams>,
        rotate_z_rad: f32,
    ) {
        use crate::primitives::LayerCompositeUniforms;

        let vp_w = self.viewport_size.0 as f32;
        let vp_h = self.viewport_size.1 as f32;

        // For 3D perspective transforms, compute the expanded bounding box of the
        // perspective-distorted quad corners so the scissor rect is large enough.
        let (effective_dest_pos, effective_dest_size) = if let Some(ref t3d) = transform_3d {
            let cx = dest_pos.0 + dest_size.0 * 0.5;
            let cy = dest_pos.1 + dest_size.1 * 0.5;
            let hw = dest_size.0 * 0.5;
            let hh = dest_size.1 * 0.5;
            // Project all 4 corners through perspective and find AABB
            let corners = [(-hw, -hh), (hw, -hh), (-hw, hh), (hw, hh)];
            let mut min_x = f32::MAX;
            let mut min_y = f32::MAX;
            let mut max_x = f32::MIN;
            let mut max_y = f32::MIN;
            for (lx, ly) in corners {
                // Rotate Y
                let ry_x = lx * t3d.cos_ry;
                let ry_z = lx * t3d.sin_ry;
                // Rotate X
                let rx_y = ly * t3d.cos_rx - ry_z * t3d.sin_rx;
                let rx_z = ly * t3d.sin_rx + ry_z * t3d.cos_rx;
                // Perspective
                let w = (t3d.perspective_d + rx_z) / t3d.perspective_d;
                let sx = cx + ry_x / w;
                let sy = cy + rx_y / w;
                min_x = min_x.min(sx);
                min_y = min_y.min(sy);
                max_x = max_x.max(sx);
                max_y = max_y.max(sy);
            }
            ((min_x, min_y), (max_x - min_x, max_y - min_y))
        } else {
            (dest_pos, dest_size)
        };

        // Calculate the visible region by intersecting dest rect with viewport and clip bounds
        // Start with destination rect (possibly expanded for 3D)
        let mut vis_x0 = effective_dest_pos.0;
        let mut vis_y0 = effective_dest_pos.1;
        let mut vis_x1 = effective_dest_pos.0 + effective_dest_size.0;
        let mut vis_y1 = effective_dest_pos.1 + effective_dest_size.1;

        // Intersect with viewport
        vis_x0 = vis_x0.max(0.0);
        vis_y0 = vis_y0.max(0.0);
        vis_x1 = vis_x1.min(vp_w);
        vis_y1 = vis_y1.min(vp_h);

        // Intersect with clip bounds if provided
        let (clip_bounds, clip_radius, clip_type) = match clip {
            Some((bounds, radius)) => {
                // Intersect with clip bounds
                vis_x0 = vis_x0.max(bounds[0]);
                vis_y0 = vis_y0.max(bounds[1]);
                vis_x1 = vis_x1.min(bounds[0] + bounds[2]);
                vis_y1 = vis_y1.min(bounds[1] + bounds[3]);
                (bounds, radius, 1)
            }
            None => ([0.0, 0.0, vp_w, vp_h], [0.0; 4], 0),
        };

        // Check if anything is visible
        let vis_w = vis_x1 - vis_x0;
        let vis_h = vis_y1 - vis_y0;
        if vis_w <= 0.0 || vis_h <= 0.0 {
            return; // Nothing visible, skip rendering
        }

        // For 3D perspective, the shader handles UV mapping via the full dest_rect/source_rect;
        // we just need the scissor to be large enough. Use the full source rect.
        let (source_rect, dest_rect) = if transform_3d.is_some() {
            // Full source rect, original dest rect (shader applies perspective)
            let src_total_w = dest_size.0 / source_size.0 as f32;
            let src_total_h = dest_size.1 / source_size.1 as f32;
            (
                [0.0, 0.0, src_total_w.min(1.0), src_total_h.min(1.0)],
                [dest_pos.0, dest_pos.1, dest_size.0, dest_size.1],
            )
        } else {
            // Calculate source rect based on what portion is visible
            // Map visible region back to source texture coordinates
            let src_total_w = dest_size.0 / source_size.0 as f32;
            let src_total_h = dest_size.1 / source_size.1 as f32;

            // Calculate what portion of the dest rect is visible
            let vis_offset_x = vis_x0 - dest_pos.0;
            let vis_offset_y = vis_y0 - dest_pos.1;

            // Map to source texture coordinates
            let src_x0 = (vis_offset_x / dest_size.0) * src_total_w;
            let src_y0 = (vis_offset_y / dest_size.1) * src_total_h;
            let src_w = (vis_w / dest_size.0) * src_total_w;
            let src_h = (vis_h / dest_size.1) * src_total_h;

            (
                [
                    src_x0.min(1.0),
                    src_y0.min(1.0),
                    src_w.min(1.0),
                    src_h.min(1.0),
                ],
                [vis_x0, vis_y0, vis_w, vis_h],
            )
        };

        let (perspective_d, sin_rx, cos_rx, sin_ry, cos_ry) = if let Some(ref t3d) = transform_3d {
            (
                t3d.perspective_d,
                t3d.sin_rx,
                t3d.cos_rx,
                t3d.sin_ry,
                t3d.cos_ry,
            )
        } else {
            (0.0, 0.0, 1.0, 0.0, 1.0)
        };
        // 2D in-plane rotation. The flat (non-perspective) path in
        // LAYER_COMPOSITE_SHADER rotates `local_pos - (0.5, 0.5)`
        // around the center, then maps into dest_rect — so motion-
        // bound subtrees that need to spin (cn::spinner's
        // rotate_timeline) can rotate their cached texture per-frame
        // without re-baking.
        let sin_rz = rotate_z_rad.sin();
        let cos_rz = rotate_z_rad.cos();

        let uniforms = LayerCompositeUniforms {
            source_rect,
            dest_rect,
            viewport_size: [vp_w, vp_h],
            opacity,
            blend_mode: blend_mode as u32,
            clip_bounds,
            clip_radius,
            clip_type,
            perspective_d,
            sin_rx,
            cos_rx,
            sin_ry,
            cos_ry,
            sin_rz,
            cos_rz,
        };

        let uniform_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Tight Blit Uniforms Buffer"),
                contents: bytemuck::cast_slice(&[uniforms]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });

        let is_blend = blend_mode != blinc_core::BlendMode::Normal;

        // For non-Normal blend modes, snapshot the target so the shader can sample dest
        let dest_snapshot = if is_blend {
            let target_texture = target.texture();
            if target_texture
                .usage()
                .contains(wgpu::TextureUsages::COPY_SRC)
            {
                let temp =
                    self.layer_texture_cache
                        .acquire_exact(&self.device, self.viewport_size, false);

                let mut copy_encoder =
                    self.device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                            label: Some("Tight Blit Blend Dest Copy"),
                        });
                copy_encoder.copy_texture_to_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: target_texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::TexelCopyTextureInfo {
                        texture: &temp.texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::Extent3d {
                        width: self.viewport_size.0,
                        height: self.viewport_size.1,
                        depth_or_array_layers: 1,
                    },
                );
                self.queue.submit(std::iter::once(copy_encoder.finish()));
                Some(temp)
            } else {
                None
            }
        } else {
            None
        };

        let bind_group = if let Some(ref snapshot) = dest_snapshot {
            self.create_layer_composite_bind_group_with_dest(
                &uniform_buffer,
                source,
                &self.path_image_sampler,
                &snapshot.view,
                &self.path_image_sampler,
            )
        } else {
            self.create_layer_composite_bind_group(
                &uniform_buffer,
                source,
                &self.path_image_sampler,
            )
        };

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Tight Blit Encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Tight Blit Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // Set scissor rect to the visible region (already intersected with clip bounds)
            let scissor_x = vis_x0.max(0.0) as u32;
            let scissor_y = vis_y0.max(0.0) as u32;
            let scissor_w = vis_w.max(1.0) as u32;
            let scissor_h = vis_h.max(1.0) as u32;

            // Phase 4d Opt 2: when a damage scissor is set,
            // intersect it with the layer's visible bounds. The
            // composite paints only in the overlap of (layer
            // content area) ∩ (damage rect). Pixels outside the
            // damage rect stay from the previous frame's render —
            // which is what makes a scissored cache repaint
            // correct for CSS-animated layer effects.
            //
            // Empty intersection ⇒ skip the draw entirely (layer
            // doesn't touch the damage region this frame). Still
            // submit the otherwise-empty render pass so the encoder
            // doesn't leak — `LoadOp::Load` + no draw is a no-op
            // on the GPU.
            let scissor_and_draw = if let Some((dx, dy, dw, dh)) = self.pending_damage_scissor {
                let lx0 = scissor_x;
                let ly0 = scissor_y;
                let lx1 = lx0 + scissor_w;
                let ly1 = ly0 + scissor_h;
                let dx1 = dx + dw;
                let dy1 = dy + dh;
                let ix0 = lx0.max(dx);
                let iy0 = ly0.max(dy);
                let ix1 = lx1.min(dx1);
                let iy1 = ly1.min(dy1);
                if ix1 <= ix0 || iy1 <= iy0 {
                    None
                } else {
                    Some((ix0, iy0, ix1 - ix0, iy1 - iy0))
                }
            } else {
                Some((scissor_x, scissor_y, scissor_w, scissor_h))
            };

            if let Some((sx, sy, sw, sh)) = scissor_and_draw {
                render_pass.set_scissor_rect(sx, sy, sw, sh);
                render_pass.set_pipeline(&self.pipelines.layer_composite);
                render_pass.set_bind_group(0, &bind_group, &[]);
                render_pass.draw(0..6, 0..1);
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));

        if let Some(snapshot) = dest_snapshot {
            self.layer_texture_cache.release(snapshot);
        }
    }

    /// Blit a texture to the target with blending
    ///
    /// For non-Normal blend modes, copies the target to a temp texture first
    /// so the shader can read the destination for blend computation.
    pub(super) fn blit_texture_to_target(
        &mut self,
        source: &wgpu::TextureView,
        target: &wgpu::TextureView,
        opacity: f32,
        blend_mode: blinc_core::BlendMode,
    ) {
        use crate::primitives::LayerCompositeUniforms;

        let is_blend = blend_mode != blinc_core::BlendMode::Normal;

        // For non-Normal blend modes, copy the target to a temp texture
        // so the shader can sample the destination
        let dest_snapshot = if is_blend {
            if let Some(target_ptr) = self.blend_target_ptr {
                // Safety: pointer is valid for the duration of the render frame
                let target_texture = unsafe { &*target_ptr };
                let temp =
                    self.layer_texture_cache
                        .acquire(&self.device, self.viewport_size, false);

                let mut copy_encoder =
                    self.device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                            label: Some("Blend Dest Copy Encoder"),
                        });
                copy_encoder.copy_texture_to_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: target_texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::TexelCopyTextureInfo {
                        texture: &temp.texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::Extent3d {
                        width: self.viewport_size.0,
                        height: self.viewport_size.1,
                        depth_or_array_layers: 1,
                    },
                );
                self.queue.submit(std::iter::once(copy_encoder.finish()));
                Some(temp)
            } else {
                // No target texture available — fall back to Normal blend
                None
            }
        } else {
            None
        };

        // Full viewport blit - source covers entire texture, dest covers entire viewport
        let vp_w = self.viewport_size.0 as f32;
        let vp_h = self.viewport_size.1 as f32;
        let effective_blend = if dest_snapshot.is_some() {
            blend_mode
        } else {
            blinc_core::BlendMode::Normal
        };
        let uniforms = LayerCompositeUniforms {
            source_rect: [0.0, 0.0, 1.0, 1.0], // Full texture (normalized)
            dest_rect: [0.0, 0.0, vp_w, vp_h],
            viewport_size: [vp_w, vp_h],
            opacity,
            blend_mode: effective_blend as u32,
            clip_bounds: [0.0, 0.0, vp_w, vp_h], // No clipping
            clip_radius: [0.0, 0.0, 0.0, 0.0],
            clip_type: 0,
            perspective_d: 0.0,
            sin_rx: 0.0,
            cos_rx: 1.0,
            sin_ry: 0.0,
            cos_ry: 1.0,
            sin_rz: 0.0,
            cos_rz: 1.0,
        };

        let uniform_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Blit Uniforms Buffer"),
                contents: bytemuck::cast_slice(&[uniforms]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });

        let bind_group = if let Some(ref snapshot) = dest_snapshot {
            self.create_layer_composite_bind_group_with_dest(
                &uniform_buffer,
                source,
                &self.path_image_sampler,
                &snapshot.view,
                &self.path_image_sampler,
            )
        } else {
            self.create_layer_composite_bind_group(
                &uniform_buffer,
                source,
                &self.path_image_sampler,
            )
        };

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Blit Encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Blit Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        // Load existing content - we're blending on top
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(&self.pipelines.layer_composite);
            render_pass.set_bind_group(0, &bind_group, &[]);
            render_pass.draw(0..6, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));

        // Release the dest snapshot texture
        if let Some(snapshot) = dest_snapshot {
            self.layer_texture_cache.release(snapshot);
        }
    }

    /// Blit a specific region from source texture to target at given position
    ///
    /// This is used for layer effects where we need to composite only the
    /// element's region back to the target at the correct position.
    pub(super) fn blit_region_to_target(
        &mut self,
        source: &wgpu::TextureView,
        target: &wgpu::TextureView,
        position: (f32, f32),
        size: (f32, f32),
        opacity: f32,
        blend_mode: blinc_core::BlendMode,
    ) {
        self.blit_region_to_target_with_clip(
            source, target, position, size, opacity, blend_mode, None,
        )
    }

    /// Blit a specific region with optional clip
    #[allow(clippy::too_many_arguments)]
    pub(super) fn blit_region_to_target_with_clip(
        &mut self,
        source: &wgpu::TextureView,
        target: &wgpu::TextureView,
        position: (f32, f32),
        size: (f32, f32),
        opacity: f32,
        blend_mode: blinc_core::BlendMode,
        clip: Option<([f32; 4], [f32; 4])>, // (bounds, radii)
    ) {
        use crate::primitives::LayerCompositeUniforms;

        let vp_w = self.viewport_size.0 as f32;
        let vp_h = self.viewport_size.1 as f32;

        // Source rect in normalized coordinates (0-1)
        // The source texture is viewport-sized, so we extract the element's region
        let source_rect = [
            position.0 / vp_w,
            position.1 / vp_h,
            size.0 / vp_w,
            size.1 / vp_h,
        ];

        // Dest rect in viewport pixel coordinates
        let dest_rect = [position.0, position.1, size.0, size.1];

        let mut uniforms = LayerCompositeUniforms {
            source_rect,
            dest_rect,
            viewport_size: [vp_w, vp_h],
            opacity,
            blend_mode: blend_mode as u32,
            clip_bounds: [0.0, 0.0, vp_w, vp_h],
            clip_radius: [0.0, 0.0, 0.0, 0.0],
            clip_type: 0,
            perspective_d: 0.0,
            sin_rx: 0.0,
            cos_rx: 1.0,
            sin_ry: 0.0,
            cos_ry: 1.0,
            sin_rz: 0.0,
            cos_rz: 1.0,
        };

        if let Some((bounds, radii)) = clip {
            uniforms.clip_bounds = bounds;
            uniforms.clip_radius = radii;
            uniforms.clip_type = 1;
        }

        let uniform_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Region Blit Uniforms Buffer"),
                contents: bytemuck::cast_slice(&[uniforms]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });

        let bind_group = self.create_layer_composite_bind_group(
            &uniform_buffer,
            source,
            &self.path_image_sampler,
        );

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Region Blit Encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Region Blit Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // Set scissor rect to only affect the element's region
            render_pass.set_scissor_rect(
                position.0.max(0.0) as u32,
                position.1.max(0.0) as u32,
                size.0.min(vp_w - position.0).max(1.0) as u32,
                size.1.min(vp_h - position.1).max(1.0) as u32,
            );

            render_pass.set_pipeline(&self.pipelines.layer_composite);
            render_pass.set_bind_group(0, &bind_group, &[]);
            render_pass.draw(0..6, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // SDF 3D Viewport Rendering
    // ─────────────────────────────────────────────────────────────────────────────
}
