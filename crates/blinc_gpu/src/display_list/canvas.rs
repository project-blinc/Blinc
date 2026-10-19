//! Synchronous GPU callbacks at ABI canvas slots.
//!
//! Prepare pipelines and buffers once, then register a closure with
//! [`Painter::set_canvas`]. Group 0 belongs to the canvas frame; caller groups
//! start at 1. The WGSL helpers `canvas_position(local)`, `canvas_local(pixel)`
//! and `canvas_clip(pixel, straight_rgba)` apply the current transform, clips
//! and record opacity. `canvas_vertex` supplies a six-vertex content-box quad.
//! Every fragment entry must use `canvas_clip`; the scissor alone is not exact.
//!
//! ```no_run
//! # use blinc_gpu::display_list::canvas::{CanvasFrame, CanvasPipelineDescriptor};
//! # use blinc_gpu::display_list::plan::Painter;
//! # fn register(painter: &mut Painter, device: &wgpu::Device, format: wgpu::TextureFormat) {
//! let pipeline = painter.create_canvas_pipeline(&device, format,
//!     &CanvasPipelineDescriptor::new(r#"
//! @fragment fn fs_main(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
//!     return canvas_clip(p.xy, vec4<f32>(0.2, 0.6, 0.9, 1.0));
//! }
//! "#), &[]);
//! painter.set_canvas(7, move |frame: &mut CanvasFrame<'_>| {
//!     let mut pass = frame.begin_pass(&pipeline);
//!     pass.draw(0..6, 0..1);
//! });
//! # }
//! ```

use std::collections::BTreeMap;
use std::sync::Arc;

use super::plan::Painter;
use super::{Kind, RECORD_FLOATS, Record, Unsupported, row};

const SHADER: &str = include_str!(concat!(env!("OUT_DIR"), "/canvas.wgsl"));

/// A canvas's logical rows and shape payload, independent of registered resources.
#[derive(Clone, Debug)]
pub struct CanvasDraw {
    pub slot: u32,
    pub rows: [[f32; 4]; RECORD_FLOATS / 4],
    /// Device pixels per logical pixel.
    pub pixel_ratio: f32,
    /// Full-frame physical origin of the target holding this draw.
    pub target_origin: [f32; 2],
    pub aux: Vec<[f32; 4]>,
}

impl Record<'_> {
    /// Decode a canvas; `list` includes any trailing polygon points.
    pub fn canvas(&self, pixel_ratio: f32, list: &[f32]) -> Result<CanvasDraw, Unsupported> {
        if self.kind() != Kind::Canvas {
            return Err(Unsupported::Kind(self.row(row::TYPES)[0]));
        }
        if let Some(why) = self.unsupported() {
            return Err(why);
        }
        let rows: [[f32; 4]; RECORD_FLOATS / 4] = std::array::from_fn(|i| self.row(i));
        let [slot, scale, _, _] = rows[row::GRADIENT];
        let [_, _, w, h] = rows[row::BOUNDS];
        let [a, b, c, d] = rows[row::AFFINE];
        if !pixel_ratio.is_finite() || pixel_ratio <= 0.0 {
            return Err(Unsupported::Degenerate);
        }
        if rows.iter().flatten().any(|v| !v.is_finite())
            || !(0.0..=0xffffff as f32).contains(&slot)
            || slot.fract() != 0.0
            || scale <= 0.0
            || !(a * d - b * c).is_finite()
            || w <= 0.0
            || h <= 0.0
        {
            return Err(Unsupported::CanvasData);
        }
        Ok(CanvasDraw {
            slot: slot as u32,
            rows,
            pixel_ratio,
            target_origin: [0.0; 2],
            aux: self.clip_rows(pixel_ratio, list)?,
        })
    }
}

impl CanvasDraw {
    pub(super) fn moved(&self, dx: f32, dy: f32) -> Self {
        let mut draw = self.clone();
        for at in [row::BOUNDS, row::CLIP, row::FADE_RECT] {
            draw.rows[at][0] += dx / self.pixel_ratio;
            draw.rows[at][1] += dy / self.pixel_ratio;
        }
        draw.target_origin[0] -= dx;
        draw.target_origin[1] -= dy;
        if !draw.aux.is_empty() {
            super::clip::translate(&mut draw.aux, dx, dy);
        }
        draw
    }

