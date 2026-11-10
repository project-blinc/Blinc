//! Path fills and strokes, emitted as SDF-stream triangles or native path rows.

use super::GpuPaintContext;
use crate::path::{
    DEFAULT_TESSELLATION_TOLERANCE, extract_brush_info, tessellate_fill_with_tolerance,
    tessellate_stroke_with_tolerance,
};
use crate::primitives::{ClipType, GpuPrimitive};
use blinc_core::{Brush, Path, Stroke};

/// Scale-aware tessellation tolerance.
///
/// The path tessellators flatten curves with a tolerance measured
/// in path-space units. When the path is later drawn at a larger
/// transform scale (canvas zoom, CSS `transform: scale()`, DPI
/// scale), one path-unit of tolerance becomes `scale` screen-pixel
/// units of deviation — at large zoom, polylines visibly
/// polygonalise. Dividing the default tolerance by `scale` keeps
/// the screen-space deviation roughly constant ≈ 0.2 px regardless
/// of zoom level. The cost is more triangles when zoomed in, which
/// is exactly the trade you want for crisp curves.
pub(super) fn scale_aware_tolerance(scale: f32) -> f32 {
    (DEFAULT_TESSELLATION_TOLERANCE / scale.max(1e-3)).max(0.005)
}

impl GpuPaintContext<'_> {
    pub(super) fn fill_path_impl(&mut self, path: &Path, brush: Brush) {
        // Apply current opacity to the brush
        let opacity = self.combined_opacity();
        let brush = Self::apply_opacity_to_brush(brush, opacity);

        // Solid-color fills route through the SDF primitive stream so
        // they interleave with text (and any other SDF draws) in
        // submission order. The tessellated-path pipeline sits in its
        // own draw call after every SDF pipeline, so a shape filled
        // through it would paint on top of text submitted earlier —
        // Lottie layers whose stack put text on top of a shape came
        // out with the text behind the shape. The SDF-stream route
        // avoids that entirely by sharing the primitive dispatch.
        //
        // Gradients take the same SDF-stream route via the PRIM_MESH
        // fill_type path: the SDF core shader already rasterises
        // linear and radial gradients per-fragment; routing them
        // through `push_mesh_primitives_brush` keeps gradient fills
        // in submission order with solids. Previously gradients went
        // to the path pipeline which drew in a single batch AFTER
        // every SDF primitive, so solids authored later in the
        // Lottie stack covered gradients authored earlier — the
        // "brown face / purple BG / seeker invisible" symptom on
        // the interactive_volume scene.
        if matches!(brush, Brush::Solid(_) | Brush::Gradient(_)) {
            let tess_tolerance = scale_aware_tolerance(self.current_uniform_scale());
            let mut tessellated = tessellate_fill_with_tolerance(path, &brush, tess_tolerance);
            let affine = self.current_affine();
            // Pre-sample per-vertex colours for multi-stop gradients
            // BEFORE the affine transform — gradient stops are
            // defined in the path's local coordinate space, so
            // sampling has to happen there. The SDF shader's built-in
            // linear / radial math only handles 2 stops (color +
            // color2); per-vertex sampling + barycentric interpolation
            // reproduces the authored multi-stop ramp at a fidelity
            // that tracks tessellation density (lyon's 0.2 px
            // tolerance keeps triangles small enough that per-vertex
            // Gouraud-style colour is visually close to true
            // multi-stop).
            let per_vertex_colors = Self::sample_per_vertex_gradient(&tessellated, &brush);
            for vertex in &mut tessellated.vertices {
                let x = vertex.position[0];
                let y = vertex.position[1];
                vertex.position[0] =
                    affine.elements[0] * x + affine.elements[2] * y + affine.elements[4];
                vertex.position[1] =
                    affine.elements[1] * x + affine.elements[3] * y + affine.elements[5];
            }
            if !tessellated.is_empty() {
                let (clip_bounds, clip_radius, clip_corner_shape, clip_type) = self.get_clip_data();
                self.push_mesh_primitives_brush(
                    &tessellated,
                    &brush,
                    &affine,
                    per_vertex_colors.as_deref(),
                    clip_bounds,
                    clip_radius,
                    clip_corner_shape,
                    clip_type,
                );
            }
            return;
        }

        // Glass / image / other brush types: fall back to the
        // tessellated-path pipeline. These are rare in Lottie and
        // the path pipeline handles them today.
        let brush_info = extract_brush_info(&brush);

        let tess_tolerance = scale_aware_tolerance(self.current_uniform_scale());
        let mut tessellated = tessellate_fill_with_tolerance(path, &brush, tess_tolerance);

        let affine = self.current_affine();
        for vertex in &mut tessellated.vertices {
            let x = vertex.position[0];
            let y = vertex.position[1];
            vertex.position[0] =
                affine.elements[0] * x + affine.elements[2] * y + affine.elements[4];
            vertex.position[1] =
                affine.elements[1] * x + affine.elements[3] * y + affine.elements[5];
        }

        if !tessellated.is_empty() {
            self.emit_native_path(tessellated, &brush_info);
        }
    }

    pub(super) fn stroke_path_impl(&mut self, path: &Path, stroke: &Stroke, brush: Brush) {
        // Apply current opacity to the brush
        let opacity = self.combined_opacity();
        let brush = Self::apply_opacity_to_brush(brush, opacity);

        // Solid + gradient strokes share the SDF-stream route so
        // submission order lines up with text. See `fill_path` for
        // the rationale.
        if matches!(brush, Brush::Solid(_) | Brush::Gradient(_)) {
            let tess_tolerance = scale_aware_tolerance(self.current_uniform_scale());
            let mut tessellated =
                tessellate_stroke_with_tolerance(path, stroke, &brush, tess_tolerance);
            let affine = self.current_affine();
            let per_vertex_colors = Self::sample_per_vertex_gradient(&tessellated, &brush);
            for vertex in &mut tessellated.vertices {
                let x = vertex.position[0];
                let y = vertex.position[1];
                vertex.position[0] =
                    affine.elements[0] * x + affine.elements[2] * y + affine.elements[4];
                vertex.position[1] =
                    affine.elements[1] * x + affine.elements[3] * y + affine.elements[5];
            }
            if !tessellated.is_empty() {
                let (clip_bounds, clip_radius, clip_corner_shape, clip_type) = self.get_clip_data();
                self.push_mesh_primitives_brush(
                    &tessellated,
                    &brush,
                    &affine,
                    per_vertex_colors.as_deref(),
                    clip_bounds,
                    clip_radius,
                    clip_corner_shape,
                    clip_type,
                );
            }
            return;
        }

        let brush_info = extract_brush_info(&brush);

        let tess_tolerance = scale_aware_tolerance(self.current_uniform_scale());
        let mut tessellated =
            tessellate_stroke_with_tolerance(path, stroke, &brush, tess_tolerance);

        let affine = self.current_affine();
        for vertex in &mut tessellated.vertices {
            let x = vertex.position[0];
            let y = vertex.position[1];
            vertex.position[0] =
                affine.elements[0] * x + affine.elements[2] * y + affine.elements[4];
            vertex.position[1] =
                affine.elements[1] * x + affine.elements[3] * y + affine.elements[5];
        }

        if !tessellated.is_empty() {
            self.emit_native_path(tessellated, &brush_info);
        }
    }

    /// Emit one `GpuPrimitive` per tessellated triangle. Each
    /// primitive's three corners live at `aux_data[off+0].xy`,
    /// `aux_data[off+0].zw`, `aux_data[off+1].xy`; the SDF vertex
    /// shader pulls them directly for `vertex_index` 0..2 and
    /// collapses 3..5 to a zero-area degenerate so the underlying
    /// 6-vertex quad draw still works. Hardware rasterises each
    /// triangle the same way the old tessellated-path pipeline did,
    /// so the fragment shader only needs solid fill + silhouette AA
    /// — no per-pixel triangle walk.
    ///
    /// For anti-aliasing, each triangle also carries which of its
    /// three edges lie on the mesh silhouette (edges that appear in
    /// exactly one triangle, identified here via a shared-edge
    /// count pass). The vertex shader emits a per-vertex barycentric
    /// varying; hardware interpolates it across the triangle, and
    /// the fragment shader uses `fwidth` on the relevant barycentric
    /// component to produce a 1-pixel smoothstep at each silhouette
    /// edge. Interior tessellation seams carry a zero flag, so the
    /// fragment keeps full alpha across them and adjacent triangles
    /// don't leave visible bands.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn push_mesh_primitives_brush(
        &mut self,
        tessellated: &crate::path::TessellatedPath,
        brush: &Brush,
        affine: &blinc_core::Affine2D,
        per_vertex_colors: Option<&[blinc_core::Color]>,
        clip_bounds: [f32; 4],
        clip_radius: [f32; 4],
        clip_corner_shape: [f32; 4],
        clip_type: ClipType,
    ) {
        use crate::primitives::PrimitiveType;
        use std::collections::HashMap;
        let indices = &tessellated.indices;
        let vertices = &tessellated.vertices;
        if indices.len() < 3 {
            return;
        }

        // Edge-occurrence count: (low_index, high_index) → count.
        // Silhouette edges appear exactly once across the whole
        // tessellation; interior edges appear twice.
        let mut edge_counts: HashMap<(u32, u32), u32> = HashMap::new();
        let mut i = 0;
        while i + 2 < indices.len() {
            let i0 = indices[i];
            let i1 = indices[i + 1];
            let i2 = indices[i + 2];
            i += 3;
            for (a, b) in [(i0, i1), (i1, i2), (i2, i0)] {
                let key = if a < b { (a, b) } else { (b, a) };
                *edge_counts.entry(key).or_insert(0) += 1;
            }
        }
        let is_silhouette = |a: u32, b: u32| -> f32 {
            let key = if a < b { (a, b) } else { (b, a) };
            match edge_counts.get(&key).copied().unwrap_or(0) {
                1 => 1.0,
                _ => 0.0,
            }
        };

        // Encode brush into gradient fields shared across every
        // triangle emitted for this fill. The SDF core shader reads
        // `color` + `color2` + `gradient_params` via `fill_type`
        // (PRIM_MESH honours the same fill_type branches as rect /
        // circle / ellipse), so we transform gradient endpoints from
        // path-local to screen space once and reuse them per
        // triangle.
        let apply = |pt: [f32; 2]| -> [f32; 2] {
            [
                affine.elements[0] * pt[0] + affine.elements[2] * pt[1] + affine.elements[4],
                affine.elements[1] * pt[0] + affine.elements[3] * pt[1] + affine.elements[5],
            ]
        };
        let scale = {
            let a = affine.elements[0];
            let b = affine.elements[1];
            let c = affine.elements[2];
            let d = affine.elements[3];
            let sx = (a * a + b * b).sqrt();
            let sy = (c * c + d * d).sqrt();
            (sx + sy) * 0.5
        };
        let (fill_type, color_arr, color2_arr, grad_params) = match brush {
            Brush::Solid(c) => (
                0u32,
                [c.r, c.g, c.b, c.a],
                [c.r, c.g, c.b, c.a],
                [0.0, 0.0, 0.0, 0.0],
            ),
            Brush::Gradient(g) => {
                let first = g.first_color();
                let last = g.last_color();
                match g {
                    blinc_core::Gradient::Linear { start, end, .. } => {
                        let s = apply([start.x, start.y]);
                        let e = apply([end.x, end.y]);
                        (
                            1u32,
                            [first.r, first.g, first.b, first.a],
                            [last.r, last.g, last.b, last.a],
                            [s[0], s[1], e[0], e[1]],
                        )
                    }
                    blinc_core::Gradient::Radial { center, radius, .. } => {
                        let c = apply([center.x, center.y]);
                        (
                            2u32,
                            [first.r, first.g, first.b, first.a],
                            [last.r, last.g, last.b, last.a],
                            [c[0], c[1], radius * scale, 0.0],
                        )
                    }
                    blinc_core::Gradient::Conic { center, .. } => {
                        let c = apply([center.x, center.y]);
                        (
                            2u32,
                            [first.r, first.g, first.b, first.a],
                            [last.r, last.g, last.b, last.a],
                            [c[0], c[1], 100.0 * scale, 0.0],
                        )
                    }
                }
            }
            _ => (
                0u32,
                [1.0, 1.0, 1.0, 1.0],
                [1.0, 1.0, 1.0, 1.0],
                [0.0, 0.0, 0.0, 0.0],
            ),
        };
        // Resolve the shared clip once for this mesh, rather than per triangle.
        let (clip_bounds, clip_radius, clip_corner_shape, clip_type, clip_shape) =
            if let Some((bounds, pointer)) = self.native_primitive_path_clip() {
                (
                    bounds,
                    [0.0; 4],
                    [1.0; 4],
                    ClipType::Rect,
                    [pointer, 0.0, 0.0, 0.0],
                )
            } else {
                (
                    clip_bounds,
                    clip_radius,
                    clip_corner_shape,
                    clip_type,
                    [0.0; 4],
                )
            };
        let clip_type_u = clip_type as u32;

        let mut i = 0;
        while i + 2 < indices.len() {
            let i0 = indices[i];
            let i1 = indices[i + 1];
            let i2 = indices[i + 2];
            i += 3;

            let v0 = vertices[i0 as usize].position;
            let v1 = vertices[i1 as usize].position;
            let v2 = vertices[i2 as usize].position;

            let ax = v1[0] - v0[0];
            let ay = v1[1] - v0[1];
            let bx = v2[0] - v0[0];
            let by = v2[1] - v0[1];
            let area2 = ax * by - ay * bx;
            if area2.abs() < 1e-4 {
                continue;
            }

            let aux_data = &mut self.active_batch_mut().aux_data;
            let aux_offset = aux_data.len() as u32;
            aux_data.push([v0[0], v0[1], v1[0], v1[1]]);
            aux_data.push([v2[0], v2[1], 0.0, 0.0]);
            // Multi-stop gradient: append 3 per-vertex colours right
            // after the triangle positions. The shader reads them at
            // `aux_data[aux_offset + 2..+5]` when
            // `type_info.w == 1u` and does barycentric interpolation
            // via `mesh_bary`, bypassing the fill_type switch so the
            // authored ramp is reproduced smoothly across the
            // triangle.
            if let Some(colors) = per_vertex_colors {
                let c0 = colors[i0 as usize];
                let c1 = colors[i1 as usize];
                let c2 = colors[i2 as usize];
                aux_data.push([c0.r, c0.g, c0.b, c0.a]);
                aux_data.push([c1.r, c1.g, c1.b, c1.a]);
                aux_data.push([c2.r, c2.g, c2.b, c2.a]);
            }

            let min_x = v0[0].min(v1[0]).min(v2[0]);
            let min_y = v0[1].min(v1[1]).min(v2[1]);
            let max_x = v0[0].max(v1[0]).max(v2[0]);
            let max_y = v0[1].max(v1[1]).max(v2[1]);

            // Silhouette flags per edge. `corner_radius` is otherwise
            // unused for mesh primitives (no rounded corners on a
            // raw triangle), so repurpose its three spare slots:
            //   [0] = edge v0→v1 (bary.z == 0)
            //   [1] = edge v1→v2 (bary.x == 0)
            //   [2] = edge v2→v0 (bary.y == 0)
            let s01 = is_silhouette(i0, i1);
            let s12 = is_silhouette(i1, i2);
            let s20 = is_silhouette(i2, i0);

            let mesh_flag: u32 = if per_vertex_colors.is_some() { 1 } else { 0 };

            let mut prim = GpuPrimitive {
                bounds: [min_x, min_y, max_x - min_x, max_y - min_y],
                color: color_arr,
                color2: color2_arr,
                gradient_params: grad_params,
                border: [0.0, 0.0, aux_offset as f32, 0.0],
                corner_radius: [s01, s12, s20, 0.0],
                clip_bounds,
                clip_radius,
                clip_corner_shape,
                clip_shape,
                type_info: [
                    PrimitiveType::Mesh as u32,
                    fill_type,
                    clip_type_u,
                    mesh_flag,
                ],
                ..GpuPrimitive::default()
            };
            prim.local_affine = [1.0, 0.0, 0.0, 1.0];

            self.push_native_primitive(prim);
        }
    }
}
