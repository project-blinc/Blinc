//! Collected text keeps geometry and a push-time frame, never foreign aux offsets.
use super::{
    Arc, DrawContext, GpuGlyph, GpuPaintContext, PrimitiveBatch, Rect, RenderContext, TextElement,
    Transform3DLayerInfo, cached_text,
};
use blinc_core::{Affine2D, ClipPath, ClipShape, Path, PathCommand, Transform};

pub(super) struct CollectedTextPath {
    path: Path,
    frame: [f32; 6],
    layer: Option<blinc_layout::LayoutNodeId>,
}

/// Glyphs are already shaped at the layer-local origin. Clips keep their owner
/// frame; CSS glyph transforms are conjugated into that same local viewport.
pub(super) fn prepare_layer_glyphs(
    text: &TextElement,
    mut glyphs: Vec<GpuGlyph>,
    size: (u32, u32),
    info: &Transform3DLayerInfo,
    dpi: f32,
) -> PrimitiveBatch {
    let [lx, ly, ..] = info.layer_bounds;
    let scissor = text.clip_bounds.map(|[x, y, w, h]| [x - lx, y - ly, w, h]);
    for glyph in &mut glyphs {
        if let Some(clip) = scissor {
            glyph.clip_bounds = clip;
        }
    }
    let mut owned = if let Some(path) = &text.path_clip {
        path.in_layer(info).prepare(&glyphs, scissor, size)
    } else {
        let mut batch = PrimitiveBatch::new();
        batch.glyphs = glyphs;
        batch
    };
    if let Some([a, b, c, d, tx, ty]) = text.css_affine {
        cached_text::convert_affine(
            &mut owned,
            [
                a,
                b,
                c,
                d,
                tx + (a * lx + c * ly - lx) / dpi,
                ty + (b * lx + d * ly - ly) / dpi,
            ],
            dpi,
            text.z_index,
        );
    }
    owned
}

pub(super) fn prepare_glyphs(
    text: &TextElement,
    mut glyphs: Vec<GpuGlyph>,
    size: (u32, u32),
) -> PrimitiveBatch {
    if let Some(clip) = text.clip_bounds {
        for glyph in &mut glyphs {
            glyph.clip_bounds = clip;
        }
    }
    if let Some(path) = &text.path_clip {
        path.prepare(&glyphs, text.clip_bounds, size)
    } else {
        let mut owned = PrimitiveBatch::new();
        owned.glyphs = glyphs;
        owned
    }
}

impl RenderContext {
    pub(super) fn render_owned_text(&mut self, target: &wgpu::TextureView, batch: &PrimitiveBatch) {
        if let (Some(atlas), Some(color)) =
            (self.text_ctx.atlas_view(), self.text_ctx.color_atlas_view())
        {
            self.renderer
                .render_text_batch(target, batch, atlas, color, self.text_ctx.sampler());
        }
    }
}

