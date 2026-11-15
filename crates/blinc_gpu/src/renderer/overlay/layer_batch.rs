//! Plain opacity groups drawn through one atlas.
//!
//! A group with no effect, blend or transform costs the group-at-a-time path a clear, a draw, an
//! alpha pass and a blit, each its own render pass and submission. Consecutive groups of that kind
//! are drawn into cells of one texture in one pass, straightened once, and composited in one pass,
//! in the order they were pushed. A group qualifies when its content is a run of plain primitives
//! lying entirely inside the viewport, so no cell receives another's pixels.

use super::{
    Bounds, Cursor, GpuRenderer, Node, PrimitiveBatch, active_effects, bounds, common_clip,
    named::Outputs, shift_primitive,
};
use crate::primitives::{GpuPrimitive, LayerCompositeUniforms, PrimitiveType};
use blinc_core::BlendMode;
use std::num::NonZeroU64;
use wgpu::util::DeviceExt;

/// The largest group, in pixels on a side, that shares an atlas; a bigger one gains little.
const MAX_CELL: u32 = 512;
/// The widest an atlas is, and the height at which another is started.
const ATLAS_EXTENT: u32 = 2048;
/// Atlas sizes round up to this, so a group that grows a pixel does not make a new texture.
const ATLAS_STEP: u32 = 64;

/// A group ready to be drawn into a cell.
struct Plan {
    opacity: f32,
    runs: Vec<(Cursor, Cursor)>,
    /// Top-left of the group in the batch's frame, a whole pixel.
    min: [f32; 2],
    size: (u32, u32),
    clip: Option<([f32; 4], [f32; 4])>,
}

/// Where a plan sits in an atlas.
#[derive(Debug, PartialEq)]
struct Placed {
    plan: usize,
    x: u32,
    y: u32,
}

#[derive(Debug, PartialEq)]
struct Atlas {
    placed: Vec<Placed>,
    size: (u32, u32),
}

/// Lays cells out in rows, in order, starting another atlas when one would pass `limit` high.
/// Each cell leaves a pixel to its right and below.
fn pack(sizes: &[(u32, u32)], limit: u32) -> Vec<Atlas> {
    let round = |v: u32| (v.div_ceil(ATLAS_STEP) * ATLAS_STEP).min(limit);
    let mut atlases = Vec::new();
    let mut placed = Vec::new();
    let (mut x, mut y, mut row, mut width) = (0, 0, 0, 0);
    let mut close = |placed: &mut Vec<Placed>, width: u32, height: u32| {
        if !placed.is_empty() {
            atlases.push(Atlas {
                placed: std::mem::take(placed),
                size: (round(width), round(height)),
            });
        }
    };
    for (plan, &(w, h)) in sizes.iter().enumerate() {
        let (pw, ph) = (w + 1, h + 1);
        if x + pw > limit {
            x = 0;
            y += row;
            row = 0;
        }
        if y + ph > limit {
            close(&mut placed, width, y + row);
            (x, y, row, width) = (0, 0, 0, 0);
        }
        placed.push(Placed { plan, x, y });
        x += pw;
        row = row.max(ph);
        width = width.max(x);
    }
    close(&mut placed, width, y + row);
    atlases
}

/// One group's composite onto the target.
struct Cell {
    source: (u32, u32),
    size: (u32, u32),
    dest: (f32, f32),
    opacity: f32,
    clip: Option<([f32; 4], [f32; 4])>,
}

