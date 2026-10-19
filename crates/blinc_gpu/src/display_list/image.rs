//! Image resource metadata and ABI fitting, kept separate from clip geometry.
//!
//! `Painter::render` uses texture dimensions as intrinsic logical size and keeps
//! ordinary images stretched. Use `Painter::render_with_images` to supply
//! [`ImageResource`] with another intrinsic size, a legacy fit, or mask mode.
//! Explicit mappings in records override the resource's fit. Image slots name
//! whole textures; SDK-specific slot encodings are resolved by the caller.
//!
//! ```no_run
//! # use blinc_gpu::{GpuImage, GpuRenderer};
//! # use blinc_gpu::display_list::{image::{ImageFit, ImageResource}, plan::{Painter, Plan}};
//! # fn draw(painter: &mut Painter, renderer: &mut GpuRenderer, target: &wgpu::TextureView, plan: &Plan, image: &GpuImage) {
//! let resource = ImageResource {
//!     natural_size: [80.0, 40.0],
//!     fit: ImageFit::Contain,
//!     mask: false,
//!     ..ImageResource::new(image)
//! };
//! let missing = painter.render_with_images(renderer, target, (800, 600), plan,
//!     [0.0; 4], &|slot| (slot == 7).then_some(resource));
//! # }
//! ```

use super::{CLIP_LOCAL, CLIP_SCREEN, Kind, NO_CLIP, Placement, Record, Unsupported, row};
use crate::image::{GpuImage, GpuImageInstance};
use crate::renderer::GpuRenderer;
use std::borrow::Cow;

/// Object fit in logical units, including the producer's extended bitmap fits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum ImageFit {
    Cover = 0,
    Contain = 1,
    #[default]
    Fill = 2,
    Tile = 3,
    None = 4,
    ScaleDown = 5,
}
impl ImageFit {
    fn read(value: f32) -> Result<Self, Unsupported> {
        match value {
            0.0 => Ok(Self::Cover),
            1.0 => Ok(Self::Contain),
            2.0 => Ok(Self::Fill),
            3.0 => Ok(Self::Tile),
            4.0 => Ok(Self::None),
            5.0 => Ok(Self::ScaleDown),
            _ => Err(Unsupported::ImageMapping),
        }
    }
}

/// Consumer-owned texture and the logical size it represents. The texture may
/// have been rasterized at a higher pixel ratio than its intrinsic size.
#[derive(Clone, Copy)]
pub struct ImageResource<'a> {
    pub image: &'a GpuImage,
    pub natural_size: [f32; 2],
    /// Used for ordinary records; mapped records carry their own fit.
    pub fit: ImageFit,
    /// Use texture alpha as coverage in the record's text colour.
    pub mask: bool,
}
impl<'a> ImageResource<'a> {
    /// An ordinary colour texture stretched over its content box.
    pub fn new(image: &'a GpuImage) -> Self {
        Self {
            image,
            natural_size: [image.width() as f32, image.height() as f32],
            fit: ImageFit::Fill,
            mask: false,
        }
    }
}

/// Producer mapping before texture dimensions are known. Repeat bit 0 is X,
/// bit 1 is Y. Position is free-space fractions, then logical pixel offsets.
#[derive(Clone, Debug)]
pub struct ImageMapping {
    pub fit: ImageFit,
    pub position: [f32; 4],
    pub repeat: u8,
}

/// A consumer image at its paint position. Fitting never changes its clip frame.
#[derive(Clone, Debug)]
pub struct ImageDraw {
    pub slot: u32,
    pub instance: GpuImageInstance,
    /// Five clip header rows, then an optional shape payload.
    pub aux: Vec<[f32; 4]>,
    pub mapping: Option<ImageMapping>,
    /// Producer's text colour, with inherited opacity already in alpha.
    pub mask_tint: [f32; 4],
    /// Device pixels per logical unit in the quad's normalized affine frame.
    pub unit: f32,
}

/// The instance and its clip/mapping data, ready for the existing image pipeline.
pub struct ResolvedImage<'a> {
    pub instance: GpuImageInstance,
    pub aux: Cow<'a, [[f32; 4]]>,
}

