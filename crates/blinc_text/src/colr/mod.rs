//! Rasterize COLRv1 paint graphs into the existing colour glyph atlas.

mod gradient;

use crate::{
    font::FontFace,
    rasterizer::{GlyphFormat, RasterizedGlyph},
};
use tiny_skia::{
    BlendMode, FillRule, Mask, Path, PathBuilder, Pixmap, PixmapPaint, Point, Rect, Transform,
};
use ttf_parser::{
    Face, GlyphId, OutlineBuilder, RgbaColor, Tag,
    colr::{ClipBox, CompositeMode, Paint, Painter},
};

pub(crate) fn rasterize(font: &FontFace, glyph: u16, size: f32) -> Option<RasterizedGlyph> {
    let mut face = font.as_ttf_face()?;
    let table = face.raw_face().table(Tag::from_bytes(b"COLR"))?;
    if table.get(..2)? != [0, 1] || !size.is_finite() || size <= 0.0 {
        return None;
    }
    if let Some(weight) = font.variation_weight() {
        face.set_variation(Tag::from_bytes(b"wght"), weight);
    }
    let mut painter = GlyphPainter::new(&face);
    face.paint_color_glyph(
        GlyphId(glyph),
        0,
        RgbaColor::new(0, 0, 0, 255),
        &mut painter,
    )?;
    let bounds = painter.bounds?;
    let scale = size / face.units_per_em() as f32;
    let left = (bounds.left() * scale).floor() - 1.0;
    let top = (bounds.bottom() * scale).ceil() + 1.0;
    let width = ((bounds.right() * scale).ceil() + 1.0 - left) as u32;
    let height = (top - (bounds.top() * scale).floor() + 1.0) as u32;
    if width == 0 || height == 0 || width > 4096 || height > 4096 {
        return None;
    }
    let base = Transform::from_row(scale, 0.0, 0.0, -scale, -left, top);
    let mut painter = GlyphPainter::new(&face);
    painter.raster = Some(Raster {
        pixmap: Pixmap::new(width, height)?,
        base,
        masks: Vec::new(),
        layers: Vec::new(),
    });
    face.paint_color_glyph(
        GlyphId(glyph),
        0,
        RgbaColor::new(0, 0, 0, 255),
        &mut painter,
    )?;
    let mut bitmap = painter.raster?.pixmap.take();
    // Atlas samples use straight RGBA; tiny-skia composes premultiplied pixels.
    for pixel in bitmap.as_chunks_mut::<4>().0 {
        if pixel[3] > 0 {
            for i in 0..3 {
                pixel[i] = ((pixel[i] as u32 * 255 + pixel[3] as u32 / 2) / pixel[3] as u32)
                    .min(255) as u8;
            }
        }
    }
    Some(RasterizedGlyph {
        bitmap,
        width,
        height,
        bearing_x: left as i16,
        bearing_y: top as i16,
        advance: (face.glyph_hor_advance(GlyphId(glyph)).unwrap_or(0) as f32 * scale).round()
            as u16,
        format: GlyphFormat::Rgba,
    })
}

struct Raster {
    pixmap: Pixmap,
    base: Transform,
    masks: Vec<Mask>,
    layers: Vec<(Pixmap, BlendMode)>,
}

struct GlyphPainter<'face, 'data> {
    face: &'face Face<'data>,
    transform: Transform,
    transforms: Vec<Transform>,
    outline: Option<Path>,
    clips: Vec<Option<Rect>>,
    bounds: Option<Rect>,
    raster: Option<Raster>,
}

impl<'face, 'data> GlyphPainter<'face, 'data> {
    fn new(face: &'face Face<'data>) -> Self {
        Self {
            face,
            transform: Transform::identity(),
            transforms: Vec::new(),
            outline: None,
            clips: Vec::new(),
            bounds: None,
            raster: None,
        }
    }

    fn push_shape(&mut self, path: Option<Path>) {
        let rect = path.as_ref().map(Path::bounds);
        let rect = match self.clips.last() {
            Some(parent) => parent.and_then(|p| rect.and_then(|r| p.intersect(&r))),
            None => rect,
        };
        self.clips.push(rect);
        if let Some(raster) = &mut self.raster {
            let mut mask = raster.masks.last().cloned().unwrap_or_else(|| {
                let mut mask = Mask::new(raster.pixmap.width(), raster.pixmap.height()).unwrap();
                mask.data_mut().fill(255);
                mask
            });
            if let Some(path) = path {
                mask.intersect_path(&path, FillRule::Winding, true, raster.base);
            } else {
                mask.data_mut().fill(0);
            }
            raster.masks.push(mask);
        }
    }
}