    fn scissor(&self, size: (u32, u32)) -> Option<[u32; 4]> {
        let [x, y, w, h] = self.rows[row::BOUNDS];
        let [a, b, c, d] = self.rows[row::AFFINE];
        let ratio = self.pixel_ratio;
        let mut low = [
            x + (a * w).min(0.0) + (c * h).min(0.0),
            y + (b * w).min(0.0) + (d * h).min(0.0),
        ];
        let mut high = [
            x + (a * w).max(0.0) + (c * h).max(0.0),
            y + (b * w).max(0.0) + (d * h).max(0.0),
        ];
        if self.rows[row::TYPES][2] as u32 & super::CLIP_SCREEN != 0 {
            let clip = self.rows[row::CLIP];
            for axis in 0..2 {
                // Keep the analytic clip's antialiasing fringe.
                low[axis] = low[axis].max(clip[axis] - 0.75);
                high[axis] = high[axis].min(clip[axis] + clip[axis + 2] + 0.75);
            }
        }
        let x0 = (low[0] * ratio).floor().clamp(0.0, size.0 as f32) as u32;
        let y0 = (low[1] * ratio).floor().clamp(0.0, size.1 as f32) as u32;
        let x1 = (high[0] * ratio).ceil().clamp(0.0, size.0 as f32) as u32;
        let y1 = (high[1] * ratio).ceil().clamp(0.0, size.1 as f32) as u32;
        (x1 > x0 && y1 > y0).then_some([x0, y0, x1.saturating_sub(x0), y1.saturating_sub(y0)])
    }
}

/// Custom shaders appended to the canvas helpers. Fragment output is straight RGBA.
///
/// Use `canvas_clip` in the fragment entry. The default vertex entry draws the
/// content box; custom vertices can use `canvas_position` for local coordinates.
/// Bind group 0 is reserved, and additional layouts begin at group 1.
pub struct CanvasPipelineDescriptor<'a> {
    pub label: Option<&'a str>,
    pub shader: &'a str,
    pub vertex_entry: &'a str,
    pub fragment_entry: &'a str,
    pub buffers: &'a [wgpu::VertexBufferLayout<'a>],
    pub primitive: wgpu::PrimitiveState,
}

impl<'a> CanvasPipelineDescriptor<'a> {
    /// A triangle-list content quad and the caller's `fs_main` fragment entry.
    pub fn new(shader: &'a str) -> Self {
        Self {
            label: Some("Display List Canvas"),
            shader,
            vertex_entry: "canvas_vertex",
            fragment_entry: "fs_main",
            buffers: &[],
            primitive: wgpu::PrimitiveState::default(),
        }
    }
}

/// Prepared outside paint and retained by the consumer across frames.
pub struct CanvasPipeline {
    pipeline: wgpu::RenderPipeline,
    owner: Arc<()>,
    format: wgpu::TextureFormat,
}

/// Borrowed for one synchronous callback. No canvas render texture is allocated.
///
/// `transform` maps local content coordinates into full-frame logical units.
/// `target_origin` is in physical pixels; subtract it after applying
/// `pixel_ratio` when writing directly to this layer's target. The shader
/// helpers already perform that conversion. Auxiliary passes can use `encoder`;
/// return to the current target with [`Self::begin_pass`]. No CPU/GPU wait occurs.
pub struct CanvasFrame<'a> {
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    pub encoder: &'a mut wgpu::CommandEncoder,
    pub target: &'a wgpu::TextureView,
    pub format: wgpu::TextureFormat,
    pub size: [f32; 2],
    pub transform: [f32; 6],
    /// Producer's device pixels per local unit, using the affine determinant.
    pub scale: f32,
    pub pixel_ratio: f32,
    pub target_size: (u32, u32),
    pub target_origin: [f32; 2],
    /// Physical `[x, y, width, height]` within the current target.
    pub scissor: [u32; 4],
    pub slot: u32,
    group: &'a wgpu::BindGroup,
    owner: &'a Arc<()>,
}