impl Record<'_> {
    /// The image this record draws, at `scale` device pixels per logical pixel,
    /// over its content box. Its texture is the consumer's, found by
    /// [`ImageDraw::slot`]. Resolve fitting and resource metadata with
    /// [`ImageDraw::resolve`], then draw its instance and auxiliary rows with
    /// [`GpuRenderer::render_images_with_aux`]. A polygon needs the whole list;
    /// use [`Record::image_with_aux`] for it.
    pub fn image(&self, scale: f32) -> Result<ImageDraw, Unsupported> {
        self.image_with_aux(scale, &[])
    }

    /// Like [`Record::image`], with the whole list for polygon clip points.
    pub fn image_with_aux(&self, scale: f32, list: &[f32]) -> Result<ImageDraw, Unsupported> {
        if self.kind() != Kind::Image {
            return Err(Unsupported::Kind(self.row(row::TYPES)[0]));
        }
        if let Some(why) = self.unsupported() {
            return Err(why);
        }
        let mapping = match self.row(row::TYPES)[1] {
            0.0 => None,
            3.0 => {
                let repeat = self.row(row::FILL_END)[0];
                let position = self.row(row::RADII);
                if !repeat.is_finite()
                    || repeat.fract() != 0.0
                    || !(0.0..=3.0).contains(&repeat)
                    || position.iter().any(|v| !v.is_finite())
                {
                    return Err(Unsupported::ImageMapping);
                }
                Some(ImageMapping {
                    fit: ImageFit::read(self.row(row::FILL_END)[2])?,
                    position,
                    repeat: repeat as u8,
                })
            }
            _ => return Err(Unsupported::ImageMapping),
        };
        let Placement {
            bounds,
            local_affine,
            size_scale,
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
            instance.clip_bounds = self.row(row::CLIP).map(|v| v * scale);
            instance.clip_radius = radii.map(|v| v * scale);
        } else {
            instance.clip_bounds = NO_CLIP;
        }
        let shape = self.clip_rows(scale, list)?;
        let shaped_screen_clip = self.clip_flags() & CLIP_SCREEN != 0
            && instance.clip_radius.iter().any(|r| *r > 0.0)
            && self.row(row::TYPES)[3] != 1.0;
        if self.clip_flags() & CLIP_LOCAL == 0
            && self.row(row::FADE).iter().all(|f| *f <= 0.0)
            && shape.is_empty()
            && !shaped_screen_clip
        {
            return Ok(ImageDraw {
                slot: self.row(row::GRADIENT)[0] as u32,
                instance,
                aux: Vec::new(),
                mapping,
                mask_tint: self.row(row::FILL),
                unit: size_scale,
            });
        }
        let mut aux = vec![[0.0; 4]; 5];
        if self.clip_flags() & CLIP_LOCAL != 0 {
            aux[0] = self.row(row::SHADOW).map(|v| v * size_scale);
            aux[1] = self.row(row::SHADOW_COLOR).map(|v| v * size_scale);
        }
        aux[2] = self.row(row::FADE_RECT).map(|v| v * scale);
        aux[3] = self.row(row::FADE).map(|v| v * scale);
        let [a, b, c, d] = self.row(row::AFFINE);
        let det = (a * d - b * c).abs();
        let aa = if [a, b, c, d] == [1.0, 0.0, 0.0, 1.0] {
            0.75
        } else {
            0.25 * (a.hypot(c) + b.hypot(d)) / det.sqrt()
        };
        aux[4] = [
            self.row(row::TYPES)[3],
            aa,
            0.0,
            if shape.is_empty() { 0.0 } else { 6.0 },
        ];
        aux.extend(shape);
        instance.abi_clip[0] = 1.0;
        Ok(ImageDraw {
            slot: self.row(row::GRADIENT)[0] as u32,
            instance,
            aux,
            mapping,
            mask_tint: self.row(row::FILL),
            unit: size_scale,
        })
    }
}

