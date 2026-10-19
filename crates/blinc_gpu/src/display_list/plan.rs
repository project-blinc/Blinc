//! A display list as an ordered plan, and the plan drawn.
//!
//! Boxes, shadows and glyphs are primitives the SDF pipelines draw together.
//! An image is a texture the consumer owns, drawn by the image pipeline in a
//! pass of its own, and a layer is its own content drawn into a texture and
//! composited back. Paint order interleaves them, so a [`Plan`] keeps
//! consecutive primitives together, puts each image or canvas between them and nests
//! each layer's items inside it.

use super::backdrop::{Backdrop, BackdropPass};
use super::canvas::{CanvasDraw, CanvasRegistry};
use super::effects::{DropShadow, LayerEffects, LayerFrame, LayerFx, LayerMask, MAX_MASK_STOPS};
use super::{DecodeError, Kind, Record, Unsupported, records, row};
use crate::image::GpuImage;
use crate::primitives::{FillType, GpuPrimitive, PrimitiveBatch, PrimitiveType};
use crate::renderer::{GpuRenderer, LayerTexture};

pub use super::image::ImageDraw;
use super::image::ImageResource;

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
    /// Gradient stop tables and shape clips, addressed by the primitives.
    pub aux: Vec<[f32; 4]>,
}

impl Run {
    /// `more` after this, its auxiliary pointers rebased to follow this run.
    fn append(&mut self, mut more: Run) {
        let base = self.aux.len() as f32;
        for p in &mut more.primitives {
            if p.gradient_table[0] > 0.5 {
                p.gradient_table[0] += base;
            }
            if p.clip_shape[0] > 0.5 {
                p.clip_shape[0] += base;
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
    Backdrop(Backdrop),
    Canvas(CanvasDraw),
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
/// A layer whose mask has more stops than `MAX_MASK_STOPS` is not drawn as a layer:
/// its content is drawn as it would be without it, and its record is reported.
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
                match record.layer(floats) {
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
            Kind::Backdrop => {
                let page = &mut open.last_mut().expect("the page is open").items;
                match record.backdrop(scale, floats) {
                    Ok(draw) => page.push(Item::Backdrop(draw)),
                    Err(why) => plan.unsupported.push((index, why)),
                }
            }
            Kind::Canvas => {
                let page = &mut open.last_mut().expect("the page is open").items;
                match record.canvas(scale, floats) {
                    Ok(draw) => page.push(Item::Canvas(draw)),
                    Err(why) => plan.unsupported.push((index, why)),
                }
            }
            Kind::Image => {
                let page = &mut open.last_mut().expect("the page is open").items;
                match record.image_with_aux(scale, floats) {
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
    /// What a layer record does when its layer is drawn over its parent. Its blur
    /// is `color.x` and its opacity `color.w`, its colour filter the three rows
    /// after the fill, its drop shadow an offset and deviation in `gradient` and
    /// a colour in `via`, and its mask a gradient in the rows a box keeps its side
    /// colours in. Lengths are in device pixels already. `list` is the whole display
    /// list, for a mask's table of stops after the records.
    pub fn layer(&self, list: &[f32]) -> Result<LayerFx, Unsupported> {
        if self.kind() != Kind::Layer {
            return Err(Unsupported::Kind(self.row(row::TYPES)[0]));
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
            mask: self.mask(list)?,
        })
    }

    /// The mask of a layer record: a gradient of alpha in the element's box.
    ///
    /// Its first row has the kind and the alpha of its first, middle and last stops, then
    /// its geometry, the element's box and the transform's 2 x 2. The offsets of those stops
    /// are in the row of gradient offsets, whose last is 1 for a middle stop, or the place of
    /// a table of stops after the records when there are more, they repeat or it is conic.
    fn mask(&self, list: &[f32]) -> Result<Option<LayerMask>, Unsupported> {
        let [kind, first_alpha, middle_alpha, last_alpha] = self.row(row::SIDE_COLORS);
        if kind < 0.5 {
            return Ok(None);
        }
        let [first, middle, last, marker] = self.row(row::GRADIENT_OFFSETS);
        let mut stops = [[0.0; 2]; MAX_MASK_STOPS];
        let (spread, count) = if marker >= 2.0 {
            let at = (marker - 2.0) as usize * 4;
            let count = *list.get(at).ok_or(Unsupported::LayerEffect)? as usize;
            let end = at + (2 + 2 * count) * 4;
            if count == 0 || count > MAX_MASK_STOPS || end > list.len() {
                return Err(Unsupported::LayerEffect);
            }
            for (i, stop) in stops.iter_mut().take(count).enumerate() {
                // A stop is a colour row, whose alpha is the mask's, then its offset.
                *stop = [list[at + (3 + 2 * i) * 4], list[at + (2 + 2 * i) * 4 + 3]];
            }
            (list[at + 1], count)
        } else if marker >= 0.5 {
            stops[..3].copy_from_slice(&[
                [first, first_alpha],
                [middle, middle_alpha],
                [last, last_alpha],
            ]);
            (0.0, 3)
        } else {
            stops[..2].copy_from_slice(&[[first, first_alpha], [last, last_alpha]]);
            (0.0, 2)
        };
        let translate = self.row(row::RADII);
        Ok(Some(LayerMask {
            kind,
            geometry: self.row(row::SIDE_COLORS + 1),
            frame: self.row(row::SIDE_COLORS + 2),
            matrix: self.row(row::SIDE_COLORS + 3),
            translate: [translate[0], translate[1], translate[2]],
            spread,
            stops,
            count: count as u32,
        }))
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
        if p.clip_fade_bounds[2] > 0.0 {
            p.clip_fade_bounds[0] += dx;
            p.clip_fade_bounds[1] += dy;
        }
        // A gradient's geometry is in the same frame as the box; a glyph's is its
        // rect in the atlas, which stays where it is.
        let kind = p.type_info[0];
        let fill = p.type_info[1];
        if kind == PrimitiveType::Rect as u32 || kind == PrimitiveType::Notch as u32 {
            if fill == FillType::LinearGradient as u32 {
                p.gradient_params[0] += dx;
                p.gradient_params[1] += dy;
                p.gradient_params[2] += dx;
                p.gradient_params[3] += dy;
            } else if fill == FillType::RadialGradient as u32
                || fill == FillType::ConicGradient as u32
            {
                p.gradient_params[0] += dx;
                p.gradient_params[1] += dy;
            }
        }
        p
    };
    items
        .iter()
        .map(|item| match item {
            Item::Primitives(run) => {
                let mut aux = run.aux.clone();
                let mut shifted = std::collections::BTreeSet::new();
                for p in &run.primitives {
                    if p.clip_shape[0] > 0.5 {
                        let at = p.clip_shape[0] as usize - 1;
                        if shifted.insert(at) {
                            super::clip::translate(&mut aux[at..], dx, dy);
                        }
                    }
                }
                Item::Primitives(Run {
                    primitives: run.primitives.iter().map(primitive).collect(),
                    aux,
                })
            }
            Item::Image(draw) => {
                let mut instance = draw.instance;
                instance.dst_rect[0] += dx;
                instance.dst_rect[1] += dy;
                if has_clip(instance.clip_bounds) {
                    instance.clip_bounds[0] += dx;
                    instance.clip_bounds[1] += dy;
                }
                let mut aux = draw.aux.clone();
                if instance.abi_clip[0] > 0.5 {
                    let base = instance.abi_clip[0] as usize - 1;
                    aux[base + 2][0] += dx;
                    aux[base + 2][1] += dy;
                    let shape = aux[base + 4][3];
                    if shape > 0.5 {
                        super::clip::translate(&mut aux[shape as usize - 1..], dx, dy);
                    }
                }
                Item::Image(ImageDraw {
                    instance,
                    aux,
                    slot: draw.slot,
                    mapping: draw.mapping.clone(),
                    mask_tint: draw.mask_tint,
                    unit: draw.unit,
                })
            }
            Item::Backdrop(draw) => Item::Backdrop(draw.moved(dx, dy)),
            Item::Canvas(draw) => Item::Canvas(draw.moved(dx, dy)),
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

fn has_backdrop(items: &[Item]) -> bool {
    items.iter().any(|item| match item {
        Item::Backdrop(_) => true,
        Item::Layer(layer) => has_backdrop(&layer.items),
        _ => false,
    })
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

/// Draws plans, retaining canvas callbacks, layer and backdrop resources between frames.
#[derive(Default)]
pub struct Painter {
    pub(super) canvases: CanvasRegistry,
    effects: Option<LayerEffects>,
    backdrop: Option<BackdropPass>,
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
        self.render_with_images(renderer, target, size, plan, clear, &|slot| {
            images(slot).map(ImageResource::new)
        })
    }

    /// Draw with resource metadata for intrinsic size, legacy fitting and masks.
    /// Unknown or invalid image resources are skipped and their slots returned.
    pub fn render_with_images<'i>(
        &mut self,
        renderer: &mut GpuRenderer,
        target: &wgpu::TextureView,
        size: (u32, u32),
        plan: &Plan,
        clear: [f64; 4],
        images: &dyn Fn(u32) -> Option<ImageResource<'i>>,
    ) -> Vec<u32> {
        self.canvases.missing.clear();
        renderer.resize(size.0, size.1);
        let mut missing = Vec::new();
        if has_backdrop(&plan.items) {
            let (device, queue) = (renderer.device_arc(), renderer.queue_arc());
            let texture = self.take_texture(&device, size, renderer.texture_format);
            renderer.render_with_clear(&texture.view, &PrimitiveBatch::new(), clear);
            self.draw(
                renderer,
                &texture.view,
                size,
                &plan.items,
                images,
                &mut missing,
            );
            let pass = self
                .backdrop
                .get_or_insert_with(|| BackdropPass::new(&device, renderer.texture_format));
            pass.copy(&device, &queue, &texture.view, target, size);
            self.textures.push(texture);
            self.textures
                .drain(..self.textures.len().saturating_sub(MAX_POOLED));
        } else {
            renderer.render_with_clear(target, &PrimitiveBatch::new(), clear);
            self.draw(renderer, target, size, &plan.items, images, &mut missing);
        }
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
        images: &dyn Fn(u32) -> Option<ImageResource<'i>>,
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
                    Some(resource) => match draw.resolve(resource) {
                        Ok(resolved) => renderer.render_images_with_aux(
                            target,
                            resource.image.view(),
                            &[resolved.instance],
                            &resolved.aux,
                        ),
                        Err(_) => missing.push(draw.slot),
                    },
                    None => missing.push(draw.slot),
                },
                Item::Canvas(draw) => {
                    let (device, queue) = (renderer.device_arc(), renderer.queue_arc());
                    self.canvases.paint(
                        &device,
                        &queue,
                        renderer.texture_format,
                        target,
                        size,
                        draw,
                    );
                }
                Item::Backdrop(draw) => {
                    let (device, queue) = (renderer.device_arc(), renderer.queue_arc());
                    self.backdrop
                        .get_or_insert_with(|| BackdropPass::new(&device, renderer.texture_format))
                        .draw(&device, &queue, target, size, draw);
                }
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
                    // A backdrop's sampling root is the whole layer, even outside its visible bounds.
                    if has_backdrop(&layer.items) {
                        content = [0, 0, size.0 as i64, size.1 as i64];
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

/// Draws `plan` once, with a [`Painter`] of its own. Effect pipelines are rebuilt
/// on each call; keep one `Painter` across frames to reuse them and register canvases.
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