impl CanvasFrame<'_> {
    /// Load the current layer, bind frame data and set its canvas scissor.
    /// The pass ends when dropped. Caller bind groups begin at 1.
    pub fn begin_pass(&mut self, pipeline: &CanvasPipeline) -> wgpu::RenderPass<'_> {
        assert!(
            Arc::ptr_eq(self.owner, &pipeline.owner),
            "canvas pipeline belongs to another Painter"
        );
        assert_eq!(
            self.format, pipeline.format,
            "canvas pipeline target format changed"
        );
        let mut pass = self.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Display List Canvas"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: self.target,
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
        pass.set_pipeline(&pipeline.pipeline);
        pass.set_bind_group(0, self.group, &[]);
        let [x, y, w, h] = self.scissor;
        pass.set_scissor_rect(x, y, w, h);
        pass
    }
}

/// A synchronous callback. Native callbacks may move with their Painter;
/// browser callbacks can retain the single-threaded WebGPU resources.
#[cfg(not(target_arch = "wasm32"))]
pub trait CanvasPaint: Send {
    fn paint(&mut self, frame: &mut CanvasFrame<'_>);
}

/// Browser counterpart without the native `Send` bound.
#[cfg(target_arch = "wasm32")]
pub trait CanvasPaint {
    fn paint(&mut self, frame: &mut CanvasFrame<'_>);
}

#[cfg(not(target_arch = "wasm32"))]
impl<F: FnMut(&mut CanvasFrame<'_>) + Send> CanvasPaint for F {
    fn paint(&mut self, frame: &mut CanvasFrame<'_>) {
        self(frame);
    }
}

#[cfg(target_arch = "wasm32")]
impl<F: FnMut(&mut CanvasFrame<'_>)> CanvasPaint for F {
    fn paint(&mut self, frame: &mut CanvasFrame<'_>) {
        self(frame);
    }
}

impl Painter {
    /// Register or replace a slot. Captured resources live until replaced,
    /// removed, or this Painter is dropped. Slots are exact ABI f32 integers.
    pub fn set_canvas(&mut self, slot: u32, paint: impl CanvasPaint + 'static) {
        assert!(slot <= 0xffffff, "canvas slot exceeds the ABI range");
        self.canvases.callbacks.insert(slot, Box::new(paint));
    }

    /// Remove a slot and release the callback's retained resources.
    pub fn remove_canvas(&mut self, slot: u32) -> bool {
        self.canvases.callbacks.remove(&slot).is_some()
    }

    /// Missing canvas slots from the last render, in paint order, including repeats.
    /// Missing image slots remain the return value of [`Self::render`].
    pub fn missing_canvases(&self) -> &[u32] {
        &self.canvases.missing
    }

    /// Create a reusable pipeline with the frame layout and shared WGSL helpers.
    /// Call outside paint, on the renderer's device and target format.
    pub fn create_canvas_pipeline(
        &mut self,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        descriptor: &CanvasPipelineDescriptor<'_>,
        extra_layouts: &[&wgpu::BindGroupLayout],
    ) -> CanvasPipeline {
        let gpu = self
            .canvases
            .gpu
            .get_or_insert_with(|| CanvasGpu::new(device));
        let mut layouts = vec![&gpu.layout];
        layouts.extend_from_slice(extra_layouts);
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: descriptor.label,
            bind_group_layouts: &layouts,
            push_constant_ranges: &[],
        });
        let source = format!("{SHADER}\n{}", descriptor.shader);
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: descriptor.label,
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: descriptor.label,
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some(descriptor.vertex_entry),
                buffers: descriptor.buffers,
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some(descriptor.fragment_entry),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: descriptor.primitive,
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });
        CanvasPipeline {
            pipeline,
            owner: gpu.owner.clone(),
            format,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    screen: [f32; 4],
    record: [[f32; 4]; RECORD_FLOATS / 4],
}

#[derive(Default)]
pub(super) struct CanvasRegistry {
    callbacks: BTreeMap<u32, Box<dyn CanvasPaint>>,
    gpu: Option<CanvasGpu>,
    pub missing: Vec<u32>,
}