impl GpuRenderer {
    /// Draws the run of plain opacity groups that starts at `items[first]` through atlases when
    /// there are at least two, and returns the index after the run. `None` leaves the group at
    /// `first` to the group-at-a-time path.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_opacity_layers(
        &mut self,
        target: &wgpu::TextureView,
        batch: &PrimitiveBatch,
        items: &[Node],
        first: usize,
        origin: [f32; 2],
        samples: u32,
        outputs: &Outputs,
    ) -> Option<usize> {
        if !self.batch_opacity_layers {
            return None;
        }
        let size = self.viewport_size;
        let viewport = Bounds {
            min: origin,
            max: [origin[0] + size.0 as f32, origin[1] + size.1 as f32],
        };
        let mut plans = Vec::new();
        let mut end = first;
        while let Some(Node::Layer(config, children)) = items.get(end) {
            match plan(config, children, batch, viewport, origin, outputs) {
                None => break,
                Some(None) => {}
                Some(Some(plan)) => plans.push(plan),
            }
            end += 1;
        }
        match plans.len() {
            0 if end > first => return Some(end),
            0 | 1 => return None,
            _ => {}
        }
        let limit = self
            .device
            .limits()
            .max_texture_dimension_2d
            .min(ATLAS_EXTENT);
        let sizes: Vec<(u32, u32)> = plans.iter().map(|p| p.size).collect();
        for atlas in pack(&sizes, limit) {
            self.draw_atlas(target, batch, &plans, &atlas, origin, samples);
        }
        Some(end)
    }

    fn draw_atlas(
        &mut self,
        target: &wgpu::TextureView,
        batch: &PrimitiveBatch,
        plans: &[Plan],
        atlas: &Atlas,
        origin: [f32; 2],
        samples: u32,
    ) {
        self.opacity_atlases += 1;
        let viewport = self.viewport_size;
        let texture = self
            .layer_texture_cache
            .acquire_exact(&self.device, atlas.size, false);
        let mut content = PrimitiveBatch::new();
        content.aux_data.clone_from(&batch.aux_data);
        let mut cells = Vec::with_capacity(atlas.placed.len());
        for at in &atlas.placed {
            let plan = &plans[at.plan];
            let (dx, dy) = (at.x as f32 - plan.min[0], at.y as f32 - plan.min[1]);
            for (start, end) in &plan.runs {
                for p in &batch.primitives[start.primitive..end.primitive] {
                    let mut p = *p;
                    shift_primitive(&mut p, dx, dy);
                    content.primitives.push(p);
                }
            }
            cells.push(Cell {
                source: (at.x, at.y),
                size: plan.size,
                dest: (plan.min[0] - origin[0], plan.min[1] - origin[1]),
                opacity: plan.opacity,
                clip: plan.clip,
            });
        }
        self.viewport_size = texture.size;
        self.render_with_clear_simple(&texture.view, &PrimitiveBatch::new(), [0.0; 4]);
        self.render_overlay_msaa_flat(&texture.view, &content, samples);
        self.viewport_size = viewport;
        let straight = self
            .layer_texture_cache
            .acquire_exact(&self.device, texture.size, false);
        self.ensure_overlay_alpha();
        self.overlay_alpha.as_ref().unwrap().straighten(
            &self.device,
            &self.queue,
            &self.bind_group_layouts.composite,
            &self.path_image_sampler,
            &texture,
            &straight,
        );
        self.composite_cells(target, &straight, &cells);
        self.layer_texture_cache.release(straight);
        self.layer_texture_cache.release(texture);
    }

    /// Composites every cell of `atlas` onto `target` in one pass, in order, each confined to the
    /// part of it that is inside the viewport and its clip.
    fn composite_cells(
        &self,
        target: &wgpu::TextureView,
        atlas: &super::LayerTexture,
        cells: &[Cell],
    ) {
        let (vp_w, vp_h) = (self.viewport_size.0 as f32, self.viewport_size.1 as f32);
        let (atlas_w, atlas_h) = (atlas.size.0 as f32, atlas.size.1 as f32);
        let used = std::mem::size_of::<LayerCompositeUniforms>();
        let stride = used
            .next_multiple_of(self.device.limits().min_uniform_buffer_offset_alignment as usize);
        let mut bytes = Vec::new();
        let mut draws = Vec::new();
        for cell in cells {
            let (w, h) = (cell.size.0 as f32, cell.size.1 as f32);
            let mut vis = [
                cell.dest.0.max(0.0),
                cell.dest.1.max(0.0),
                (cell.dest.0 + w).min(vp_w),
                (cell.dest.1 + h).min(vp_h),
            ];
            let (clip_bounds, clip_radius, clip_type) = match cell.clip {
                Some((b, r)) => {
                    vis = [
                        vis[0].max(b[0]),
                        vis[1].max(b[1]),
                        vis[2].min(b[0] + b[2]),
                        vis[3].min(b[1] + b[3]),
                    ];
                    (b, r, 1)
                }
                None => ([0.0, 0.0, vp_w, vp_h], [0.0; 4], 0),
            };
            let (vis_w, vis_h) = (vis[2] - vis[0], vis[3] - vis[1]);
            if vis_w <= 0.0 || vis_h <= 0.0 {
                continue;
            }
            let mut scissor = (
                vis[0].max(0.0) as u32,
                vis[1].max(0.0) as u32,
                vis_w.max(1.0) as u32,
                vis_h.max(1.0) as u32,
            );
            if let Some((dx, dy, dw, dh)) = self.pending_damage_scissor {
                let x0 = scissor.0.max(dx);
                let y0 = scissor.1.max(dy);
                let x1 = (scissor.0 + scissor.2).min(dx + dw);
                let y1 = (scissor.1 + scissor.3).min(dy + dh);
                if x1 <= x0 || y1 <= y0 {
                    continue;
                }
                scissor = (x0, y0, x1 - x0, y1 - y0);
            }
            let uniforms = LayerCompositeUniforms {
                source_rect: [
                    (cell.source.0 as f32 + vis[0] - cell.dest.0) / atlas_w,
                    (cell.source.1 as f32 + vis[1] - cell.dest.1) / atlas_h,
                    vis_w / atlas_w,
                    vis_h / atlas_h,
                ],
                dest_rect: [vis[0], vis[1], vis_w, vis_h],
                viewport_size: [vp_w, vp_h],
                opacity: cell.opacity,
                blend_mode: BlendMode::Normal as u32,
                clip_bounds,
                clip_radius,
                clip_type,
                perspective_d: 0.0,
                sin_rx: 0.0,
                cos_rx: 1.0,
                sin_ry: 0.0,
                cos_ry: 1.0,
                sin_rz: 0.0,
                cos_rz: 1.0,
            };
            bytes.extend_from_slice(bytemuck::bytes_of(&uniforms));
            bytes.resize(draws.len() * stride + stride, 0);
            draws.push(scissor);
        }
        if draws.is_empty() {
            return;
        }
        let uniforms = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Opacity Layer Uniforms"),
                contents: &bytes,
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let binding = |index: usize| wgpu::BufferBinding {
            buffer: &uniforms,
            offset: (index * stride) as u64,
            size: NonZeroU64::new(used as u64),
        };
        let groups: Vec<wgpu::BindGroup> = (0..draws.len())
            .map(|index| {
                self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Opacity Layer Bind Group"),
                    layout: &self.bind_group_layouts.layer_composite,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::Buffer(binding(index)),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(&atlas.view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::Sampler(&self.path_image_sampler),
                        },
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: wgpu::BindingResource::TextureView(
                                &self.dummy_blend_dest_view,
                            ),
                        },
                        wgpu::BindGroupEntry {
                            binding: 4,
                            resource: wgpu::BindingResource::Sampler(&self.path_image_sampler),
                        },
                    ],
                })
            })
            .collect();
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Opacity Layer Encoder"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Opacity Layer Pass"),
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
            pass.set_pipeline(&self.pipelines.layer_composite);
            for (group, (x, y, w, h)) in groups.iter().zip(&draws) {
                pass.set_scissor_rect(*x, *y, *w, *h);
                pass.set_bind_group(0, group, &[]);
                pass.draw(0..6, 0..1);
            }
        }
        self.queue.submit(std::iter::once(encoder.finish()));
    }
}

