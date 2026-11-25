//! Clip stack: pushing clips and resolving them into per-primitive clip data.

use super::aux_data::PolygonClipData;
use super::{ClipData, GpuPaintContext};
use crate::primitives::ClipType;
use blinc_core::{Affine2D, ClipShape, CornerRadius, FillRule, Point, Rect};

/// Set in a polygon clip's packed vertex count when it is filled even-odd.
const POLYGON_EVEN_ODD: u32 = 1 << 31;

impl GpuPaintContext<'_> {
    /// Get the current clip fade from the clip stack
    /// Returns the topmost non-zero fade, scaled by DPI
    pub(super) fn get_clip_fade(&self) -> [f32; 4] {
        for (_clip, _poly_meta, fade) in self.clip_stack.iter().rev() {
            if fade[0] > 0.0 || fade[1] > 0.0 || fade[2] > 0.0 || fade[3] > 0.0 {
                // Fade distances are in CSS pixels; scale by transform
                let affine = self.current_affine();
                let sx = (affine.elements[0] * affine.elements[0]
                    + affine.elements[1] * affine.elements[1])
                    .sqrt();
                let sy = (affine.elements[2] * affine.elements[2]
                    + affine.elements[3] * affine.elements[3])
                    .sqrt();
                return [
                    fade[0] * sy, // top
                    fade[1] * sx, // right
                    fade[2] * sy, // bottom
                    fade[3] * sx, // left
                ];
            }
        }
        [0.0; 4]
    }

    /// Transform a clip shape by the current transform
    /// Note: For rotated transforms, this computes the axis-aligned bounding box
    pub(super) fn transform_clip_shape(&self, shape: ClipShape) -> ClipShape {
        let affine = self.current_affine();

        // Check if this is identity transform (common case)
        if affine == Affine2D::IDENTITY {
            return shape;
        }

        // Uniform scale factor extracted from the affine — captures DPI
        // scaling (Retina = ~2.0) plus any uniform element scale. Used
        // by the polygon branch below to bring element-local vertices
        // into physical pixels without applying rotation.
        let [a, b, c, d, ..] = affine.elements;
        let det = a * d - b * c;
        let uniform_scale = det.abs().sqrt().max(1e-6);

        match shape {
            ClipShape::Rect(rect) => {
                // Transform all four corners and compute AABB
                let corners = [
                    Point::new(rect.x(), rect.y()),
                    Point::new(rect.x() + rect.width(), rect.y()),
                    Point::new(rect.x() + rect.width(), rect.y() + rect.height()),
                    Point::new(rect.x(), rect.y() + rect.height()),
                ];

                let transformed: Vec<Point> =
                    corners.iter().map(|p| self.transform_point(*p)).collect();

                let min_x = transformed
                    .iter()
                    .map(|p| p.x)
                    .fold(f32::INFINITY, f32::min);
                let max_x = transformed
                    .iter()
                    .map(|p| p.x)
                    .fold(f32::NEG_INFINITY, f32::max);
                let min_y = transformed
                    .iter()
                    .map(|p| p.y)
                    .fold(f32::INFINITY, f32::min);
                let max_y = transformed
                    .iter()
                    .map(|p| p.y)
                    .fold(f32::NEG_INFINITY, f32::max);

                ClipShape::Rect(Rect::new(min_x, min_y, max_x - min_x, max_y - min_y))
            }
            ClipShape::RoundedRect {
                rect,
                corner_radius,
                corner_shape,
            } => {
                // Transform corners and compute AABB
                let corners = [
                    Point::new(rect.x(), rect.y()),
                    Point::new(rect.x() + rect.width(), rect.y()),
                    Point::new(rect.x() + rect.width(), rect.y() + rect.height()),
                    Point::new(rect.x(), rect.y() + rect.height()),
                ];

                let transformed: Vec<Point> =
                    corners.iter().map(|p| self.transform_point(*p)).collect();

                let min_x = transformed
                    .iter()
                    .map(|p| p.x)
                    .fold(f32::INFINITY, f32::min);
                let max_x = transformed
                    .iter()
                    .map(|p| p.x)
                    .fold(f32::NEG_INFINITY, f32::max);
                let min_y = transformed
                    .iter()
                    .map(|p| p.y)
                    .fold(f32::INFINITY, f32::min);
                let max_y = transformed
                    .iter()
                    .map(|p| p.y)
                    .fold(f32::NEG_INFINITY, f32::max);

                // Scale the corner radii by the average scale factor
                let a = affine.elements[0];
                let b = affine.elements[1];
                let c = affine.elements[2];
                let d = affine.elements[3];
                let scale_x = (a * a + b * b).sqrt();
                let scale_y = (c * c + d * d).sqrt();
                let avg_scale = (scale_x + scale_y) * 0.5;

                let scaled_radius = CornerRadius::new(
                    corner_radius.top_left * avg_scale,
                    corner_radius.top_right * avg_scale,
                    corner_radius.bottom_right * avg_scale,
                    corner_radius.bottom_left * avg_scale,
                );

                ClipShape::RoundedRect {
                    rect: Rect::new(min_x, min_y, max_x - min_x, max_y - min_y),
                    corner_radius: scaled_radius,
                    corner_shape,
                }
            }
            ClipShape::Circle { center, radius } => {
                let transformed_center = self.transform_point(center);

                // For non-uniform scale, circle becomes ellipse - compute approximate radius
                let a = affine.elements[0];
                let b = affine.elements[1];
                let c = affine.elements[2];
                let d = affine.elements[3];
                let scale_x = (a * a + b * b).sqrt();
                let scale_y = (c * c + d * d).sqrt();

                if (scale_x - scale_y).abs() < 0.001 {
                    // Uniform scale - keep as circle
                    ClipShape::Circle {
                        center: transformed_center,
                        radius: radius * scale_x,
                    }
                } else {
                    // Non-uniform scale - convert to ellipse
                    ClipShape::Ellipse {
                        center: transformed_center,
                        radii: blinc_core::Vec2::new(radius * scale_x, radius * scale_y),
                    }
                }
            }
            ClipShape::Ellipse { center, radii } => {
                let transformed_center = self.transform_point(center);

                let a = affine.elements[0];
                let b = affine.elements[1];
                let c = affine.elements[2];
                let d = affine.elements[3];
                let scale_x = (a * a + b * b).sqrt();
                let scale_y = (c * c + d * d).sqrt();

                ClipShape::Ellipse {
                    center: transformed_center,
                    radii: blinc_core::Vec2::new(radii.x * scale_x, radii.y * scale_y),
                }
            }
            ClipShape::Path(path) => {
                // Raw-image path clips capture their push-time affine separately.
                ClipShape::Path(path)
            }
            ClipShape::Polygon(pts) => {
                // Polygon vertices stay in element-local coords (no
                // rotation / translation) so the fragment shader's
                // `sp - prim.bounds.xy` test rotates with the element —
                // any rotation reflected in prim.rotation / local_affine
                // (CSS, motion-binding spring, timeline updated by
                // apply_binding_deltas) naturally rotates the polygon.
                //
                // BUT prim.bounds.xy is in physical pixels (DPI-scaled),
                // so element-local coords need to be in physical pixels
                // too — otherwise on Retina (2x) a 24-px logical element
                // would have local_p ranging 0..48 but vertices 0..24,
                // clipping the arc to the top-left quarter and making
                // most of the ring invisible. Apply only the uniform
                // (DPI) scale; skip the rotation portion of the affine.
                let scale = uniform_scale;
                let scaled: Vec<Point> = pts
                    .iter()
                    .map(|p| Point::new(p.x * scale, p.y * scale))
                    .collect();
                ClipShape::Polygon(scaled)
            }
        }
    }

    /// Get clip data from the current clip stack.
    ///
    /// Returns `(clip_bounds, clip_radius, clip_corner_shape, clip_type)`.
    /// `clip_corner_shape` is a per-corner superellipse `n` parameter
    /// matching the [`CornerShape`] encoding (n=1.0 = round, 2.0 =
    /// squircle, 0.0 = bevel, 100.0 = square, -1.0 = scoop). When the
    /// active rounded clip was pushed with
    /// [`ClipShape::rounded_rect_shaped`], the shader's clip
    /// evaluator uses this to call `sd_shaped_rect` so the clip
    /// curve matches the parent's fill — otherwise the clip stays
    /// circular (default `[1.0; 4]`).
    ///
    /// For multiple rect clips, computes the intersection of all
    /// clips. For mixed clip types, uses the topmost clip
    /// (conservative approximation). Stacked RoundedRect clips with
    /// different `corner_shape` values currently collapse to the
    /// last-seen shape — typical UI use is a single squircle parent
    /// pushing `overflow:clip`, where this is exact.
    ///
    /// Corner radius handling: a rectangular clip (non-rounded) will
    /// reset the corner radius to 0 for any corners it covers, so a
    /// child with overflow_clip() doesn't inherit rounded corners
    /// from a parent.
    pub(super) fn get_clip_data(&mut self) -> ([f32; 4], [f32; 4], [f32; 4], ClipType) {
        self.get_clip_data_with_path_scissor(false, false)
    }

    /// `skip_inner` leaves out the inner clips, which the primitive then has as a shape clip.
    pub(super) fn get_clip_data_with_path_scissor(
        &mut self,
        keep_path_scissor: bool,
        skip_inner: bool,
    ) -> ([f32; 4], [f32; 4], [f32; 4], ClipType) {
        // P4.3 Option B clip-aware bake: while inside the OUTERMOST
        // composite layer, ancestor clips below the snapshot base
        // are stripped from per-primitive clip rects. They get
        // captured separately and re-applied at blit time by
        // `composite_*_layers_overlay` (the rounded radius is
        // threaded through via `current_clip_rounded` so flex-
        // siblings sitting inside a rounded-rect parent get their
        // scissor rounded too). Only clips pushed AFTER
        // `push_composite_layer` (i.e. intrinsic clips the subtree
        // itself emits, like an inner overflow_clip on a descendant)
        // participate in the per-primitive clip rect.
        let start = self.composite_layer_clip_base.first().copied().unwrap_or(0);
        let active_clips = &self.clip_stack[start..];
        if active_clips.is_empty() {
            // No clip - use large bounds
            return (
                [-10000.0, -10000.0, 100000.0, 100000.0],
                [0.0; 4],
                [1.0; 4],
                ClipType::None,
            );
        }

        // Try to compute intersection of all rect clips
        // Start with very large bounds
        let mut intersect_min_x = f32::NEG_INFINITY;
        let mut intersect_min_y = f32::NEG_INFINITY;
        let mut intersect_max_x = f32::INFINITY;
        let mut intersect_max_y = f32::INFINITY;
        let mut has_rect_clips = false;

        // Track whether the topmost clip is a plain Rect (not rounded)
        let mut topmost_is_plain_rect = false;

        for (clip, meta, _fade) in active_clips {
            if skip_inner && matches!(meta, Some(ClipData::Inner(_))) {
                continue;
            }
            match clip {
                ClipShape::Rect(rect) => {
                    // Intersect with this rect
                    intersect_min_x = intersect_min_x.max(rect.x());
                    intersect_min_y = intersect_min_y.max(rect.y());
                    intersect_max_x = intersect_max_x.min(rect.x() + rect.width());
                    intersect_max_y = intersect_max_y.min(rect.y() + rect.height());
                    has_rect_clips = true;
                    topmost_is_plain_rect = true;
                }
                ClipShape::RoundedRect { rect, .. } => {
                    // Intersect with this rect
                    intersect_min_x = intersect_min_x.max(rect.x());
                    intersect_min_y = intersect_min_y.max(rect.y());
                    intersect_max_x = intersect_max_x.min(rect.x() + rect.width());
                    intersect_max_y = intersect_max_y.min(rect.y() + rect.height());
                    has_rect_clips = true;
                    topmost_is_plain_rect = false;
                }
                // For non-rect clips, fall back to topmost-only behavior
                ClipShape::Circle { .. }
                | ClipShape::Ellipse { .. }
                | ClipShape::Path(_)
                | ClipShape::Polygon(_) => {
                    // Can't easily intersect with circles/ellipses/paths/polygons
                    // Fall through to use the topmost clip
                    topmost_is_plain_rect = false;
                }
            }
        }

        // Find the topmost non-rect clip anywhere in the stack (circle,
        // ellipse, polygon, path). When one exists it becomes the shape
        // clip for the primitive and any rect clips contribute only to
        // the scissor bounds. Previously this scanned only
        // `clip_stack.last()`, so pushing a rect clip on top of a
        // polygon (e.g. a precomp's inner canvas rect stacked on a
        // matte polygon) silently dropped the polygon — which is what
        // made `blinc_lottie`'s track mattes look unclipped when the
        // matted layer was a precomp. The GPU still only evaluates one
        // non-rect shape per primitive, so we stop at the first one
        // found walking top-down.
        let topmost_non_rect_idx = active_clips.iter().rposition(|(c, _, _)| {
            matches!(
                c,
                ClipShape::Circle { .. }
                    | ClipShape::Ellipse { .. }
                    | ClipShape::Polygon(_)
                    | ClipShape::Path(_)
            )
        });
        let topmost_is_non_rect = topmost_non_rect_idx.is_some();

        // If we have rect clips AND the topmost clip is rect-based, use the intersection
        if has_rect_clips && !topmost_is_non_rect {
            let width = (intersect_max_x - intersect_min_x).max(0.0);
            let height = (intersect_max_y - intersect_min_y).max(0.0);

            // Each corner of the intersection takes the largest radius among the rounded clips
            // whose own corner reaches it. A clip whose corner lies elsewhere has no say, however
            // large its radius, so a list inside a rounder frame keeps its own corners. The
            // superellipse `n` comes from the clip the radius came from.
            let mut radii = [0.0f32; 4];
            let mut corner_shape = [1.0f32; 4];
            for (clip, meta, _fade) in active_clips {
                if skip_inner && matches!(meta, Some(ClipData::Inner(_))) {
                    continue;
                }
                let ClipShape::RoundedRect {
                    rect,
                    corner_radius,
                    corner_shape: shape,
                } = clip
                else {
                    continue;
                };
                // How far the intersection's edges lie inside this clip's.
                let left = intersect_min_x - rect.x();
                let top = intersect_min_y - rect.y();
                let right = rect.x() + rect.width() - intersect_max_x;
                let bottom = rect.y() + rect.height() - intersect_max_y;
                let corners = [
                    (corner_radius.top_left, shape.top_left, left, top),
                    (corner_radius.top_right, shape.top_right, right, top),
                    (
                        corner_radius.bottom_right,
                        shape.bottom_right,
                        right,
                        bottom,
                    ),
                    (corner_radius.bottom_left, shape.bottom_left, left, bottom),
                ];
                for (i, (r, n, dx, dy)) in corners.into_iter().enumerate() {
                    if dx < r && dy < r {
                        let radius = (r - dx.max(0.0)).clamp(0.0, r);
                        if radius > radii[i] {
                            radii[i] = radius;
                            corner_shape[i] = n;
                        }
                    }
                }
            }

            return (
                [intersect_min_x, intersect_min_y, width, height],
                radii,
                corner_shape,
                ClipType::Rect,
            );
        }

        // Fall back to topmost clip for non-rect clips.
        // For non-rect clips (circle, ellipse, polygon), clip_bounds carries the
        // parent rect scissor (from accumulated rect clips) and clip_radius carries
        // the shape-specific data. The shader applies both rect scissor AND shape clip.
        let scissor_bounds = if has_rect_clips {
            let width = (intersect_max_x - intersect_min_x).max(0.0);
            let height = (intersect_max_y - intersect_min_y).max(0.0);
            [intersect_min_x, intersect_min_y, width, height]
        } else {
            [-10000.0, -10000.0, 100000.0, 100000.0]
        };

        // Prefer the topmost non-rect clip (if any) for the shape
        // entry; otherwise fall back to the topmost clip outright
        // (which will be a rect / rounded-rect and lands in the rect-
        // intersection branches below). Scanning instead of just
        // `last()` is what lets a rect clip stacked on top of a
        // polygon matte still resolve to the polygon as the shape.
        let shape_idx = topmost_non_rect_idx.unwrap_or(active_clips.len() - 1);
        let (clip, poly_meta, _fade) = &active_clips[shape_idx];
        match clip {
            ClipShape::Rect(rect) => (
                [rect.x(), rect.y(), rect.width(), rect.height()],
                [0.0; 4],
                [1.0; 4],
                ClipType::Rect,
            ),
            ClipShape::RoundedRect {
                rect,
                corner_radius,
                corner_shape,
            } => (
                [rect.x(), rect.y(), rect.width(), rect.height()],
                [
                    corner_radius.top_left,
                    corner_radius.top_right,
                    corner_radius.bottom_right,
                    corner_radius.bottom_left,
                ],
                [
                    corner_shape.top_left,
                    corner_shape.top_right,
                    corner_shape.bottom_right,
                    corner_shape.bottom_left,
                ],
                ClipType::Rect,
            ),
            ClipShape::Circle { center, radius } => (
                // clip_bounds = rect scissor, clip_radius = [cx, cy, radius, 0]
                scissor_bounds,
                [center.x, center.y, *radius, 0.0],
                [1.0; 4],
                ClipType::Circle,
            ),
            ClipShape::Ellipse { center, radii } => (
                // clip_bounds = rect scissor, clip_radius = [cx, cy, rx, ry]
                scissor_bounds,
                [center.x, center.y, radii.x, radii.y],
                [1.0; 4],
                ClipType::Ellipse,
            ),
            ClipShape::Polygon(_) => {
                // clip_bounds = rect scissor, clip_radius = [even_odd, 0, vertex_count, aux_offset]
                let packed = poly_meta.as_ref().unwrap().polygon().unwrap().packed_count;
                let aux_offset = self.polygon_clip_offset(start + shape_idx);
                let even_odd = packed & POLYGON_EVEN_ODD != 0;
                let vertex_count = packed & !POLYGON_EVEN_ODD;
                (
                    scissor_bounds,
                    [
                        if even_odd { 1.0 } else { 0.0 },
                        0.0,
                        vertex_count as f32,
                        aux_offset as f32,
                    ],
                    [1.0; 4],
                    ClipType::Polygon,
                )
            }
            ClipShape::Path(_) => {
                // Images and native primitives attach path shape rows separately.
                (
                    if keep_path_scissor {
                        scissor_bounds
                    } else {
                        [-10000.0, -10000.0, 100000.0, 100000.0]
                    },
                    [0.0; 4],
                    [1.0; 4],
                    ClipType::None,
                )
            }
        }
    }

    pub(super) fn push_clip_impl(&mut self, shape: ClipShape) {
        // Transform the clip shape by the current transform
        // Note: This only works correctly for translate + uniform scale transforms.
        // Rotation transforms are approximated (the bounding box is used).
        let transformed_shape = self.transform_clip_shape(shape);
        // Pack polygon vertices in the active batch. Emission copies the
        // payload into any other batch that inherits this clip; cached
        // offsets remain valid until their batch is drained.
        // The rule is taken by every push, so it never reaches a later clip.
        let rule = std::mem::take(&mut self.pending_clip_fill_rule);
        let rule_bit = if rule == FillRule::EvenOdd {
            POLYGON_EVEN_ODD
        } else {
            0
        };
        let clip_data = if let ClipShape::Polygon(ref pts) = transformed_shape {
            let batch = self.aux_batch();
            let active = self.active_batch_mut();
            let aux_offset = active.aux_data.len() as u32;
            let vertex_count = pts.len() as u32;
            // Pack vertices as vec4s: (x0, y0, x1, y1) — 2 vertices per vec4
            let mut i = 0;
            while i < pts.len() {
                let x0 = pts[i].x;
                let y0 = pts[i].y;
                let (x1, y1) = if i + 1 < pts.len() {
                    (pts[i + 1].x, pts[i + 1].y)
                } else {
                    (0.0, 0.0) // padding
                };
                active.aux_data.push([x0, y0, x1, y1]);
                i += 2;
            }
            Some(ClipData::Polygon(PolygonClipData::new(
                batch,
                aux_offset,
                vertex_count | rule_bit,
            )))
        } else if let ClipShape::Path(_) = &transformed_shape {
            Some(ClipData::Path(super::path_clip::PathClipData::new(
                self.current_affine(),
                rule,
            )))
        } else if self.row_clip_base.is_some() && self.can_be_inner(&transformed_shape) {
            Some(ClipData::Inner(super::aux_data::InnerClipData::default()))
        } else {
            None
        };
        let fade = std::mem::replace(&mut self.pending_overflow_fade, [0.0; 4]);
        self.clip_stack.push((transformed_shape, clip_data, fade));
    }

    /// Whether `shape`, pushed inside content that rows move, can be one of its inner clips: a
    /// rect or a rounded rect, with no other kind of shape clip in force, which would take the one
    /// shape a primitive has.
    fn can_be_inner(&self, shape: &ClipShape) -> bool {
        matches!(shape, ClipShape::Rect(_) | ClipShape::RoundedRect { .. })
            && !self.clip_stack.iter().any(|(s, _, _)| {
                matches!(
                    s,
                    ClipShape::Circle { .. }
                        | ClipShape::Ellipse { .. }
                        | ClipShape::Polygon(_)
                        | ClipShape::Path(_)
                )
            })
    }

    /// The intersection of the clips in force, `skip_inner` leaving out the inner clips of
    /// content that rows move.
    pub(super) fn clip_aabb(&self, skip_inner: bool) -> Option<[f32; 4]> {
        if self.clip_stack.is_empty() {
            return None;
        }
        // Each clip in the stack is already in screen coordinates
        // (push_clip transforms by the active affine on entry). The
        // effective bounds is the intersection of every entry's
        // AABB. Non-rect clips (circles, paths, polygons) contribute
        // their bounding box — the compositor overlay uses the
        // result as a scissor rect, so a looser-than-necessary AABB
        // is safe (over-includes pixels) while a tighter bound would
        // be incorrect.
        let mut min_x = f32::NEG_INFINITY;
        let mut min_y = f32::NEG_INFINITY;
        let mut max_x = f32::INFINITY;
        let mut max_y = f32::INFINITY;
        for (shape, meta, _) in &self.clip_stack {
            if skip_inner && matches!(meta, Some(ClipData::Inner(_))) {
                continue;
            }
            let (sx, sy, sw, sh) = match shape {
                ClipShape::Rect(r) => (r.x(), r.y(), r.width(), r.height()),
                ClipShape::RoundedRect { rect, .. } => {
                    (rect.x(), rect.y(), rect.width(), rect.height())
                }
                ClipShape::Circle { center, radius } => (
                    center.x - radius,
                    center.y - radius,
                    radius * 2.0,
                    radius * 2.0,
                ),
                ClipShape::Ellipse { center, radii } => (
                    center.x - radii.x,
                    center.y - radii.y,
                    radii.x * 2.0,
                    radii.y * 2.0,
                ),
                ClipShape::Polygon(pts) => {
                    if pts.is_empty() {
                        continue;
                    }
                    let mut pmin_x = f32::INFINITY;
                    let mut pmin_y = f32::INFINITY;
                    let mut pmax_x = f32::NEG_INFINITY;
                    let mut pmax_y = f32::NEG_INFINITY;
                    for p in pts {
                        pmin_x = pmin_x.min(p.x);
                        pmin_y = pmin_y.min(p.y);
                        pmax_x = pmax_x.max(p.x);
                        pmax_y = pmax_y.max(p.y);
                    }
                    (pmin_x, pmin_y, pmax_x - pmin_x, pmax_y - pmin_y)
                }
                ClipShape::Path(_) => continue, // skip — no easy AABB
            };
            min_x = min_x.max(sx);
            min_y = min_y.max(sy);
            max_x = max_x.min(sx + sw);
            max_y = max_y.min(sy + sh);
        }
        if max_x <= min_x || max_y <= min_y {
            return Some([min_x, min_y, 0.0, 0.0]);
        }
        Some([min_x, min_y, max_x - min_x, max_y - min_y])
    }
}
