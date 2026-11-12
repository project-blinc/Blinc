//! The offscreen targets the overlay passes draw into and composite from, one per viewport size
//! and sample count in use. A pass at one sample draws straight into the single-sampled texture,
//! so only a multisampled pass has a multisampled texture as well. A target no frame has drawn
//! for `IDLE_FRAMES` is freed, so a pass that ran for a while (at startup, say) does not keep
//! a window-sized texture for the life of the app.

use super::GpuRenderer;
use wgpu::util::DeviceExt;

use super::idle::IDLE_FRAMES;

/// One size's target and what composites it.
pub(super) struct OverlayTarget {
    /// Drawn into and resolved from when the pass takes more than one sample.
    pub(super) msaa_view: Option<wgpu::TextureView>,
    /// Drawn into at one sample, resolved into otherwise; the composite reads it.
    pub(super) resolve_view: wgpu::TextureView,
    pub(super) composite_bind_group: wgpu::BindGroup,
    width: u32,
    height: u32,
    sample_count: u32,
    /// The frame that last drew it.
    last_use: u64,
}

/// The overlay targets in use.
#[derive(Default)]
pub(super) struct OverlayTargets {
    targets: Vec<OverlayTarget>,
    /// Frames started so far, the clock `last_use` is read against.
    frame: u64,
}

impl OverlayTargets {
    /// Frames started so far.
    pub(super) fn frame(&self) -> u64 {
        self.frame
    }

    pub(super) fn clear(&mut self) {
        self.targets.clear();
    }

    pub(super) fn get(&self, index: usize) -> &OverlayTarget {
        &self.targets[index]
    }

    pub(super) fn next_frame(&mut self) {
        self.frame += 1;
        let now = self.frame;
        self.targets.retain(|t| now - t.last_use <= IDLE_FRAMES);
    }
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct CompositeUniforms {
    opacity: f32,
    blend_mode: u32,
    _padding: [f32; 2],
}

impl GpuRenderer {
    /// The index of the overlay target for the current viewport at `sample_count`, made if there
    /// is none. It holds until the frame ends.
    pub(super) fn overlay_target(&mut self, sample_count: u32) -> usize {
        let (width, height) = self.viewport_size;
        let key = (width, height, sample_count);
        let now = self.overlay_targets.frame;
        let found = self
            .overlay_targets
            .targets
            .iter()
            .position(|t| (t.width, t.height, t.sample_count) == key);
        let index = match found {
            Some(index) => index,
            None => {
                let target = self.make_overlay_target(width, height, sample_count);
                self.overlay_targets.targets.push(target);
                self.overlay_targets.targets.len() - 1
            }
        };
        self.overlay_targets.targets[index].last_use = now;
        index
    }

    fn make_overlay_target(&self, width: u32, height: u32, sample_count: u32) -> OverlayTarget {
        let texture = |label, samples, usage| {
            self.device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: samples,
                    dimension: wgpu::TextureDimension::D2,
                    format: self.texture_format,
                    usage,
                    view_formats: &[],
                })
                .create_view(&wgpu::TextureViewDescriptor::default())
        };
        let msaa_view = (sample_count > 1).then(|| {
            texture(
                "Overlay MSAA Texture",
                sample_count.max(1),
                wgpu::TextureUsages::RENDER_ATTACHMENT,
            )
        });
        let resolve_view = texture(
            "Overlay Resolve Texture",
            1,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        );
        let sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Overlay Blend Sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        // Opacity 1, normal blend.
        let uniforms = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Overlay Composite Uniforms"),
                contents: bytemuck::bytes_of(&CompositeUniforms {
                    opacity: 1.0,
                    blend_mode: 0,
                    _padding: [0.0; 2],
                }),
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let composite_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Overlay Composite Bind Group"),
            layout: &self.bind_group_layouts.composite,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniforms.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&resolve_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        OverlayTarget {
            msaa_view,
            resolve_view,
            composite_bind_group,
            width,
            height,
            sample_count,
            last_use: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::RendererConfig;

    fn renderer() -> Option<GpuRenderer> {
        pollster::block_on(GpuRenderer::new(RendererConfig::default())).ok()
    }

    /// The target `overlay_target` gives for a `size` viewport.
    fn target_at(r: &mut GpuRenderer, size: (u32, u32), samples: u32) -> wgpu::TextureView {
        r.viewport_size = size;
        let index = r.overlay_target(samples);
        r.overlay_targets.get(index).resolve_view.clone()
    }

    #[test]
    fn a_single_sampled_target_has_no_multisampled_texture() {
        let Some(mut r) = renderer() else { return };
        r.viewport_size = (64, 32);
        let one = r.overlay_target(1);
        assert!(r.overlay_targets.get(one).msaa_view.is_none());
        let four = r.overlay_target(4);
        assert!(r.overlay_targets.get(four).msaa_view.is_some());
    }

    #[test]
    fn alternating_sizes_keep_their_targets() {
        let Some(mut r) = renderer() else { return };
        let window = target_at(&mut r, (200, 100), 1);
        let layer = target_at(&mut r, (80, 40), 1);
        for _ in 0..10 {
            assert!(
                target_at(&mut r, (200, 100), 1) == window,
                "the window's target was rebuilt"
            );
            assert!(
                target_at(&mut r, (80, 40), 1) == layer,
                "the layer's target was rebuilt"
            );
            r.next_frame();
        }
        assert_eq!(r.overlay_targets.targets.len(), 2);
    }

    #[test]
    fn a_size_no_longer_drawn_frees_its_target() {
        let Some(mut r) = renderer() else { return };
        target_at(&mut r, (80, 40), 1);
        for _ in 0..=IDLE_FRAMES {
            target_at(&mut r, (200, 100), 1);
            r.next_frame();
        }
        assert_eq!(r.overlay_targets.targets.len(), 1);
        assert_eq!(r.overlay_targets.targets[0].width, 200);
        for _ in 0..=IDLE_FRAMES {
            r.next_frame();
        }
        assert!(
            r.overlay_targets.targets.is_empty(),
            "an idle pass kept its target"
        );
    }

    #[test]
    fn a_layer_pool_no_pass_uses_is_flushed_by_frames() {
        let Some(mut r) = renderer() else { return };
        let texture = r.layer_texture_cache.acquire(&r.device, (64, 64), false);
        r.layer_texture_cache.release(texture);
        assert_eq!(r.layer_texture_cache.pool_small.len(), 1);
        for _ in 0..=super::super::layer_cache::IDLE_FULL_FLUSH {
            r.next_frame();
        }
        assert!(
            r.layer_texture_cache.pool_small.is_empty(),
            "the idle pool kept its texture"
        );
    }

    #[test]
    fn the_scene_copy_goes_once_no_frame_uses_it() {
        let Some(mut r) = renderer() else { return };
        r.viewport_size = (64, 32);
        for frame in 0..(IDLE_FRAMES * 2) {
            if frame % 5 == 0 {
                r.ensure_scene_copy();
            }
            r.next_frame();
        }
        assert!(
            r.scene_copy_texture.is_some(),
            "a copy used every few frames was freed"
        );
        for _ in 0..=IDLE_FRAMES {
            r.next_frame();
        }
        assert!(
            r.scene_copy_texture.is_none(),
            "an unused scene copy was kept"
        );
    }
}
