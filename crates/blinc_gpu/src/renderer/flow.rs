//! Flow shader passes: pending flows, the scene copy they sample, and the cursor.

use super::GpuRenderer;

impl GpuRenderer {
    /// Get a mutable reference to the @flow pipeline cache
    pub fn flow_pipeline_cache(&mut self) -> &mut crate::flow_pipeline::FlowPipelineCache {
        &mut self.flow_pipeline_cache
    }

    /// Set the window cursor, in physical pixels, for flows that follow it.
    pub fn set_flow_cursor(&mut self, cursor: [f32; 2]) {
        self.flow_cursor = cursor;
    }

    /// Draw a flow from `DrawContext::draw_shader` into `target`, within `visible` (the part of
    /// its bounds the clip leaves).
    pub fn render_pending_flow(
        &mut self,
        target: &wgpu::TextureView,
        flow: &crate::paint::PendingFlow,
        visible: Option<[f32; 4]>,
    ) -> bool {
        if let Err(e) = self.flow_pipeline_cache.compile(&flow.shader) {
            tracing::warn!("flow '{}' compile error: {}", flow.shader.name, e);
            return false;
        }
        let [x, y, w, h] = flow.bounds;
        let (tw, th) = self.viewport_size;
        let uniforms = crate::flow_pipeline::FlowUniformData {
            viewport_size: [tw as f32, th as f32],
            time: flow
                .inputs
                .time
                .unwrap_or_else(crate::flow_pipeline::app_clock_secs),
            frame_index: 0.0,
            element_bounds: flow.bounds,
            pointer: flow.inputs.pointer.unwrap_or([
                (self.flow_cursor[0] - x) / w.max(1.0),
                (self.flow_cursor[1] - y) / h.max(1.0),
            ]),
            corner_radius: flow.corner_radius,
            _padding: 0.0,
        };
        let drawn = self.render_flow(
            target,
            &flow.shader.name,
            &uniforms,
            Some(flow.bounds),
            visible,
        );
        if !drawn {
            tracing::warn!("flow '{}' render failed", flow.shader.name);
        }
        drawn
    }

    /// Render a @flow fragment shader into a target texture.
    ///
    /// Compiles the flow on first use, updates uniforms, and draws a fullscreen quad.
    /// If `viewport` is Some([x, y, w, h]), the quad is scoped to that region in pixels.
    /// Returns false if the flow is not found or compilation failed.
    /// Ensure the scene copy texture exists, matches viewport size, and is up-to-date.
    ///
    /// Called once per frame before rendering any flows that use `sample_scene()`.
    /// Returns the scene texture view, or None if the copy failed.
    pub(super) fn ensure_scene_copy(&mut self) -> Option<&wgpu::TextureView> {
        self.scene_copy_last_frame = self.current_frame();
        let (tw, th) = self.viewport_size;
        if tw == 0 || th == 0 {
            return None;
        }

        // Recreate texture on viewport resize
        let needs_recreate = match &self.scene_copy_texture {
            Some((_, _, w, h)) => *w != tw || *h != th,
            None => true,
        };
        if needs_recreate {
            let tex = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Flow Scene Copy Texture"),
                size: wgpu::Extent3d {
                    width: tw,
                    height: th,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: self.texture_format,
                usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
            self.scene_copy_texture = Some((tex, view, tw, th));
            // Scene texture changed — invalidate bind groups that reference it
            self.flow_pipeline_cache.invalidate_scene_bind_groups();
        }

        // Copy current render target → scene copy texture (single copy per frame)
        if let Some((scene_tex, _, _, _)) = &self.scene_copy_texture
            && let Some(tex_ptr) = self.blend_target_ptr
        {
            let src_tex = unsafe { &*tex_ptr };
            let mut copy_encoder =
                self.device
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                        label: Some("Flow Scene Copy Encoder"),
                    });
            copy_encoder.copy_texture_to_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: src_tex,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyTextureInfo {
                    texture: scene_tex,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::Extent3d {
                    width: tw,
                    height: th,
                    depth_or_array_layers: 1,
                },
            );
            self.queue.submit(std::iter::once(copy_encoder.finish()));
        }

        self.scene_copy_texture.as_ref().map(|(_, v, _, _)| v)
    }

    pub fn render_flow(
        &mut self,
        target: &wgpu::TextureView,
        flow_name: &str,
        uniforms: &crate::flow_pipeline::FlowUniformData,
        viewport: Option<[f32; 4]>,
        clip: Option<[f32; 4]>,
    ) -> bool {
        // If this flow uses sample_scene(), ensure scene copy is ready
        let scene_view_owned: Option<*const wgpu::TextureView> =
            if self.flow_pipeline_cache.needs_scene_texture(flow_name) {
                self.ensure_scene_copy().map(|v| v as *const _)
            } else {
                None
            };
        // SAFETY: scene_copy_texture is owned by self and lives for the duration of this call
        let scene_view = scene_view_owned.map(|ptr| unsafe { &*ptr });

        if !self
            .flow_pipeline_cache
            .prepare_render(&self.queue, flow_name, uniforms, scene_view)
        {
            return false;
        }

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Flow Render Encoder"),
            });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Flow Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load, // Preserve existing content
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // The viewport is the element's whole bounds, so the shader's uv covers the element
            // even where part of it is off the target or clipped. The scissor is what remains
            // visible: the bounds within the target and within `clip`.
            let mut draw = true;
            if let Some([x, y, w, h]) = viewport {
                let (tw, th) = self.viewport_size;
                let (tw, th) = (tw as f32, th as f32);
                let (mut left, mut top, mut right, mut bottom) =
                    (x.max(0.0), y.max(0.0), (x + w).min(tw), (y + h).min(th));
                if let Some([cx, cy, cw, ch]) = clip {
                    left = left.max(cx);
                    top = top.max(cy);
                    right = right.min(cx + cw);
                    bottom = bottom.min(cy + ch);
                }
                if right - left >= 1.0 && bottom - top >= 1.0 {
                    let limit = self.device.limits().max_texture_dimension_2d as f32;
                    if w <= limit && h <= limit {
                        pass.set_viewport(x, y, w.max(1.0), h.max(1.0), 0.0, 1.0);
                    }
                    pass.set_scissor_rect(
                        left as u32,
                        top as u32,
                        (right - left) as u32,
                        (bottom - top) as u32,
                    );
                } else {
                    draw = false;
                }
            }

            if draw {
                self.flow_pipeline_cache
                    .render_fragment(&mut pass, flow_name);
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        true
    }
}
