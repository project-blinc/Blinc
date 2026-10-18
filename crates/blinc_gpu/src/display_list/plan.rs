//! A display list as an ordered plan, and the plan drawn.
//!
//! Boxes, shadows and glyphs are primitives the SDF pipelines draw together.
//! An image is a texture the consumer owns, drawn by the image pipeline in a
//! pass of its own. Paint order interleaves the two, so a [`Plan`] keeps
//! consecutive primitives together and puts each image between them.

use super::{
    CLIP_SCREEN, DecodeError, Kind, NO_CLIP, Placement, Record, Unsupported, records, row,
};
use crate::image::{GpuImage, GpuImageInstance};
use crate::primitives::{GpuPrimitive, PrimitiveBatch};
use crate::renderer::GpuRenderer;

/// An image to draw: the consumer's resource for `slot`, over a box.
#[derive(Clone, Copy, Debug)]
pub struct ImageDraw {
    /// The consumer's name for the image, set by whoever laid the scene out.
    pub slot: u32,
    pub instance: GpuImageInstance,
}

/// One step of a plan.
#[derive(Debug)]
pub enum Item {
    /// Primitives in paint order, drawn together.
    Primitives(Vec<GpuPrimitive>),
    Image(ImageDraw),
}

/// A display list in paint order, and the records that had no place in it.
#[derive(Debug, Default)]
pub struct Plan {
    pub items: Vec<Item>,
    /// The index of each record left out, and why.
    pub unsupported: Vec<(usize, Unsupported)>,
}

/// Plans `count` records at `scale` device pixels per logical pixel.
pub fn plan(floats: &[f32], count: usize, version: u32, scale: f32) -> Result<Plan, DecodeError> {
    let mut plan = Plan::default();
    for (index, record) in records(floats, count, version)?.enumerate() {
        let item = if record.kind() == Kind::Image {
            record.image(scale).map(Item::Image).map_err(Some)
        } else {
            record
                .to_primitive(scale)
                .map(|p| Item::Primitives(vec![p]))
                .map_err(Some)
        };
        match item {
            Ok(Item::Primitives(mut one)) => match plan.items.last_mut() {
                Some(Item::Primitives(run)) => run.append(&mut one),
                _ => plan.items.push(Item::Primitives(one)),
            },
            Ok(image) => plan.items.push(image),
            Err(why) => plan
                .unsupported
                .push((index, why.expect("an error names its cause"))),
        }
    }
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
}

/// Draws `plan` over `target`, cleared to `clear` first, in paint order.
///
/// `images` finds the texture of a slot. An image whose slot it does not know
/// is not drawn, and its slot is returned.
pub fn render<'i>(
    renderer: &mut GpuRenderer,
    target: &wgpu::TextureView,
    plan: &Plan,
    clear: [f64; 4],
    images: &dyn Fn(u32) -> Option<&'i GpuImage>,
) -> Vec<u32> {
    renderer.render_with_clear(target, &PrimitiveBatch::new(), clear);
    let mut missing = Vec::new();
    for item in &plan.items {
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
        }
    }
    missing
}