impl<'data> Painter<'data> for GlyphPainter<'_, 'data> {
    fn outline_glyph(&mut self, glyph: GlyphId) {
        let mut builder = Builder(PathBuilder::new());
        self.face.outline_glyph(glyph, &mut builder);
        self.outline = builder
            .0
            .finish()
            .and_then(|path| path.transform(self.transform));
    }
    fn push_clip(&mut self) {
        self.push_shape(self.outline.clone());
    }
    fn push_clip_box(&mut self, clip: ClipBox) {
        let path = Rect::from_ltrb(clip.x_min, clip.y_min, clip.x_max, clip.y_max)
            .map(PathBuilder::from_rect)
            .and_then(|path| path.transform(self.transform));
        self.push_shape(path);
    }
    fn pop_clip(&mut self) {
        self.clips.pop();
        if let Some(raster) = &mut self.raster {
            raster.masks.pop();
        }
    }
    fn push_transform(&mut self, t: ttf_parser::Transform) {
        self.transforms.push(self.transform);
        self.transform = self
            .transform
            .pre_concat(Transform::from_row(t.a, t.b, t.c, t.d, t.e, t.f));
    }
    fn pop_transform(&mut self) {
        if let Some(t) = self.transforms.pop() {
            self.transform = t;
        }
    }
    fn push_layer(&mut self, mode: CompositeMode) {
        if let Some(raster) = &mut self.raster {
            let next = Pixmap::new(raster.pixmap.width(), raster.pixmap.height()).unwrap();
            let previous = std::mem::replace(&mut raster.pixmap, next);
            raster.layers.push((previous, blend(mode)));
        }
    }
    fn pop_layer(&mut self) {
        if let Some(raster) = &mut self.raster
            && let Some((mut previous, mode)) = raster.layers.pop()
        {
            previous.draw_pixmap(
                0,
                0,
                raster.pixmap.as_ref(),
                &PixmapPaint {
                    blend_mode: mode,
                    ..Default::default()
                },
                Transform::identity(),
                None,
            );
            raster.pixmap = previous;
        }
    }
    fn paint(&mut self, paint: Paint<'data>) {
        let Some(bounds) = self.clips.last().copied().flatten() else {
            return;
        };
        self.bounds = match self.bounds {
            Some(b) => Rect::from_ltrb(
                b.left().min(bounds.left()),
                b.top().min(bounds.top()),
                b.right().max(bounds.right()),
                b.bottom().max(bounds.bottom()),
            ),
            None => Some(bounds),
        };
        let brush = gradient::Brush::new(paint, self.face.variation_coordinates());
        let Some(raster) = &mut self.raster else {
            return;
        };
        let Some(inverse) = raster.base.pre_concat(self.transform).invert() else {
            return;
        };
        let mut fill = Pixmap::new(raster.pixmap.width(), raster.pixmap.height()).unwrap();
        let width = fill.width() as usize;
        for (i, pixel) in fill
            .data_mut()
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .enumerate()
        {
            let mut point = Point::from_xy((i % width) as f32 + 0.5, (i / width) as f32 + 0.5);
            inverse.map_point(&mut point);
            pixel.copy_from_slice(&brush.sample(point.x, point.y));
        }
        raster.pixmap.draw_pixmap(
            0,
            0,
            fill.as_ref(),
            &PixmapPaint::default(),
            Transform::identity(),
            raster.masks.last(),
        );
    }
}

struct Builder(PathBuilder);
impl OutlineBuilder for Builder {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.move_to(x, y);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0.line_to(x, y);
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.0.quad_to(x1, y1, x, y);
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.0.cubic_to(x1, y1, x2, y2, x, y);
    }
    fn close(&mut self) {
        self.0.close();
    }
}

fn blend(mode: CompositeMode) -> BlendMode {
    match mode {
        CompositeMode::Clear => BlendMode::Clear,
        CompositeMode::Source => BlendMode::Source,
        CompositeMode::Destination => BlendMode::Destination,
        CompositeMode::SourceOver => BlendMode::SourceOver,
        CompositeMode::DestinationOver => BlendMode::DestinationOver,
        CompositeMode::SourceIn => BlendMode::SourceIn,
        CompositeMode::DestinationIn => BlendMode::DestinationIn,
        CompositeMode::SourceOut => BlendMode::SourceOut,
        CompositeMode::DestinationOut => BlendMode::DestinationOut,
        CompositeMode::SourceAtop => BlendMode::SourceAtop,
        CompositeMode::DestinationAtop => BlendMode::DestinationAtop,
        CompositeMode::Xor => BlendMode::Xor,
        CompositeMode::Plus => BlendMode::Plus,
        CompositeMode::Screen => BlendMode::Screen,
        CompositeMode::Overlay => BlendMode::Overlay,
        CompositeMode::Darken => BlendMode::Darken,
        CompositeMode::Lighten => BlendMode::Lighten,
        CompositeMode::ColorDodge => BlendMode::ColorDodge,
        CompositeMode::ColorBurn => BlendMode::ColorBurn,
        CompositeMode::HardLight => BlendMode::HardLight,
        CompositeMode::SoftLight => BlendMode::SoftLight,
        CompositeMode::Difference => BlendMode::Difference,
        CompositeMode::Exclusion => BlendMode::Exclusion,
        CompositeMode::Multiply => BlendMode::Multiply,
        CompositeMode::Hue => BlendMode::Hue,
        CompositeMode::Saturation => BlendMode::Saturation,
        CompositeMode::Color => BlendMode::Color,
        CompositeMode::Luminosity => BlendMode::Luminosity,
    }
}