/// `None` when the group is not for an atlas, `Some(None)` when the group-at-a-time path would
/// draw nothing for it, otherwise its plan.
fn plan(
    config: &blinc_core::LayerConfig,
    children: &[Node],
    batch: &PrimitiveBatch,
    viewport: Bounds,
    origin: [f32; 2],
    outputs: &Outputs,
) -> Option<Option<Plan>> {
    if config.id.is_some_and(|id| outputs.wanted.contains(&id)) {
        return None;
    }
    if config.opacity <= 0.0 {
        return Some(None);
    }
    if config.opacity >= 1.0
        || config.blend_mode != BlendMode::Normal
        || config.transform_3d.is_some()
        || !active_effects(&config.effects).is_empty()
    {
        return None;
    }
    let mut runs = Vec::with_capacity(children.len());
    for child in children {
        let Node::Run(start, end) = child else {
            return None;
        };
        if start.vertex != end.vertex || start.index != end.index {
            return None;
        }
        if !batch.primitives[start.primitive..end.primitive]
            .iter()
            .all(plain)
        {
            return None;
        }
        runs.push((*start, *end));
    }
    let Some(b) = bounds(children, batch, viewport, &outputs.wanted) else {
        return Some(None);
    };
    if b.max[0] <= viewport.min[0]
        || b.max[1] <= viewport.min[1]
        || b.min[0] >= viewport.max[0]
        || b.min[1] >= viewport.max[1]
    {
        return Some(None);
    }
    let min = b.min.map(f32::floor);
    let max = b.max.map(f32::ceil);
    let size = (
        (max[0] - min[0]).max(1.0) as u32,
        (max[1] - min[1]).max(1.0) as u32,
    );
    if size.0 > MAX_CELL || size.1 > MAX_CELL {
        return None;
    }
    let clip = common_clip(children, batch).map(|(mut bounds, radius)| {
        bounds[0] -= origin[0];
        bounds[1] -= origin[1];
        (bounds, radius)
    });
    Some(Some(Plan {
        opacity: config.opacity,
        runs,
        min,
        size,
        clip,
    }))
}

