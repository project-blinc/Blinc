//! A glyph atlas texture kept up to date from a display list's producer.
//!
//! `blinc_abi` hands out each atlas as a size and a sub-rectangle of bytes
//! that changed since a revision the consumer last saw, with the whole atlas
//! when it was resized or the history is gone. [`AtlasTexture`] applies those
//! updates; the renderer samples it through
//! [`GpuRenderer::set_glyph_atlas`](crate::GpuRenderer::set_glyph_atlas).

/// One update to an atlas, the fields of `blinc_abi`'s `AtlasInfo` and the
/// bytes it read: `update_width` by `update_height` texels at `(x, y)`.
#[derive(Clone, Copy, Debug)]
pub struct AtlasUpdate<'a> {
    /// The atlas's size.
    pub width: u32,
    pub height: u32,
    pub x: u32,
    pub y: u32,
    pub update_width: u32,
    pub update_height: u32,
    /// Row-major texels of the updated rectangle.
    pub bytes: &'a [u8],
}

/// Why an update could not be applied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AtlasError {
    /// The atlas changed size and the update is not the whole of it.
    NeedsFullUpload,
    /// The bytes are not the rectangle's size, or it lies outside the atlas.
    BadRect,
}

impl std::fmt::Display for AtlasError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NeedsFullUpload => write!(f, "the atlas was resized; upload all of it"),
            Self::BadRect => write!(f, "the update's bytes do not fill its rectangle"),
        }
    }
}

impl std::error::Error for AtlasError {}

/// A glyph atlas on the GPU: one byte of coverage per texel, or RGBA for colour glyphs.
pub struct AtlasTexture {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    size: (u32, u32),
    color: bool,
}

impl AtlasTexture {
    /// An empty atlas of one texel, which a renderer can bind before a first update.
    pub fn new(device: &wgpu::Device, color: bool) -> Self {
        let (texture, view) = create(device, color, (1, 1));
        Self {
            texture,
            view,
            size: (1, 1),
            color,
        }
    }

    pub fn view(&self) -> &wgpu::TextureView {
        &self.view
    }

    pub fn size(&self) -> (u32, u32) {
        self.size
    }

    fn bytes_per_texel(&self) -> u32 {
        if self.color { 4 } else { 1 }
    }

    /// Applies `update`. A change of size replaces the texture, which is then
    /// all of `update`; any other update writes only its rectangle.
    pub fn update(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        update: &AtlasUpdate<'_>,
    ) -> Result<(), AtlasError> {
        let full = update.x == 0
            && update.y == 0
            && update.update_width == update.width
            && update.update_height == update.height;
        let fits = update.x.checked_add(update.update_width) <= Some(update.width)
            && update.y.checked_add(update.update_height) <= Some(update.height);
        let want = update.update_width as usize
            * update.update_height as usize
            * self.bytes_per_texel() as usize;
        if !fits || update.bytes.len() != want || update.width == 0 || update.height == 0 {
            return Err(AtlasError::BadRect);
        }
        let size = (update.width, update.height);
        if size != self.size {
            if !full {
                return Err(AtlasError::NeedsFullUpload);
            }
            (self.texture, self.view) = create(device, self.color, size);
            self.size = size;
        }
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: update.x,
                    y: update.y,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            update.bytes,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(update.update_width * self.bytes_per_texel()),
                rows_per_image: None,
            },
            wgpu::Extent3d {
                width: update.update_width,
                height: update.update_height,
                depth_or_array_layers: 1,
            },
        );
        Ok(())
    }
}

/// The formats the renderer's own placeholder atlases use, so either binds in their place.
fn create(
    device: &wgpu::Device,
    color: bool,
    size: (u32, u32),
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(if color {
            "Display List Color Glyph Atlas"
        } else {
            "Display List Glyph Atlas"
        }),
        size: wgpu::Extent3d {
            width: size.0,
            height: size.1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: if color {
            wgpu::TextureFormat::Rgba8UnormSrgb
        } else {
            wgpu::TextureFormat::R8Unorm
        },
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}
