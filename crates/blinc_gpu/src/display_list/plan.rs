//! A display list as an ordered plan, and the plan drawn.
//!
//! Boxes, shadows and glyphs are primitives the SDF pipelines draw together.
//! An image is a texture the consumer owns, drawn by the image pipeline in a
//! pass of its own, and a layer is its own content drawn into a texture and
//! composited back. Paint order interleaves them, so a [`Plan`] keeps
//! consecutive primitives together, puts each image between them and nests
//! each layer's items inside it.

use super::effects::{DropShadow, LayerEffects, LayerFx};
use super::{
    CLIP_SCREEN, DecodeError, Kind, NO_CLIP, Placement, Record, Unsupported, records, row,
};
use crate::image::{GpuImage, GpuImageInstance};
use crate::primitives::{GpuPrimitive, PrimitiveBatch};
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
#[derive(Debug)]
pub struct Layer {
    pub items: Vec<Item>,
    pub fx: LayerFx,
}

/// One step of a plan.
#[derive(Debug)]
pub enum Item {
    /// Primitives in paint order, drawn together.
    Primitives(Vec<GpuPrimitive>),
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
        (Some(Item::Primitives(run)), Item::Primitives(mut more)) => run.append(&mut more),
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
                    Ok(fx) => parent.push(Item::Layer(Layer { items: inner, fx })),
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
                match record.to_primitive(scale) {
                    Ok(p) => push(page, Item::Primitives(vec![p])),
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

/// Draws plans, keeping what drawing a layer's effects needs between them.
#[derive(Default)]
pub struct Painter {
    effects: Option<LayerEffects>,
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
        renderer.render_with_clear(target, &PrimitiveBatch::new(), clear);
        let mut missing = Vec::new();
        self.draw(renderer, target, size, &plan.items, images, &mut missing);
        missing
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
                    batch.primitives.extend_from_slice(run);
                    renderer.render_overlay(target, &batch);
                }
                Item::Image(draw) => match images(draw.slot) {
                    Some(image) => renderer.render_images(target, image.view(), &[draw.instance]),
                    None => missing.push(draw.slot),
                },
                Item::Layer(layer) => {
                    let (device, queue) = (renderer.device_arc(), renderer.queue_arc());
                    let format = renderer.texture_format;
                    // Exactly the target's size: the renderer maps clip space onto the
                    // whole texture, which a pooled texture rounded up would stretch.
                    let texture = LayerTexture::new(&device, size, format, false);
                    renderer.render_with_clear(&texture.view, &PrimitiveBatch::new(), [0.0; 4]);
                    self.draw(renderer, &texture.view, size, &layer.items, images, missing);

                    let rows = (layer.fx.blur > 0.0)
                        .then(|| LayerTexture::new(&device, size, format, false));
                    let shadow = layer
                        .fx
                        .shadow
                        .map(|_| LayerTexture::new(&device, size, format, false));
                    let effects = self
                        .effects
                        .get_or_insert_with(|| LayerEffects::new(&device, format));
                    effects.composite(
                        &device,
                        &queue,
                        &texture.view,
                        [
                            rows.as_ref().map_or(&texture.view, |t| &t.view),
                            shadow.as_ref().map_or(&texture.view, |t| &t.view),
                        ],
                        target,
                        &layer.fx,
                    );
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
