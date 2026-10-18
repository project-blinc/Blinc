//! A display list as an ordered plan, and the plan drawn.
//!
//! Boxes, shadows and glyphs are primitives the SDF pipelines draw together.
//! An image is a texture the consumer owns, drawn by the image pipeline in a
//! pass of its own, and a layer is its own content drawn into a texture and
//! composited back. Paint order interleaves them, so a [`Plan`] keeps
//! consecutive primitives together, puts each image between them and nests
//! each layer's items inside it.

use super::effects::{DropShadow, LayerEffects, LayerFrame, LayerFx};
use super::{
    CLIP_SCREEN, DecodeError, Kind, NO_CLIP, Placement, Record, Unsupported, records, row,
};
use crate::image::{GpuImage, GpuImageInstance};
use crate::primitives::{FillType, GpuPrimitive, PrimitiveBatch, PrimitiveType};
use crate::renderer::{GpuRenderer, LayerTexture};

/// An image to draw: the consumer's resource for `slot`, over a box.
#[derive(Clone, Copy, Debug)]
pub struct ImageDraw {
    /// The consumer's name for the image, set by whoever laid the scene out.
    pub slot: u32,
    pub instance: GpuImageInstance,
}

/// A group drawn into a layer of its own and then over its parent as `fx`
/// says, so what overlaps inside it does not show through itself.
#[derive(Clone, Debug)]
pub struct Layer {
    pub items: Vec<Item>,
    pub fx: LayerFx,
    /// Where the layer reaches on the target, in device pixels: what it drew,
    /// widened by how far its blur and shadow spread. Nothing outside it changes.
    pub bounds: [f32; 4],
}

/// Primitives in paint order, drawn together, and what they keep in `aux_data`.
#[derive(Clone, Debug, Default)]
pub struct Run {
    pub primitives: Vec<GpuPrimitive>,
    /// The stop tables of their gradients, which they name by offset.
    pub aux: Vec<[f32; 4]>,
}

impl Run {
    /// `more` after this, its tables' offsets moved to follow this one's.
    fn append(&mut self, mut more: Run) {
        let base = self.aux.len() as f32;
        for p in &mut more.primitives {
            if p.gradient_table[0] > 0.5 {
                p.gradient_table[0] += base;
            }
        }
        self.primitives.append(&mut more.primitives);
        self.aux.append(&mut more.aux);
    }
}

/// One step of a plan.
#[derive(Clone, Debug)]
pub enum Item {
    /// Primitives in paint order, drawn together.
    Primitives(Run),
    Image(ImageDraw),
    Layer(Layer),
}

/// A display list in paint order, and the records that had no place in it.
#[derive(Debug, Default)]
pub struct Plan {
    pub items: Vec<Item>,
    /// The index of each record left out, and why, in list order.
    pub unsupported: Vec<(usize, Unsupported)>,
}

/// A layer being read: what is in it so far, and where it began.
struct Open {
    items: Vec<Item>,
    began: usize,
}

fn push(items: &mut Vec<Item>, item: Item) {
    match (items.last_mut(), item) {
        (Some(Item::Primitives(run)), Item::Primitives(more)) => run.append(more),
        (_, item) => items.push(item),
    }
}

