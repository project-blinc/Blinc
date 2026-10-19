//! Layer mask ownership, record tables and reusable GPU upload.
use super::{Kind, Record, Unsupported, row};

/// Stops kept inline in the layer uniforms; larger masks use a texture.
pub const MAX_MASK_STOPS: usize = 16;

/// A layer's mask: a gradient measured in the element's box, whose alpha, at each
/// pixel, the layer's is multiplied by.
#[derive(Clone, Debug, PartialEq)]
pub struct LayerMask {
    /// 1 linear, 2 radial, 3 conic.
    pub kind: f32,
    /// Linear: from `(x, y)` to `(z, w)`; radial: the centre and radius `z`; conic: the
    /// centre and start angle `z`. In fractions of the box.
    pub geometry: [f32; 4],
    /// The element's box in layout units, in the frame its transform maps.
    pub frame: [f32; 4],
    /// The transform's 2 x 2 part, which takes the box to the target.
    pub matrix: [f32; 4],
    /// Its translation, and the display's scale.
    pub translate: [f32; 3],
    /// 0 pad, 1 repeat, 2 reflect.
    pub spread: f32,
    /// The stops, an offset and an alpha, of which the first `count` are used.
    pub stops: std::sync::Arc<[[f32; 2]]>,
    pub count: u32,
}

impl LayerMask {
    /// Move the target's pixel origin without moving the mask's element frame.
    pub(super) fn translate_by(&mut self, dx: f32, dy: f32) {
        let scale = self.translate[2].max(0.0001);
        self.translate[0] += dx / scale;
        self.translate[1] += dy / scale;
    }
}

impl Record<'_> {
    /// The mask of a layer record: a gradient of alpha in the element's box.
    ///
    /// Its first row has the kind and the alpha of its first, middle and last stops, then
    /// its geometry, the element's box and the transform's 2 x 2. The offsets of those stops
    /// are in the row of gradient offsets, whose last is 1 for a middle stop, or the place of
    /// a table of stops after the records when there are more, they repeat or it is conic.
    pub(super) fn mask(&self, list: &[f32]) -> Result<Option<LayerMask>, Unsupported> {
        let [kind, first_alpha, middle_alpha, last_alpha] = self.row(row::SIDE_COLORS);
        if kind < 0.5 {
            return Ok(None);
        }
        let [first, middle, last, marker] = self.row(row::GRADIENT_OFFSETS);
        if !marker.is_finite() || marker < 0.0 || marker.fract() != 0.0 {
            return Err(Unsupported::LayerEffect);
        }
        let mut stops = Vec::new();
        let (spread, count) = if marker >= 2.0 {
            let at = ((marker - 2.0) as usize)
                .checked_mul(4)
                .ok_or(Unsupported::LayerEffect)?;
            let count = *list.get(at).ok_or(Unsupported::LayerEffect)?;
            if !count.is_finite() || count < 1.0 || count.fract() != 0.0 || count >= u32::MAX as f32
            {
                return Err(Unsupported::LayerEffect);
            }
            let count = count as usize;
            let len = count
                .checked_mul(8)
                .and_then(|n| n.checked_add(8))
                .ok_or(Unsupported::LayerEffect)?;
            let table = list
                .get(at..at.checked_add(len).ok_or(Unsupported::LayerEffect)?)
                .ok_or(Unsupported::LayerEffect)?;
            for rows in table[8..].chunks_exact(8) {
                stops.push([rows[4], rows[3]]);
            }
            (list[at + 1], count)
        } else if marker >= 0.5 {
            stops.extend_from_slice(&[
                [first, first_alpha],
                [middle, middle_alpha],
                [last, last_alpha],
            ]);
            (0.0, 3)
        } else {
            stops.extend_from_slice(&[[first, first_alpha], [last, last_alpha]]);
            (0.0, 2)
        };
        if ![1.0, 2.0, 3.0].contains(&kind)
            || ![0.0, 1.0, 2.0].contains(&spread)
            || stops.iter().flatten().any(|v| !v.is_finite())
        {
            return Err(Unsupported::LayerEffect);
        }
        let translate = self.row(row::RADII);
        Ok(Some(LayerMask {
            kind,
            geometry: self.row(row::SIDE_COLORS + 1),
            frame: self.row(row::SIDE_COLORS + 2),
            matrix: self.row(row::SIDE_COLORS + 3),
            translate: [translate[0], translate[1], translate[2]],
            spread,
            stops: stops.into(),
            count: count as u32,
        }))
    }
}

/// Two offset/alpha pairs per texel, addressed across rows without filtering.
pub(super) struct MaskTable {
    texture: wgpu::Texture,
    pub(super) view: wgpu::TextureView,
    size: (u32, u32),
    upload: Vec<[f32; 4]>,
}
impl MaskTable {
    pub(super) fn new(device: &wgpu::Device) -> Self {
        Self::with_size(device, (1, 1))
    }
    fn with_size(device: &wgpu::Device, size: (u32, u32)) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Display List Mask Stops"),
            size: wgpu::Extent3d {
                width: size.0,
                height: size.1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Self {
            texture,
            view,
            size,
            upload: Vec::new(),
        }
    }
    pub(super) fn update(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        stops: &[[f32; 2]],
    ) {
        if stops.len() <= MAX_MASK_STOPS {
            return;
        }
        let texels = stops.len().div_ceil(2) as u32;
        let width = texels
            .next_power_of_two()
            .min(1024)
            .min(device.limits().max_texture_dimension_2d)
            .max(self.size.0);
        let height = texels.div_ceil(width).next_power_of_two().max(self.size.1);
        if (width, height) != self.size {
            let upload = std::mem::take(&mut self.upload);
            *self = Self::with_size(device, (width, height));
            self.upload = upload;
        }
        let rows = texels.div_ceil(width);
        self.upload.clear();
        self.upload.resize((width * rows) as usize, [0.0; 4]);
        for (i, pair) in stops.chunks(2).enumerate() {
            let b = pair.get(1).copied().unwrap_or_default();
            self.upload[i] = [pair[0][0], pair[0][1], b[0], b[1]];
        }
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytemuck::cast_slice(&self.upload),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 16),
                rows_per_image: None,
            },
            wgpu::Extent3d {
                width,
                height: rows,
                depth_or_array_layers: 1,
            },
        );
    }
}