impl CollectedTextPath {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn capture(
        clip: &ClipPath,
        bounds: [f32; 4],
        css: Option<[f32; 6]>,
        motion_scale: (f32, f32),
        motion_translate: (f32, f32),
        motion_center: Option<(f32, f32)>,
        scale: f32,
        layer: Option<blinc_layout::LayoutNodeId>,
    ) -> Option<Arc<Self>> {
        let [x, y, w, h] = bounds;
        let points: Vec<_> = match clip {
            ClipPath::Path { vertices } => vertices.clone(),
            ClipPath::Polygon { points } => points
                .iter()
                .map(|(x, y)| (x.resolve(w), y.resolve(h)))
                .collect(),
            _ => return None,
        };
        let mut path = Path::new();
        if points.len() >= 3 {
            path = path.move_to(points[0].0, points[0].1);
            for &(x, y) in &points[1..] {
                path = path.line_to(x, y);
            }
            path = path.close();
        }
        let [a, b, c, d, tx, ty] = css.unwrap_or([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
        let (sx, sy) = motion_scale;
        let (mx, my) = motion_translate;
        let (cx, cy) = motion_center.unwrap_or((0.0, 0.0));
        Some(Arc::new(Self {
            path,
            layer,
            frame: [
                a * sx,
                b * sy,
                c * sx,
                d * sy,
                (a * x + c * y + tx) * sx + cx * (1.0 - sx) + mx,
                (b * x + d * y + ty) * sy + cy * (1.0 - sy) + my,
            ]
            .map(|v| v * scale),
        }))
    }

    fn in_layer(&self, info: &Transform3DLayerInfo) -> Self {
        let [lx, ly, ..] = info.layer_bounds;
        let mut frame = self.frame;
        frame[4] -= lx;
        frame[5] -= ly;
        if self.layer == Some(info.node_id) {
            return Self {
                path: self.path.clone(),
                frame,
                layer: self.layer,
            };
        }
        // An outside ancestor clips in screen space after projection. Pull its
        // polygon back onto the layer plane so the existing contour reader can
        // apply it in the flat bake without making that ancestor tilt.
        let [a, b, c, d, tx, ty] = self.frame;
        let mut path = Path::new();
        for command in self.path.commands() {
            path = match command {
                PathCommand::MoveTo(p) | PathCommand::LineTo(p) => {
                    let Some([x, y]) =
                        layer_point(info, [a * p.x + c * p.y + tx, b * p.x + d * p.y + ty])
                    else {
                        return Self {
                            path: Path::new(),
                            frame: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
                            layer: self.layer,
                        };
                    };
                    if matches!(command, PathCommand::MoveTo(_)) {
                        path.move_to(x, y)
                    } else {
                        path.line_to(x, y)
                    }
                }
                PathCommand::Close => path.close(),
                _ => unreachable!("collected CSS paths contain only polygon vertices"),
            };
        }
        Self {
            path,
            frame: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            layer: self.layer,
        }
    }

    pub(super) fn prepare(
        &self,
        glyphs: &[GpuGlyph],
        scissor: Option<[f32; 4]>,
        size: (u32, u32),
    ) -> PrimitiveBatch {
        let mut paint = GpuPaintContext::new(size.0 as f32, size.1 as f32);
        paint.push_transform(Transform::Affine2D(Affine2D {
            elements: self.frame,
        }));
        paint.push_clip(ClipShape::Path(self.path.clone()));
        paint.pop_transform();
        if let Some([x, y, w, h]) = scissor {
            paint.push_clip(ClipShape::rect(Rect::new(x, y, w, h)));
        }
        paint.push_prepared_glyphs(glyphs);
        paint.take_batch()
    }
}

fn layer_point(info: &Transform3DLayerInfo, point: [f32; 2]) -> Option<[f32; 2]> {
    let [lx, ly, lw, lh] = info.layer_bounds.map(f64::from);
    let t = info.transform_3d;
    let (sx, cx, sy, cy, d) = (
        f64::from(t.sin_rx),
        f64::from(t.cos_rx),
        f64::from(t.sin_ry),
        f64::from(t.cos_ry),
        f64::from(t.perspective_d),
    );
    let u = f64::from(point[0]) - lx - lw * 0.5;
    let v = f64::from(point[1]) - ly - lh * 0.5;
    let (x, y) = if d > 0.0 {
        let (a, b, c, e) = (
            cy - u * cx * sy / d,
            -u * sx / d,
            -sx * sy - v * cx * sy / d,
            cx - v * sx / d,
        );
        let det = a * e - b * c;
        if !det.is_finite() || det.abs() <= 1e-12 {
            return None;
        }
        let (x, y) = ((u * e - b * v) / det, (a * v - u * c) / det);
        if 1.0 + (sx * y + cx * sy * x) / d <= 0.0 {
            return None;
        }
        (x, y)
    } else {
        (u, v)
    };
    let p = [(x + lw * 0.5) as f32, (y + lh * 0.5) as f32];
    p.iter().all(|v| v.is_finite()).then_some(p)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn layer_text_clip_projection_roundtrip_and_degenerate_frames() {
        blinc_theme::ThemeState::init(
            blinc_theme::HybridTheme::bundle(),
            blinc_theme::ColorScheme::Light,
        );
        let tree = blinc_layout::RenderTree::from_element(&blinc_layout::div().w(140.0).h(90.0));
        let mut info = Transform3DLayerInfo {
            node_id: tree.root().unwrap(),
            layer_bounds: [17.25, 29.5, 140.5, 90.25],
            transform_3d: blinc_core::Transform3DParams {
                sin_rx: 0.0,
                cos_rx: 1.0,
                sin_ry: 0.0,
                cos_ry: 1.0,
                perspective_d: 500.0,
            },
            opacity: 1.0,
        };
        for (rx, ry) in [
            (0.0_f32, 0.0_f32),
            (20.0, 35.0),
            (-25.0, -40.0),
            (15.0, 120.0),
        ] {
            let t = &mut info.transform_3d;
            (t.sin_rx, t.cos_rx) = rx.to_radians().sin_cos();
            (t.sin_ry, t.cos_ry) = ry.to_radians().sin_cos();
            let [lx, ly, w, h] = info.layer_bounds;
            for [x, y] in [[0.0, 0.0], [w, 0.0], [0.0, h], [w, h], [w * 0.5, h * 0.5]] {
                let t = info.transform_3d;
                let (u, v) = (x - w * 0.5, y - h * 0.5);
                let z = u * t.sin_ry;
                let denom = 1.0 + (v * t.sin_rx + z * t.cos_rx) / t.perspective_d;
                let screen = [
                    lx + w * 0.5 + u * t.cos_ry / denom,
                    ly + h * 0.5 + (v * t.cos_rx - z * t.sin_rx) / denom,
                ];
                let actual = layer_point(&info, screen).unwrap();
                assert!(
                    (actual[0] - x).abs() < 0.001 && (actual[1] - y).abs() < 0.001,
                    "{actual:?} expected {:?}",
                    [x, y]
                );
            }
        }
        let clip = ClipPath::Path {
            vertices: vec![(0.0, 0.0), (50.0, 0.0), (0.0, 60.0)],
        };
        let shape = CollectedTextPath::capture(
            &clip,
            [30.5, 40.25, 100.0, 80.0],
            None,
            (1.0, 1.0),
            (0.0, 0.0),
            None,
            1.0,
            Some(info.node_id),
        )
        .unwrap();
        let local = shape.in_layer(&info);
        let g = GpuGlyph {
            bounds: [0.0, 0.0, 20.0, 20.0],
            ..Default::default()
        };
        let rows = local.prepare(&[g], None, (141, 91)).aux_data;
        assert_eq!(&rows[3][..2], &[13.25, 10.75]);
        assert_eq!(shape.frame[4..], [30.5, 40.25]);
        let outside = CollectedTextPath::capture(
            &clip,
            [30.5, 40.25, 100.0, 80.0],
            None,
            (1.0, 1.0),
            (0.0, 0.0),
            None,
            1.0,
            None,
        )
        .unwrap();
        let rows = outside
            .in_layer(&info)
            .prepare(&[g], None, (141, 91))
            .aux_data;
        assert_eq!(&rows[3][..2], &layer_point(&info, [30.5, 40.25]).unwrap());
        assert!(layer_point(&info, [f32::NAN, 0.0]).is_none());
        info.transform_3d = blinc_core::Transform3DParams {
            sin_rx: 0.0,
            cos_rx: 1.0,
            sin_ry: 1.0,
            cos_ry: 0.0,
            perspective_d: 500.0,
        };
        assert!(layer_point(&info, [87.5, 74.625]).is_none());
    }
}
