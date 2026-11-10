//! Clips, ABI shape clips and gradient tables: the parts of Blinc's SDF core shader that read
//! the auxiliary data.

russl::russl! {
    use super::access::aux;
    use super::common::{sd_circle, sd_ellipse, sd_rounded_rect, sd_shaped_rect};

    // ============================================================================
    // SDF Functions
    // ============================================================================

    // Calculate clip alpha (1.0 = inside clip, 0.0 = outside)
    // For non-rect clips (circle, ellipse, polygon):
    //   clip_bounds = rect scissor from parent clips [x, y, w, h]
    //   clip_radius = shape-specific data
    // The shader applies BOTH the rect scissor AND the shape clip.
    // clip_fade = (top, right, bottom, left) overflow fade distances in pixels
    pub fn calculate_clip_alpha(p: Vec2, clip_bounds: Vec4, clip_radius: Vec4, clip_corner_shape: Vec4, clip_type: u32, clip_fade: Vec4, fade_bounds: Vec4) -> f32 {
        let mut alpha: f32 = 1.0;

        if clip_type != 0u32 {
            let aa_width = 0.75;
            match clip_type {
                1u32 /* CLIP_RECT */ => {
                    let clip_origin = clip_bounds.xy;
                    let clip_size = clip_bounds.zw;
                    let clip_d = sd_shaped_rect(p, clip_origin, clip_size, clip_radius, clip_corner_shape);
                    alpha = 1.0 - smoothstep(-aa_width, aa_width, clip_d);
                }
                2u32 /* CLIP_CIRCLE */ => {
                    let scissor_d = sd_rounded_rect(p, clip_bounds.xy, clip_bounds.zw, Vec4::splat(0.0));
                    let scissor_alpha = 1.0 - smoothstep(-aa_width, aa_width, scissor_d);
                    let center = clip_radius.xy;
                    let radius = clip_radius.z;
                    let clip_d = sd_circle(p, center, radius);
                    let shape_alpha = 1.0 - smoothstep(-aa_width, aa_width, clip_d);
                    alpha = scissor_alpha * shape_alpha;
                }
                3u32 /* CLIP_ELLIPSE */ => {
                    let scissor_d = sd_rounded_rect(p, clip_bounds.xy, clip_bounds.zw, Vec4::splat(0.0));
                    let scissor_alpha = 1.0 - smoothstep(-aa_width, aa_width, scissor_d);
                    let center = clip_radius.xy;
                    let radii = clip_radius.zw;
                    let clip_d = sd_ellipse(p, center, radii);
                    let shape_alpha = 1.0 - smoothstep(-aa_width, aa_width, clip_d);
                    alpha = scissor_alpha * shape_alpha;
                }
                4u32 /* CLIP_POLYGON */ => {
                    // Scissor-only for early discard. The polygon shape test
                    // is deferred to fs_main after `sp` is computed, so it
                    // can run against element-local coords (`sp - prim.bounds.xy`)
                    // and rotate with motion bindings / CSS transforms.
                    let scissor_d = sd_rounded_rect(p, clip_bounds.xy, clip_bounds.zw, Vec4::splat(0.0));
                    alpha = 1.0 - smoothstep(-aa_width, aa_width, scissor_d);
                }
                _ => {}
            }
        }

        // Apply overflow fade (smooth alpha ramp at clip edges)
        if clip_fade.x > 0.0 || clip_fade.y > 0.0 || clip_fade.z > 0.0 || clip_fade.w > 0.0 {
            let fade_rect = if fade_bounds.z > 0.0 { fade_bounds } else { clip_bounds };
            let clip_min = fade_rect.xy;
            let clip_max = fade_rect.xy + fade_rect.zw;
            if clip_fade.x > 0.0 { alpha *= saturate((p.y - clip_min.y) / clip_fade.x); }  // top
            if clip_fade.y > 0.0 { alpha *= saturate((clip_max.x - p.x) / clip_fade.y); }  // right
            if clip_fade.z > 0.0 { alpha *= saturate((clip_max.y - p.y) / clip_fade.z); }  // bottom
            if clip_fade.w > 0.0 { alpha *= saturate((p.x - clip_min.x) / clip_fade.w); }  // left
        }

        return alpha;
    }

    // Polygon clip using winding number test with edge-distance anti-aliasing.
    // Vertices packed in AUX_DATA as Vec4::new(x0, y0, x1, y1) — 2 vertices per vec4.
    pub fn calculate_polygon_clip_alpha(p: Vec2, vertex_count: u32, aux_offset: u32, even_odd: bool) -> f32 {
        if vertex_count < 3u32 {
            return 1.0;
        }

        let mut winding: i32 = 0;
        let mut min_edge_dist: f32 = 1e10;

        for i in 0u32..vertex_count {
            // Read vertex i: packed as (x0, y0, x1, y1) per vec4
            let vec_idx = aux_offset + (i / 2u32);
            let data = aux(vec_idx);
            let mut vi: Vec2;
            if (i % 2u32) == 0u32 {
                vi = data.xy;
            } else {
                vi = data.zw;
            }

            // Read vertex j (next, wrapping)
            let j = (i + 1u32) % vertex_count;
            let vec_idx_j = aux_offset + (j / 2u32);
            let data_j = aux(vec_idx_j);
            let mut vj: Vec2;
            if (j % 2u32) == 0u32 {
                vj = data_j.xy;
            } else {
                vj = data_j.zw;
            }

            // Winding number contribution (crossing number test)
            let edge = vj - vi;
            if vi.y <= p.y {
                if vj.y > p.y {
                    // Upward crossing
                    let cross_val = edge.x * (p.y - vi.y) - edge.y * (p.x - vi.x);
                    if cross_val > 0.0 {
                        winding = winding + 1;
                    }
                }
            } else {
                if vj.y <= p.y {
                    // Downward crossing
                    let cross_val = edge.x * (p.y - vi.y) - edge.y * (p.x - vi.x);
                    if cross_val < 0.0 {
                        winding = winding - 1;
                    }
                }
            }

            // Minimum distance to this edge segment (for anti-aliasing)
            let ap = p - vi;
            let edge_len_sq = dot(edge, edge);
            let mut t: f32 = 0.0;
            if edge_len_sq > 0.0001 {
                t = clamp(dot(ap, edge) / edge_len_sq, 0.0, 1.0);
            }
            let closest = vi + edge * t;
            let dist = length(p - closest);
            min_edge_dist = min(min_edge_dist, dist);
        }

        // Inside when the winding number is nonzero, or odd for the even-odd rule
        let is_inside = if even_odd { (winding & 1) != 0 } else { winding != 0 };

        // Signed distance: negative inside, positive outside
        let signed_dist = if is_inside { -min_edge_dist } else { min_edge_dist };

        // Anti-aliased edge
        let aa_width = 0.75;
        return 1.0 - smoothstep(-aa_width, aa_width, signed_dist);
    }

    // The colour of a gradient at `position`, from its table in AUX_DATA: a header
    // of the stop count and the spread, the average colour, then each stop's colour
    // and offset. Stops mix in premultiplied colour. A repeating or reflecting
    // gradient folds `position` into the span of its stops first.
    pub fn gradient_table_color(position: f32, first: u32) -> Vec4 {
        let header = aux(first);
        let count = ((header.x) as u32);
        let start = aux(first + 3u32).x;
        let end = aux(first + 1u32 + 2u32 * count).x;
        let mut t = position;
        if header.y > 0.5 {
            let period = end - start;
            if period <= 0.0001 {
                return aux(first + 1u32);
            }
            let mut phase = (t - start) / period;
            if header.y > 1.5 {
                phase = 1.0 - abs(2.0 * fract(phase * 0.5) - 1.0);
            } else {
                phase = fract(phase);
            }
            t = start + phase * period;
        }
        let mut color = aux(first + 2u32);
        let mut offset = start;
        let mut i = 1u32;
        while i < count {
            let next = aux(first + 2u32 + 2u32 * i);
            let at = aux(first + 3u32 + 2u32 * i).x;
            if t < at {
                let amount = clamp((t - offset) / max(at - offset, 1e-20), 0.0, 1.0);
                let premult = mix(
                    Vec4::new(color.rgb * color.a, color.a),
                    Vec4::new(next.rgb * next.a, next.a),
                    amount
                );
                if premult.a > 0.0 {
                    return Vec4::new(premult.rgb / premult.a, premult.a);
                }
                return Vec4::splat(0.0);
            }
            color = next;
            offset = at;
            i = i + 1u32;
        }
        return color;
    }

    // Binding-dependent helpers are composed after shader declarations by build.rs.
    pub fn abi_clip_data(index: u32) -> Vec4 {
        return aux(index);
    }

    pub fn abi_clip_crossing(a: Vec2, b: Vec2, p: Vec2) -> i32 {
        let side = (b.x - a.x) * (p.y - a.y) - (p.x - a.x) * (b.y - a.y);
        if a.y <= p.y {
            if b.y > p.y && side > 0.0 { return 1; }
        } else if b.y <= p.y && side < 0.0 { return -1; }
        return 0;
    }

    pub fn abi_clip_filled(winding: i32, even_odd: bool) -> i32 {
        return if if even_odd { (winding & 1) != 0 } else { winding != 0 } { 1 } else { 0 };
    }

    pub fn abi_clip_polygon(p: Vec2, first: u32, count: u32, even_odd: bool, aa: f32) -> f32 {
        let s = aa * 0.7;
        let mut w0 = 0;
        let mut w1 = 0;
        let mut w2 = 0;
        let mut w3 = 0;
        let mut w4 = 0;
        let mut distance_sq = 1e20;
        let mut previous = Vec2::splat(0.0);
        // Contours are explicitly closed. A point at 1e30 separates adjacent rings.
        for i in 0u32..count {
            let data = abi_clip_data(first + i / 2u32);
            let v = if (i & 1u32) != 0u32 { data.zw } else { data.xy };
            if i > 0u32 && v.x < 1e29 && previous.x < 1e29 {
                let edge = previous - v;
                let delta = p - v;
                let closest = delta - edge * clamp(dot(delta, edge) / max(dot(edge, edge), 1e-12), 0.0, 1.0);
                distance_sq = min(distance_sq, dot(closest, closest));
                w0 += abi_clip_crossing(previous, v, p);
                w1 += abi_clip_crossing(previous, v, p + Vec2::new(s, s));
                w2 += abi_clip_crossing(previous, v, p + Vec2::new(-s, s));
                w3 += abi_clip_crossing(previous, v, p + Vec2::new(s, -s));
                w4 += abi_clip_crossing(previous, v, p + Vec2::new(-s, -s));
            }
            previous = v;
        }
        let inside = abi_clip_filled(w1, even_odd) + abi_clip_filled(w2, even_odd)
            + abi_clip_filled(w3, even_odd) + abi_clip_filled(w4, even_odd);
        let distance = if abi_clip_filled(w0, even_odd) != 0 { -sqrt(distance_sq) } else { sqrt(distance_sq) };
        let mut alpha = 1.0 - smoothstep(-aa, aa, distance);
        if inside == 4 { alpha = 1.0; }
        if inside == 0 { alpha = 0.0; }
        return alpha;
    }

    pub fn abi_shape_clip_alpha(pixel: Vec2, pointer: f32) -> f32 {
        if pointer < 0.5 { return 1.0; }
        let base = ((pointer) as u32) - 1u32;
        let frame = abi_clip_data(base);
        let info = abi_clip_data(base + 1u32);
        let shape = abi_clip_data(base + 2u32);
        let p = Vec2::new(frame.x * pixel.x + frame.z * pixel.y, frame.y * pixel.x + frame.w * pixel.y) + info.xy;
        // Analytic halfPixel in the clip's frame, valid inside nonuniform control flow.
        let aa = max(0.25 * (length(frame.xy) + length(frame.zw)), 1e-6);
        if info.z > 2.5 {
            return abi_clip_polygon(p, base + ((shape.x) as u32), ((shape.y) as u32), shape.z > 0.5, aa);
        }
        if info.z > 1.5 {
            let distance = sd_rounded_rect(p, shape.xy, shape.zw, Vec4::splat(info.w));
            return 1.0 - smoothstep(-aa, aa, distance);
        }
        let radius = max(shape.zw, Vec2::splat(1e-4));
        let u = (p - shape.xy) / radius;
        let len = length(u);
        let distance = (len - 1.0) * len / max(length(u / radius), 1e-4);
        return 1.0 - smoothstep(-aa, aa, distance);
    }
}