impl ImageDraw {
    /// Resolve intrinsic geometry without moving the content quad or its clips.
    /// Mapping payloads follow all clip/shape data, so existing pointers stay valid.
    pub fn resolve(&self, resource: ImageResource<'_>) -> Result<ResolvedImage<'_>, Unsupported> {
        self.resolve_geometry(resource.natural_size, resource.fit, resource.mask)
    }

    fn resolve_geometry(
        &self,
        natural_size: [f32; 2],
        fit: ImageFit,
        mask: bool,
    ) -> Result<ResolvedImage<'_>, Unsupported> {
        let [sw, sh] = natural_size;
        if [sw, sh, self.unit]
            .iter()
            .any(|v| !v.is_finite() || *v <= 0.0)
        {
            return Err(Unsupported::ImageMapping);
        }
        let mut instance = self.instance;
        if mask {
            instance.tint = self.mask_tint;
            instance.params[1] = 1.0;
            instance.abi_clip[1] = 1.0;
        }
        let Some(mapping) = self.mapping.clone().or_else(|| {
            (fit != ImageFit::Fill).then_some(ImageMapping {
                fit,
                position: if fit == ImageFit::Tile {
                    [0.0; 4]
                } else {
                    [0.5, 0.5, 0.0, 0.0]
                },
                repeat: if fit == ImageFit::Tile { 3 } else { 0 },
            })
        }) else {
            return Ok(ResolvedImage {
                instance,
                aux: Cow::Borrowed(&self.aux),
            });
        };
        let [w, h] = [
            instance.dst_rect[2] / self.unit,
            instance.dst_rect[3] / self.unit,
        ];
        let contain = (w / sw).min(h / sh);
        let factor = match mapping.fit {
            ImageFit::Cover => (w / sw).max(h / sh),
            ImageFit::Contain => contain,
            ImageFit::ScaleDown => contain.min(1.0),
            _ => 1.0,
        };
        let [dw, dh] = if mapping.fit == ImageFit::Fill {
            [w, h]
        } else {
            [sw * factor, sh * factor]
        };
        let [px, py, ox, oy] = mapping.position;
        let rect = [(w - dw) * px + ox, (h - dh) * py + oy, dw, dh].map(|v| v * self.unit);
        if rect.iter().any(|v| !v.is_finite()) || dw <= 0.0 || dh <= 0.0 {
            return Err(Unsupported::ImageMapping);
        }
        let [a, b, c, d] = instance.transform;
        let aa = 0.25 * (a.hypot(c) + b.hypot(d)) / (a * d - b * c).abs();
        let mut aux = self.aux.clone();
        instance.abi_clip[2] = aux.len() as f32 + 1.0;
        aux.push(rect);
        aux.push([mapping.repeat as f32, aa, 0.0, 0.0]);
        Ok(ResolvedImage {
            instance,
            aux: Cow::Owned(aux),
        })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn both_mapped_image_shaders_validate() {
        for source in [
            include_str!(concat!(env!("OUT_DIR"), "/image.wgsl")),
            include_str!(concat!(env!("OUT_DIR"), "/image_dt.wgsl")),
        ] {
            let module = naga::front::wgsl::parse_str(source).expect("image WGSL parses");
            naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::empty(),
            )
            .validate(&module)
            .expect("image WGSL validates");
        }
    }
    use super::*;
    use crate::display_list::{DISPLAY_LIST_VERSION, records};
    fn fixture(bytes: &[u8]) -> ImageDraw {
        let list: Vec<_> = bytes
            .chunks_exact(4)
            .map(|v| f32::from_le_bytes(v.try_into().unwrap()))
            .collect();
        records(&list, 1, DISPLAY_LIST_VERSION)
            .unwrap()
            .next()
            .unwrap()
            .image_with_aux(1.0, &list)
            .unwrap()
    }
    #[test]
    fn cover_and_contain_resolve_free_space_position_and_pixel_offsets() {
        let cases = [
            (
                include_bytes!("../../tests/fixtures/display_list/mapping_cover_x1.f32").as_slice(),
                [-2.875, 1.625, 130.0, 65.0],
            ),
            (
                include_bytes!("../../tests/fixtures/display_list/mapping_contain_x1.f32")
                    .as_slice(),
                [-0.375, 10.375, 110.0, 55.0],
            ),
        ];
        for (bytes, want) in cases {
            let draw = fixture(bytes);
            let resolved = draw
                .resolve_geometry([80.0, 40.0], ImageFit::Fill, false)
                .unwrap();
            let index = resolved.instance.abi_clip[2] as usize - 1;
            for (a, b) in resolved.aux[index].iter().zip(want) {
                assert!(
                    (a - b).abs() < 1e-4,
                    "{:?} vs {want:?}",
                    resolved.aux[index]
                );
            }
            assert_eq!(resolved.instance.dst_rect, draw.instance.dst_rect);
        }
    }
    #[test]
    fn scale_down_keeps_small_images_at_intrinsic_size_and_shrinks_large_images() {
        let draw = fixture(include_bytes!(
            "../../tests/fixtures/display_list/mapping_down_x1.f32"
        ));
        for (natural, want) in [
            ([20.0, 12.0], [20.0, 12.0]),
            ([240.0, 180.0], [86.666664, 65.0]),
        ] {
            let out = draw
                .resolve_geometry(natural, ImageFit::Fill, false)
                .unwrap();
            let at = out.instance.abi_clip[2] as usize - 1;
            for (a, b) in out.aux[at][2..].iter().zip(want) {
                assert!((a - b).abs() < 1e-4);
            }
        }
        for natural in [[0.0, 1.0], [-1.0, 2.0], [f32::NAN, 2.0]] {
            assert!(matches!(
                draw.resolve_geometry(natural, ImageFit::Fill, false),
                Err(Unsupported::ImageMapping)
            ));
        }
    }
    #[test]
    fn masks_use_tint_alpha_once_and_plain_images_borrow_their_auxiliary_data() {
        let draw = fixture(include_bytes!(
            "../../tests/fixtures/display_list/mapping_mask_x1.f32"
        ));
        let out = draw
            .resolve_geometry([80.0, 40.0], ImageFit::Fill, true)
            .unwrap();
        assert_eq!(out.instance.tint, draw.mask_tint);
        assert_eq!(out.instance.params[1], 1.0);
        assert_eq!(out.instance.abi_clip[1], 1.0);
        assert!(matches!(out.aux, Cow::Borrowed(_)));
        assert_eq!(out.instance.abi_clip[2], 0.0);
    }
}
