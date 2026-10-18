//! A display list as an ordered plan, and the plan drawn.
//!
//! Boxes, shadows and glyphs are primitives the SDF pipelines draw together.
//! An image is a texture the consumer owns, drawn by the image pipeline in a
//! pass of its own, and a layer is its own content drawn into a texture and
//! composited back. Paint order interleaves them, so a [`Plan`] keeps
//! consecutive primitives together, puts each image between them and nests
//! each layer's items inside it.

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

/// A group drawn into a layer of its own and faded as one, so what overlaps
/// inside it does not show through itself.
#[derive(Debug)]
pub struct Layer {
    pub items: Vec<Item>,
    pub opacity: f32,
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
/// A layer that does more than fade is not drawn as a layer: its content is
/// drawn as it would be without it, and its record is reported.
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
                    Ok(opacity) => parent.push(Item::Layer(Layer {
                        items: inner,
                        opacity,
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

    /// The opacity a layer record fades its layer by, when that is all it
    /// does. Its blur is `color.x`, its colour filter the three rows after the
    /// fill, its drop shadow's colour `via` and its mask the kind in the first
    /// of the side-colour rows.
    pub fn layer(&self) -> Result<f32, Unsupported> {
        if self.kind() != Kind::Layer {
            return Err(Unsupported::Kind(self.row(row::TYPES)[0]));
        }
        const IDENTITY: [[f32; 4]; 3] = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
        ];
        let [blur, _, _, alpha] = self.row(row::FILL);
        let filtered = (0..3).any(|i| self.row(row::FILL_END + i) != IDENTITY[i]);
        let shadowed = self.row(row::GRADIENT_VIA)[3] > 0.0;
        let masked = self.row(row::SIDE_COLORS)[0] > 0.5;
        if blur > 0.0 || filtered || shadowed || masked {
            return Err(Unsupported::LayerEffect);
        }
        Ok(alpha)
    }
}

/// Draws `plan` over `target`, `size` device pixels across and cleared to
/// `clear` first, in paint order.
///
/// `images` finds the texture of a slot. An image whose slot it does not know
/// is not drawn, and its slot is returned.
pub fn render<'i>(
    renderer: &mut GpuRenderer,
    target: &wgpu::TextureView,
    size: (u32, u32),
    plan: &Plan,
    clear: [f64; 4],
    images: &dyn Fn(u32) -> Option<&'i GpuImage>,
) -> Vec<u32> {
    renderer.render_with_clear(target, &PrimitiveBatch::new(), clear);
    let mut missing = Vec::new();
    draw(renderer, target, size, &plan.items, images, &mut missing);
    missing
}

fn draw<'i>(
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
                // Exactly the target's size: the renderer maps clip space onto the
                // whole texture, which a pooled texture rounded up would stretch.
                let texture =
                    LayerTexture::new(renderer.device(), size, renderer.texture_format, false);
                renderer.render_with_clear(&texture.view, &PrimitiveBatch::new(), [0.0; 4]);
                draw(renderer, &texture.view, size, &layer.items, images, missing);
                let mut encoder =
                    renderer
                        .device()
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                            label: Some("Display List Layer Composite"),
                        });
                renderer.composite_layer(
                    &mut encoder,
                    target,
                    &texture,
                    0.0,
                    0.0,
                    layer.opacity,
                    blinc_core::BlendMode::Normal,
                );
                renderer.queue().submit(std::iter::once(encoder.finish()));
            }
        }
    }
}
