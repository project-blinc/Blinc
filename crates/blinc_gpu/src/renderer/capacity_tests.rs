//! A frame with more glyphs than the glyph buffer holds is truncated, not a validation error.

use super::{GpuRenderer, RendererConfig};
use crate::primitives::GpuGlyph;

fn renderer(max_glyphs: usize) -> Option<GpuRenderer> {
    let mut r = pollster::block_on(GpuRenderer::new(RendererConfig {
        max_glyphs,
        texture_format: Some(wgpu::TextureFormat::Rgba8Unorm),
        ..RendererConfig::default()
    }))
    .ok()?;
    r.resize(64, 64);
    Some(r)
}

fn texture(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    usage: wgpu::TextureUsages,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("capacity test"),
        size: wgpu::Extent3d {
            width: 4,
            height: 4,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage,
        view_formats: &[],
    })
}

#[test]
fn more_glyphs_than_the_buffer_holds_are_truncated() {
    let Some(mut r) = renderer(8) else {
        eprintln!("no GPU adapter; skipping");
        return;
    };
    let device = r.device_arc();
    let target = texture(
        &device,
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureUsages::RENDER_ATTACHMENT,
    );
    let atlas = texture(
        &device,
        wgpu::TextureFormat::R8Unorm,
        wgpu::TextureUsages::TEXTURE_BINDING,
    );
    let color_atlas = texture(
        &device,
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureUsages::TEXTURE_BINDING,
    );
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor::default());
    let glyphs = vec![GpuGlyph::default(); 40];

    r.render_text(
        &target.create_view(&wgpu::TextureViewDescriptor::default()),
        &glyphs,
        &atlas.create_view(&wgpu::TextureViewDescriptor::default()),
        &color_atlas.create_view(&wgpu::TextureViewDescriptor::default()),
        &sampler,
    );
}