/// Plans `count` records at `scale` device pixels per logical pixel.
///
/// A layer that is masked is not drawn as a layer: its content is drawn as it
/// would be without it, and its record is reported.
pub fn plan(floats: &[f32], count: usize, version: u32, scale: f32) -> Result<Plan, DecodeError> {
    let mut plan = Plan::default();
    let mut open = vec![Open {
        items: Vec::new(),
        began: 0,
    }];
    for (index, record) in records(floats, count, version)?.enumerate() {
        match record.kind() {
            Kind::LayerBegin => open.push(Open {
                items: Vec::new(),
                began: index,
            }),
            Kind::Layer if open.len() == 1 => plan
                .unsupported
                .push((index, Unsupported::Kind(record.row(row::TYPES)[0]))),
            Kind::Layer => {
                let inner = open.pop().expect("a layer is open").items;
                let parent = &mut open.last_mut().expect("the page is open").items;
                match record.layer() {
                    Ok(fx) => parent.push(Item::Layer(Layer {
                        items: inner,
                        fx,
                        bounds: record.row(row::BOUNDS).map(|v| v * scale),
                    })),
                    Err(why) => {
                        plan.unsupported.push((index, why));
                        inner.into_iter().for_each(|item| push(parent, item));
                    }
                }
            }
            Kind::Image => {
                let page = &mut open.last_mut().expect("the page is open").items;
                match record.image(scale) {
                    Ok(draw) => page.push(Item::Image(draw)),
                    Err(why) => plan.unsupported.push((index, why)),
                }
            }
            _ => {
                let page = &mut open.last_mut().expect("the page is open").items;
                // A gradient's table goes at the end of the run the primitive joins.
                let aux_base = match page.last() {
                    Some(Item::Primitives(run)) => run.aux.len(),
                    _ => 0,
                };
                match record.primitive(scale, floats, aux_base) {
                    Ok((p, table)) => match page.last_mut() {
                        Some(Item::Primitives(run)) => {
                            run.primitives.push(p);
                            run.aux.extend(table);
                        }
                        _ => page.push(Item::Primitives(Run {
                            primitives: vec![p],
                            aux: table,
                        })),
                    },
                    Err(why) => plan.unsupported.push((index, why)),
                }
            }
        }
    }
    // A layer never closed is drawn flat, and reported where it began.
    while open.len() > 1 {
        let unclosed = open.pop().expect("a layer is open");
        plan.unsupported
            .push((unclosed.began, Unsupported::Kind(super::KIND_LAYER_BEGIN)));
        let parent = &mut open.last_mut().expect("the page is open").items;
        unclosed
            .items
            .into_iter()
            .for_each(|item| push(parent, item));
    }
    plan.items = open.pop().expect("the page is open").items;
    plan.unsupported.sort_by_key(|(index, _)| *index);
    Ok(plan)
}

impl Record<'_> {
    /// The image this record draws, at `scale` device pixels per logical pixel,
    /// stretched over its box. Its texture is the consumer's, found by
    /// [`ImageDraw::slot`].
    pub fn image(&self, scale: f32) -> Result<ImageDraw, Unsupported> {
        if self.kind() != Kind::Image {
            return Err(Unsupported::Kind(self.row(row::TYPES)[0]));
        }
        if let Some(why) = self.unsupported() {
            return Err(why);
        }
        let Placement {
            bounds,
            local_affine,
            ..
        } = self.placement(scale);
        let mut instance = GpuImageInstance {
            dst_rect: bounds,
            tint: [1.0; 4],
            params: [0.0, self.row(row::FILL_END)[3], 0.0, 0.0],
            transform: local_affine,
            ..GpuImageInstance::default()
        };
        if self.clip_flags() & CLIP_SCREEN != 0 {
            let radii = self.row(row::CLIP_RADII);
            // The image pipeline rounds a clip's corners as circles.
            if radii.iter().any(|r| *r > 0.0) && self.row(row::TYPES)[3] != 1.0 {
                return Err(Unsupported::ClipCorner);
            }
            instance.clip_bounds = self.row(row::CLIP).map(|v| v * scale);
            instance.clip_radius = radii.map(|v| v * scale);
        } else {
            instance.clip_bounds = NO_CLIP;
        }
        Ok(ImageDraw {
            slot: self.row(row::GRADIENT)[0] as u32,
            instance,
        })
    }

    /// What a layer record does when its layer is drawn over its parent. Its blur
    /// is `color.x` and its opacity `color.w`, its colour filter the three rows
    /// after the fill, its drop shadow an offset and deviation in `gradient` and
    /// a colour in `via`, and a mask is the kind in the first side-colour row,
    /// which is not read yet. Lengths are in device pixels already.
    pub fn layer(&self) -> Result<LayerFx, Unsupported> {
        if self.kind() != Kind::Layer {
            return Err(Unsupported::Kind(self.row(row::TYPES)[0]));
        }
        if self.row(row::SIDE_COLORS)[0] > 0.5 {
            return Err(Unsupported::LayerEffect);
        }
        let [blur, _, _, opacity] = self.row(row::FILL);
        let [x, y, deviation, _] = self.row(row::GRADIENT);
        let color = self.row(row::GRADIENT_VIA);
        Ok(LayerFx {
            blur,
            opacity,
            matrix: [
                self.row(row::FILL_END),
                self.row(row::BORDER),
                self.row(row::BORDER_COLOR),
            ],
            shadow: (color[3] > 0.0).then_some(DropShadow {
                offset: [x, y],
                deviation,
                color,
            }),
        })
    }
}

