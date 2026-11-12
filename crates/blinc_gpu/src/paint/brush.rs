//! Brush resolution: GPU colour and gradient parameters, opacity, and per-vertex gradient sampling.

use super::GpuPaintContext;
use crate::primitives::FillType;
use blinc_core::{Brush, Color, Point, Rect};

impl GpuPaintContext<'_> {
    /// Convert a Brush to GPU color components and gradient parameters
    /// Returns (color1, color2, gradient_params, fill_type)
    /// Note: Glass brushes are handled separately in fill methods - this returns transparent
    pub(super) fn brush_to_colors(
        &self,
        brush: &Brush,
    ) -> ([f32; 4], [f32; 4], [f32; 4], FillType) {
        let opacity = self.combined_opacity();
        match brush {
            Brush::Solid(color) => {
                let c = [color.r, color.g, color.b, color.a * opacity];
                // Default gradient params (not used for solid)
                (c, c, [0.0, 0.0, 1.0, 0.0], FillType::Solid)
            }
            Brush::Glass(_) => {
                // Glass is handled via glass primitives, not regular primitives
                // Return transparent as a fallback (should never be used)
                ([0.0; 4], [0.0; 4], [0.0, 0.0, 1.0, 0.0], FillType::Solid)
            }
            Brush::Image(_) => {
                // Image backgrounds are handled separately via the image pipeline
                // Return transparent as a fallback
                ([0.0; 4], [0.0; 4], [0.0, 0.0, 1.0, 0.0], FillType::Solid)
            }
            Brush::Blur(_) => {
                // Blur is handled via glass primitives, not regular primitives
                // Return transparent as a fallback (should never be used)
                ([0.0; 4], [0.0; 4], [0.0, 0.0, 1.0, 0.0], FillType::Solid)
            }
            Brush::Gradient(gradient) => {
                let (stops, fill_type, gradient_params) = match gradient {
                    blinc_core::Gradient::Linear {
                        start, end, stops, ..
                    } => {
                        // Linear gradient: (x1, y1, x2, y2) in user space
                        (
                            stops,
                            FillType::LinearGradient,
                            [start.x, start.y, end.x, end.y],
                        )
                    }
                    blinc_core::Gradient::Radial {
                        center,
                        radius,
                        stops,
                        ..
                    } => {
                        // Radial gradient: (cx, cy, radius, 0) in user space
                        (
                            stops,
                            FillType::RadialGradient,
                            [center.x, center.y, *radius, 0.0],
                        )
                    }
                    // Conic gradients treated as radial for now
                    blinc_core::Gradient::Conic { center, stops, .. } => (
                        stops,
                        FillType::RadialGradient,
                        [center.x, center.y, 100.0, 0.0],
                    ),
                };

                let (c1, c2) = if stops.len() >= 2 {
                    let s1 = &stops[0];
                    let s2 = &stops[stops.len() - 1];
                    (
                        [s1.color.r, s1.color.g, s1.color.b, s1.color.a * opacity],
                        [s2.color.r, s2.color.g, s2.color.b, s2.color.a * opacity],
                    )
                } else if !stops.is_empty() {
                    let c = &stops[0].color;
                    let arr = [c.r, c.g, c.b, c.a * opacity];
                    (arr, arr)
                } else {
                    ([1.0, 1.0, 1.0, opacity], [1.0, 1.0, 1.0, opacity])
                };

                (c1, c2, gradient_params, fill_type)
            }
        }
    }

    /// Transform gradient parameters by the current transform
    /// For linear gradients, transforms (x1, y1, x2, y2) to screen space
    /// For radial gradients, transforms (cx, cy, radius, 0) to screen space
    /// Convert ObjectBoundingBox gradient coords (0..1) to local rect pixel coords.
    pub(super) fn obb_to_rect_coords(
        brush: &Brush,
        params: [f32; 4],
        rect: Rect,
        fill_type: FillType,
    ) -> [f32; 4] {
        let is_obb = matches!(
            brush,
            Brush::Gradient(blinc_core::Gradient::Linear {
                space: blinc_core::GradientSpace::ObjectBoundingBox,
                ..
            }) | Brush::Gradient(blinc_core::Gradient::Radial {
                space: blinc_core::GradientSpace::ObjectBoundingBox,
                ..
            }) | Brush::Gradient(blinc_core::Gradient::Conic {
                space: blinc_core::GradientSpace::ObjectBoundingBox,
                ..
            })
        );
        if !is_obb || fill_type == FillType::Solid {
            return params;
        }
        let is_radial = fill_type == FillType::RadialGradient;
        if is_radial {
            [
                rect.x() + params[0] * rect.width(),
                rect.y() + params[1] * rect.height(),
                params[2] * rect.width().max(rect.height()),
                params[3],
            ]
        } else {
            [
                rect.x() + params[0] * rect.width(),
                rect.y() + params[1] * rect.height(),
                rect.x() + params[2] * rect.width(),
                rect.y() + params[3] * rect.height(),
            ]
        }
    }

    pub(super) fn transform_gradient_params(&self, params: [f32; 4], is_radial: bool) -> [f32; 4] {
        if is_radial {
            // Radial gradient: (cx, cy, radius, 0)
            let center = self.transform_point(Point::new(params[0], params[1]));
            // Scale radius by average scale factor
            let affine = self.current_affine();
            let a = affine.elements[0];
            let b = affine.elements[1];
            let c = affine.elements[2];
            let d = affine.elements[3];
            let scale_x = (a * a + b * b).sqrt();
            let scale_y = (c * c + d * d).sqrt();
            let avg_scale = (scale_x + scale_y) / 2.0;
            [center.x, center.y, params[2] * avg_scale, params[3]]
        } else {
            // Linear gradient: (x1, y1, x2, y2)
            let start = self.transform_point(Point::new(params[0], params[1]));
            let end = self.transform_point(Point::new(params[2], params[3]));
            [start.x, start.y, end.x, end.y]
        }
    }

    /// Apply opacity to a brush by modifying the color's alpha channel
    pub(super) fn apply_opacity_to_brush(brush: Brush, opacity: f32) -> Brush {
        if opacity >= 1.0 {
            return brush;
        }
        match brush {
            Brush::Solid(color) => {
                Brush::Solid(Color::rgba(color.r, color.g, color.b, color.a * opacity))
            }
            // For gradients, we'd need to modify each stop's color
            // For now, return as-is since SVGs typically use solid colors
            other => other,
        }
    }

    /// Pre-sample a multi-stop gradient at every tessellated vertex's
    /// path-local position. Returns `None` for solids, 2-stop
    /// gradients (the SDF shader's `mix(color, color2, t)` path
    /// handles them precisely per-fragment), or non-gradient
    /// brushes. Output aligns with `tessellated.vertices`.
    pub(super) fn sample_per_vertex_gradient(
        tessellated: &crate::path::TessellatedPath,
        brush: &Brush,
    ) -> Option<Vec<blinc_core::Color>> {
        let Brush::Gradient(gradient) = brush else {
            return None;
        };
        let stops = gradient.stops();
        if stops.len() <= 2 {
            return None;
        }
        // Multi-stop per-vertex sampling is gated behind an opt-in
        // env flag until the layer-ordering regression it introduced
        // in the interactive_volume scene is root-caused. Without
        // this gate, the default 2-stop `mix(first, last, t)`
        // fallback keeps rendering order identical to the
        // known-working baseline.
        if std::env::var("BLINC_MULTISTOP_VERTEX").ok().as_deref() != Some("1") {
            return None;
        }
        let sample = |p: [f32; 2]| -> blinc_core::Color {
            let t = match gradient {
                blinc_core::Gradient::Linear { start, end, .. } => {
                    let dx = end.x - start.x;
                    let dy = end.y - start.y;
                    let len_sq = dx * dx + dy * dy;
                    if len_sq > 1e-6 {
                        (((p[0] - start.x) * dx + (p[1] - start.y) * dy) / len_sq).clamp(0.0, 1.0)
                    } else {
                        0.0
                    }
                }
                blinc_core::Gradient::Radial { center, radius, .. } => {
                    let dx = p[0] - center.x;
                    let dy = p[1] - center.y;
                    let d = (dx * dx + dy * dy).sqrt();
                    (d / radius.max(1e-5)).clamp(0.0, 1.0)
                }
                blinc_core::Gradient::Conic { center, .. } => {
                    // Approximate conic as radial until we add conic shader
                    // support — mirrors `extract_brush_info`'s handling.
                    let dx = p[0] - center.x;
                    let dy = p[1] - center.y;
                    let d = (dx * dx + dy * dy).sqrt();
                    (d / 100.0).clamp(0.0, 1.0)
                }
            };
            Self::sample_stops(stops, t)
        };
        Some(
            tessellated
                .vertices
                .iter()
                .map(|v| sample(v.position))
                .collect(),
        )
    }

    /// Piecewise-linear interpolation through a sorted gradient stop
    /// list at parameter `t`. Clamps outside the first/last stop
    /// offsets. Mirrors `blinc_gpu::path::sample_gradient_stops`
    /// without re-exporting it.
    pub(super) fn sample_stops(stops: &[blinc_core::GradientStop], t: f32) -> blinc_core::Color {
        if stops.is_empty() {
            return blinc_core::Color::TRANSPARENT;
        }
        if t <= stops[0].offset {
            return stops[0].color;
        }
        let last = stops.len() - 1;
        if t >= stops[last].offset {
            return stops[last].color;
        }
        for i in 0..last {
            let s0 = &stops[i];
            let s1 = &stops[i + 1];
            if t >= s0.offset && t <= s1.offset {
                let range = s1.offset - s0.offset;
                if range < 1e-6 {
                    return s0.color;
                }
                let local = (t - s0.offset) / range;
                return blinc_core::Color {
                    r: s0.color.r + (s1.color.r - s0.color.r) * local,
                    g: s0.color.g + (s1.color.g - s0.color.g) * local,
                    b: s0.color.b + (s1.color.b - s0.color.b) * local,
                    a: s0.color.a + (s1.color.a - s0.color.a) * local,
                };
            }
        }
        stops[last].color
    }
}