impl CanvasRegistry {
    pub fn paint(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        target: &wgpu::TextureView,
        size: (u32, u32),
        draw: &CanvasDraw,
    ) {
        let Some(paint) = self.callbacks.get_mut(&draw.slot) else {
            self.missing.push(draw.slot);
            return;
        };
        let Some(scissor) = draw.scissor(size) else {
            return;
        };
        let gpu = self.gpu.get_or_insert_with(|| CanvasGpu::new(device));
        let group = gpu.prepare(device, queue, draw, size);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Display List Canvas"),
        });
        let [a, b, c, d] = draw.rows[row::AFFINE];
        let bounds = draw.rows[row::BOUNDS];
        paint.paint(&mut CanvasFrame {
            device,
            queue,
            encoder: &mut encoder,
            target,
            format,
            size: [bounds[2], bounds[3]],
            transform: [
                a,
                b,
                c,
                d,
                bounds[0] + draw.target_origin[0] / draw.pixel_ratio,
                bounds[1] + draw.target_origin[1] / draw.pixel_ratio,
            ],
            scale: draw.rows[row::GRADIENT][1],
            pixel_ratio: draw.pixel_ratio,
            target_size: size,
            target_origin: draw.target_origin,
            scissor,
            slot: draw.slot,
            group: &group,
            owner: &gpu.owner,
        });
        // Submit before reusing frame uniforms for the next canvas.
        queue.submit(std::iter::once(encoder.finish()));
    }
}

struct CanvasGpu {
    layout: wgpu::BindGroupLayout,
    uniforms: wgpu::Buffer,
    aux: wgpu::Texture,
    aux_height: u32,
    owner: Arc<()>,
}

impl CanvasGpu {
    fn new(device: &wgpu::Device) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Display List Canvas"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        Self {
            layout,
            uniforms: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Display List Canvas"),
                size: std::mem::size_of::<Params>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            aux: Self::texture(device, 1),
            aux_height: 1,
            owner: Arc::new(()),
        }
    }

    fn texture(device: &wgpu::Device, height: u32) -> wgpu::Texture {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Display List Canvas Clips"),
            size: wgpu::Extent3d {
                width: 1024,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        })
    }

    fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        draw: &CanvasDraw,
        size: (u32, u32),
    ) -> wgpu::BindGroup {
        let height = (draw.aux.len() as u32).div_ceil(1024).max(1);
        if height > self.aux_height {
            self.aux = Self::texture(device, height);
            self.aux_height = height;
        }
        if !draw.aux.is_empty() {
            let mut data = draw.aux.clone();
            data.resize(height as usize * 1024, [0.0; 4]);
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &self.aux,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                bytemuck::cast_slice(&data),
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(1024 * 16),
                    rows_per_image: Some(height),
                },
                wgpu::Extent3d {
                    width: 1024,
                    height,
                    depth_or_array_layers: 1,
                },
            );
        }
        queue.write_buffer(
            &self.uniforms,
            0,
            bytemuck::bytes_of(&Params {
                screen: [
                    size.0 as f32,
                    size.1 as f32,
                    draw.pixel_ratio,
                    if draw.aux.is_empty() { 0.0 } else { 1.0 },
                ],
                record: draw.rows,
            }),
        );
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Display List Canvas"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.uniforms.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(
                        &self.aux.create_view(&Default::default()),
                    ),
                },
            ],
        })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn replacing_and_removing_callbacks_releases_captured_resources() {
        use super::{CanvasFrame, Painter};
        use std::sync::Arc;
        let mut painter = Painter::default();
        let retained = Arc::new(());
        let resource = retained.clone();
        painter.set_canvas(7, move |_: &mut CanvasFrame<'_>| {
            let _ = &resource;
        });
        assert_eq!(Arc::strong_count(&retained), 2);
        painter.set_canvas(7, |_: &mut CanvasFrame<'_>| {});
        assert_eq!(Arc::strong_count(&retained), 1);
        let resource = retained.clone();
        painter.set_canvas(7, move |_: &mut CanvasFrame<'_>| {
            let _ = &resource;
        });
        assert!(painter.remove_canvas(7));
        assert!(!painter.remove_canvas(7));
        assert_eq!(Arc::strong_count(&retained), 1);
    }

    #[test]
    fn canvas_helpers_validate_with_a_consumer_fragment() {
        let source = format!(
            "{}\n@fragment fn fs_main(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {{ return canvas_clip(p.xy, vec4<f32>(1.0)); }}",
            super::SHADER
        );
        let module = naga::front::wgsl::parse_str(&source).expect("canvas WGSL parses");
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .expect("canvas WGSL validates without storage");
    }
}