/// Whether a clip bound is a real clip and not the sentinel for none.
fn has_clip(bounds: [f32; 4]) -> bool {
    bounds[0] > -5000.0 && bounds[2] < 90000.0
}

/// `item` moved by `(dx, dy)`, into the frame of a texture that begins there.
fn moved(items: &[Item], dx: f32, dy: f32) -> Vec<Item> {
    let primitive = |p: &GpuPrimitive| {
        let mut p = *p;
        p.bounds[0] += dx;
        p.bounds[1] += dy;
        if has_clip(p.clip_bounds) {
            p.clip_bounds[0] += dx;
            p.clip_bounds[1] += dy;
        }
        // A gradient's geometry is in the same frame as the box; a glyph's is its
        // rect in the atlas, which stays where it is.
        let kind = p.type_info[0];
        let fill = p.type_info[1];
        if kind == PrimitiveType::Rect as u32 {
            if fill == FillType::LinearGradient as u32 {
                p.gradient_params[0] += dx;
                p.gradient_params[1] += dy;
                p.gradient_params[2] += dx;
                p.gradient_params[3] += dy;
            } else if fill == FillType::RadialGradient as u32 {
                p.gradient_params[0] += dx;
                p.gradient_params[1] += dy;
            }
        }
        p
    };
    items
        .iter()
        .map(|item| match item {
            Item::Primitives(run) => Item::Primitives(Run {
                primitives: run.primitives.iter().map(primitive).collect(),
                aux: run.aux.clone(),
            }),
            Item::Image(draw) => {
                let mut instance = draw.instance;
                instance.dst_rect[0] += dx;
                instance.dst_rect[1] += dy;
                if has_clip(instance.clip_bounds) {
                    instance.clip_bounds[0] += dx;
                    instance.clip_bounds[1] += dy;
                }
                Item::Image(ImageDraw {
                    slot: draw.slot,
                    instance,
                })
            }
            Item::Layer(layer) => Item::Layer(Layer {
                items: moved(&layer.items, dx, dy),
                fx: layer.fx,
                bounds: [
                    layer.bounds[0] + dx,
                    layer.bounds[1] + dy,
                    layer.bounds[2],
                    layer.bounds[3],
                ],
            }),
        })
        .collect()
}

/// Content textures are rounded up to this many pixels, so layers that differ a
/// little between frames share one.
const TEXTURE_STEP: u32 = 64;

/// Textures kept for the next layer.
const MAX_POOLED: usize = 8;

/// Clears `view` to transparent.
fn clear(device: &wgpu::Device, queue: &wgpu::Queue, view: &wgpu::TextureView) {
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("Display List Layer Clear"),
    });
    drop(encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("Display List Layer Clear"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
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
    }));
    queue.submit(std::iter::once(encoder.finish()));
}

/// Draws plans, keeping what drawing a layer's effects needs between them.
#[derive(Default)]
pub struct Painter {
    effects: Option<LayerEffects>,
    /// Content textures a layer is done with, to draw into again.
    textures: Vec<LayerTexture>,
}