/// A primitive that draws from its own bounds alone: no perspective, mesh or clip frame, which
/// carry geometry in the aux buffer that a move would have to rewrite per group.
fn plain(p: &GpuPrimitive) -> bool {
    p.perspective[2] <= 0.0
        && p.perspective[3] <= 0.0
        && p.type_info[0] != PrimitiveType::Mesh as u32
        && p.clip_shape[0] as u32 == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cells_fill_rows_left_to_right() {
        let atlases = pack(&[(100, 50), (100, 60), (100, 40)], 256);
        assert_eq!(atlases.len(), 1);
        let at: Vec<_> = atlases[0].placed.iter().map(|p| (p.x, p.y)).collect();
        assert_eq!(at, [(0, 0), (101, 0), (0, 61)]);
        assert_eq!(atlases[0].size, (256, 128));
    }

    #[test]
    fn a_full_atlas_starts_another_in_order() {
        let atlases = pack(&[(200, 200), (200, 200), (200, 200)], 256);
        let plans: Vec<Vec<usize>> = atlases
            .iter()
            .map(|a| a.placed.iter().map(|p| p.plan).collect())
            .collect();
        assert_eq!(plans, [vec![0], vec![1], vec![2]]);
        assert!(
            atlases
                .iter()
                .all(|a| a.placed[0].x == 0 && a.placed[0].y == 0)
        );
    }

    #[test]
    fn atlas_sizes_round_up_so_small_growth_keeps_the_texture() {
        let a = pack(&[(100, 50), (101, 50)], 2048);
        let b = pack(&[(103, 52), (104, 50)], 2048);
        assert_eq!(a[0].size, b[0].size);
    }

    #[test]
    fn nothing_packs_to_nothing() {
        assert!(pack(&[], 2048).is_empty());
    }

    const SIZE: u32 = 160;

    fn renderer() -> Option<GpuRenderer> {
        let mut r = pollster::block_on(GpuRenderer::new(crate::RendererConfig {
            texture_format: Some(wgpu::TextureFormat::Rgba8Unorm),
            ..Default::default()
        }))
        .ok()?;
        r.resize(SIZE, SIZE);
        Some(r)
    }

    /// A background, then groups whose own draws overlap, so a group is not the same picture as
    /// its draws faded one by one.
    fn scene() -> PrimitiveBatch {
        use crate::primitives::LayerCommand;
        use blinc_core::LayerConfig;
        let rect = |x, y, w, h, c: [f32; 3]| {
            GpuPrimitive::rect(x, y, w, h).with_color(c[0], c[1], c[2], 1.0)
        };
        let mut batch = PrimitiveBatch::new();
        batch.push(rect(0.0, 0.0, SIZE as f32, SIZE as f32, [0.0, 0.0, 1.0]));
        let mut group = |opacity: f32, prims: Vec<GpuPrimitive>| {
            batch.push_layer_command(LayerCommand::Push {
                config: LayerConfig::new().opacity(opacity),
            });
            for p in prims {
                batch.push(p);
            }
            batch.push_layer_command(LayerCommand::Pop);
        };
        // A pair that shares an atlas.
        group(
            0.5,
            vec![
                rect(10.0, 10.0, 30.0, 30.0, [1.0, 0.0, 0.0]),
                rect(25.0, 25.0, 30.0, 30.0, [0.0, 1.0, 0.0]),
            ],
        );
        group(
            0.3,
            vec![
                rect(70.0, 10.0, 30.0, 30.0, [1.0, 1.0, 1.0]),
                rect(80.0, 20.0, 30.0, 30.0, [1.0, 1.0, 0.0]),
            ],
        );
        // Cut by the viewport: its cell is as large as the group, and the composite keeps to the
        // viewport.
        group(0.6, vec![rect(140.0, 60.0, 40.0, 30.0, [0.0, 1.0, 1.0])]);
        // A clipped one, then two more that share an atlas.
        group(
            0.8,
            vec![
                rect(10.0, 70.0, 40.0, 40.0, [1.0, 0.0, 1.0])
                    .with_clip_rect(20.0, 70.0, 20.0, 40.0),
            ],
        );
        group(
            0.4,
            vec![
                rect(60.0, 100.0, 25.0, 25.0, [1.0, 0.5, 0.0]).with_corner_radius(8.0),
                rect(70.0, 110.0, 25.0, 25.0, [0.0, 0.5, 0.0]).with_corner_radius(8.0),
            ],
        );
        group(0.7, vec![rect(100.0, 100.0, 20.0, 20.0, [0.5, 0.0, 1.0])]);
        // Cut by the left edge and by the top one, so a cell begins before the viewport does.
        group(
            0.5,
            vec![
                rect(-12.0, 130.0, 30.0, 24.0, [1.0, 0.0, 0.0]),
                rect(-4.0, 134.0, 20.0, 20.0, [1.0, 1.0, 0.0]).with_corner_radius(6.0),
            ],
        );
        group(0.6, vec![rect(120.0, -8.0, 30.0, 24.0, [0.0, 1.0, 0.0])]);
        batch
    }

    fn render(r: &mut GpuRenderer, batch: &PrimitiveBatch, samples: u32) -> Vec<u8> {
        let device = r.device_arc();
        let queue = r.queue_arc();
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("layer batch"),
            size: wgpu::Extent3d {
                width: SIZE,
                height: SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = target.create_view(&Default::default());
        r.render_overlay_msaa(&view, batch, samples);
        let padded = (SIZE * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("layer batch readback"),
            size: (padded * SIZE) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            target.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: None,
                },
            },
            wgpu::Extent3d {
                width: SIZE,
                height: SIZE,
                depth_or_array_layers: 1,
            },
        );
        queue.submit([encoder.finish()]);
        readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, |r| r.expect("map"));
        device
            .poll(wgpu::PollType::Wait)
            .expect("the copy finishes");
        let mapped = readback.slice(..).get_mapped_range();
        mapped
            .chunks_exact(padded as usize)
            .flat_map(|row| &row[..(SIZE * 4) as usize])
            .copied()
            .collect()
    }

    fn at(pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
        let i = ((y * SIZE + x) * 4) as usize;
        [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
    }

    fn near(a: [u8; 4], b: [u8; 4], by: u8) -> bool {
        a.iter().zip(b).all(|(a, b)| a.abs_diff(b) <= by)
    }

    #[test]
    fn groups_through_an_atlas_paint_the_pixels_the_groups_alone_do() {
        let Some(mut r) = renderer() else {
            eprintln!("no GPU adapter; skipping");
            return;
        };
        let batch = scene();
        for samples in [1, 4] {
            r.set_batch_opacity_layers(false);
            let alone = render(&mut r, &batch, samples);
            let before = r.opacity_atlas_count();
            r.set_batch_opacity_layers(true);
            let batched = render(&mut r, &batch, samples);
            // All eight groups, those cut by the viewport among them, share one atlas.
            assert_eq!(r.opacity_atlas_count() - before, 1);
            let worst = alone
                .iter()
                .zip(&batched)
                .map(|(a, b)| a.abs_diff(*b))
                .max()
                .unwrap();
            assert!(worst <= 1, "{samples} samples: pixels differ by {worst}");
            // A group fades as one picture: green covers red inside it, then the pair is half
            // over the blue.
            assert!(
                near(at(&batched, 30, 30), [0, 128, 128, 255], 3),
                "{:?}",
                at(&batched, 30, 30)
            );
            assert!(
                near(at(&batched, 12, 12), [128, 0, 128, 255], 3),
                "{:?}",
                at(&batched, 12, 12)
            );
            // The clip cuts the clipped group's left edge.
            assert!(near(at(&batched, 15, 90), [0, 0, 255, 255], 1));
        }
    }
}