impl Painter {
    /// Draws `plan` over `target`, `size` device pixels across and cleared to
    /// `clear` first, in paint order.
    ///
    /// `images` finds the texture of a slot. An image whose slot it does not know
    /// is not drawn, and its slot is returned.
    pub fn render<'i>(
        &mut self,
        renderer: &mut GpuRenderer,
        target: &wgpu::TextureView,
        size: (u32, u32),
        plan: &Plan,
        clear: [f64; 4],
        images: &dyn Fn(u32) -> Option<&'i GpuImage>,
    ) -> Vec<u32> {
        renderer.resize(size.0, size.1);
        renderer.render_with_clear(target, &PrimitiveBatch::new(), clear);
        let mut missing = Vec::new();
        self.draw(renderer, target, size, &plan.items, images, &mut missing);
        missing
    }

    /// A texture of exactly `size`, from the pool when one is free.
    ///
    /// Exactly, not at least: the renderer maps clip space onto the whole of a
    /// texture, which a larger one would stretch.
    fn take_texture(
        &mut self,
        device: &wgpu::Device,
        size: (u32, u32),
        format: wgpu::TextureFormat,
    ) -> LayerTexture {
        match self.textures.iter().position(|t| t.size == size) {
            Some(at) => self.textures.swap_remove(at),
            None => LayerTexture::new(device, size, format, false),
        }
    }

    fn draw<'i>(
        &mut self,
        renderer: &mut GpuRenderer,
        target: &wgpu::TextureView,
        size: (u32, u32),
        items: &[Item],
        images: &dyn Fn(u32) -> Option<&'i GpuImage>,
        missing: &mut Vec<u32>,
    ) {
        for item in items {
            match item {
                Item::Primitives(run) => {
                    let mut batch = PrimitiveBatch::new();
                    batch.primitives.extend_from_slice(&run.primitives);
                    batch.aux_data.extend_from_slice(&run.aux);
                    renderer.render_overlay(target, &batch);
                }
                Item::Image(draw) => match images(draw.slot) {
                    Some(image) => renderer.render_images(target, image.view(), &[draw.instance]),
                    None => missing.push(draw.slot),
                },
                Item::Layer(layer) => {
                    // The pixels the layer reaches, whole, on the target.
                    let [x, y, w, h] = layer.bounds;
                    let (x0, y0) = (x.floor().max(0.0), y.floor().max(0.0));
                    let (x1, y1) = (
                        (x + w).ceil().min(size.0 as f32),
                        (y + h).ceil().min(size.1 as f32),
                    );
                    if x1 <= x0 || y1 <= y0 {
                        continue;
                    }
                    let region = [x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32];
                    // Its content is drawn into a texture that covers just that, and the
                    // pixels a drop shadow is read from: the region shifted back by the
                    // shadow's offset. The renderer's viewport is set to the texture's size.
                    let mut content = [
                        region[0] as i64,
                        region[1] as i64,
                        (region[0] + region[2]) as i64,
                        (region[1] + region[3]) as i64,
                    ];
                    if let Some(shadow) = layer.fx.shadow {
                        let shift = shadow.offset.map(|o| (o + 0.5).floor() as i64);
                        content[0] = content[0].min(content[0] - shift[0]);
                        content[1] = content[1].min(content[1] - shift[1]);
                        content[2] = content[2].max(content[2] - shift[0]);
                        content[3] = content[3].max(content[3] - shift[1]);
                    }
                    let (cx0, cy0) = (content[0].max(0) as u32, content[1].max(0) as u32);
                    let (cx1, cy1) = (
                        (content[2].max(0) as u32).min(size.0),
                        (content[3].max(0) as u32).min(size.1),
                    );
                    let tight = (
                        ((cx1 - cx0).div_ceil(TEXTURE_STEP) * TEXTURE_STEP).min(size.0),
                        ((cy1 - cy0).div_ceil(TEXTURE_STEP) * TEXTURE_STEP).min(size.1),
                    );
                    let (device, queue) = (renderer.device_arc(), renderer.queue_arc());
                    let format = renderer.texture_format;
                    let texture = self.take_texture(&device, tight, format);
                    clear(&device, &queue, &texture.view);
                    renderer.resize(tight.0, tight.1);
                    let inner = moved(&layer.items, -(cx0 as f32), -(cy0 as f32));
                    self.draw(renderer, &texture.view, tight, &inner, images, missing);
                    renderer.resize(size.0, size.1);

                    let effects = self
                        .effects
                        .get_or_insert_with(|| LayerEffects::new(&device, format));
                    let frame = LayerFrame {
                        size: tight,
                        origin: [cx0, cy0],
                        region,
                    };
                    effects.composite(&device, &queue, &texture.view, target, frame, &layer.fx);
                    self.textures.push(texture);
                    let excess = self.textures.len().saturating_sub(MAX_POOLED);
                    self.textures.drain(..excess);
                }
            }
        }
    }
}

/// Draws `plan` once, with a [`Painter`] of its own. A layer with effects
/// builds its pipelines on every call, so draw frames with one `Painter`.
pub fn render<'i>(
    renderer: &mut GpuRenderer,
    target: &wgpu::TextureView,
    size: (u32, u32),
    plan: &Plan,
    clear: [f64; 4],
    images: &dyn Fn(u32) -> Option<&'i GpuImage>,
) -> Vec<u32> {
    Painter::default().render(renderer, target, size, plan, clear, images)
}
