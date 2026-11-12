//! The SDF simple shader: plain rects and circles, the common primitive drawn with as little work
//! per fragment as possible. It has a solid fill, a rounded rect (circular or convex superellipse
//! corners) or a circle, an optional uniform border and an optional rounded rect clip.
//! `GpuPrimitive::is_plain_core` picks the primitives that take it; the rest use `sdf_core`, and
//! for these the pixels match the core shader's.
//!
//! The vertex stage reads the primitive once per vertex and passes what the fragment stage needs
//! as flat values, so the fragment stage does no memory loads. Storage tier only.

russl::russl! {
    use super::access::{UNIFORMS, prim};
    use super::common::{sd_rounded_rect, superellipse_dist};

    struct VertexOutput {
        #[builtin(position)] position: Vec4,
        #[location(0)] uv: Vec2,
        #[location(1)] #[flat] color: Vec4,
        #[location(2)] #[flat] bounds: Vec4,
        #[location(3)] #[flat] radius: Vec4,
        #[location(4)] #[flat] border_color: Vec4,
        #[location(5)] #[flat] clip_bounds: Vec4,
        #[location(6)] #[flat] clip_radius: Vec4,
        // x: border width, y: 1 for a circle, z: 1 when a rounded rect clip applies.
        #[location(7)] #[flat] misc: Vec3,
        // The corner exponent of each corner: 1 is a circle, other positive values a superellipse.
        #[location(8)] #[flat] shape: Vec4,
    }

    #[vertex]
    fn vs_main(
        #[builtin(vertex_index)] vertex_index: u32,
        #[builtin(instance_index)] instance_index: u32,
    ) -> VertexOutput {
        let mut out: VertexOutput;
        let p = prim(instance_index);

        // Two triangles over the bounds. A match rather than an indexed array: the PowerVR Vulkan
        // driver compiles indexing a constant array wrongly (see `sdf_core`).
        let mut uv: Vec2;
        match vertex_index {
            0u32 => { uv = Vec2::new(0.0, 0.0); }
            1u32 => { uv = Vec2::new(1.0, 0.0); }
            2u32 => { uv = Vec2::new(0.0, 1.0); }
            3u32 => { uv = Vec2::new(1.0, 0.0); }
            4u32 => { uv = Vec2::new(1.0, 1.0); }
            _ => { uv = Vec2::new(0.0, 1.0); }
        }
        let pos = p.bounds.xy + uv * p.bounds.zw;
        out.position = Vec4::new(
            (pos.x / UNIFORMS.viewport_size.x) * 2.0 - 1.0,
            1.0 - (pos.y / UNIFORMS.viewport_size.y) * 2.0,
            0.0,
            1.0
        );
        out.uv = pos;
        out.color = p.color;
        out.bounds = p.bounds;
        out.radius = p.corner_radius;
        out.border_color = p.border_color;
        out.clip_bounds = p.clip_bounds;
        out.clip_radius = p.clip_radius;
        out.shape = p.corner_shape;
        // A border given as one width in `border.x`, or the same width on all four sides.
        out.misc = Vec3::new(
            p.border.x,
            if p.type_info.x == 1u32 { 1.0 } else { 0.0 },
            if p.type_info.z == 1u32 { 1.0 } else { 0.0 },
        );
        return out;
    }

    // `sd_shaped_rect` for the corner exponents this shader takes (positive and below 100): a
    // circle where the exponent is 1, otherwise a convex superellipse. Outside the corner squares,
    // nearly every fragment of a large rect, it skips the superellipse's `pow` calls and returns
    // what the general form returns there.
    fn sd_convex_rect(p: Vec2, origin: Vec2, size: Vec2, radius: Vec4, shape: Vec4) -> f32 {
        let half_size = size * 0.5;
        let rel = p - (origin + half_size);
        let q = abs(rel) - half_size;
        let mut r: f32;
        let mut n: f32;
        if rel.y < 0.0 {
            if rel.x > 0.0 {
                r = radius.y; n = shape.y;
            } else {
                r = radius.x; n = shape.x;
            }
        } else {
            if rel.x > 0.0 {
                r = radius.z; n = shape.z;
            } else {
                r = radius.w; n = shape.w;
            }
        }
        r = min(r, min(half_size.x, half_size.y));
        let qa = q + Vec2::splat(r);
        if abs(n - 1.0) < 0.01 {
            return length(max(qa, Vec2::splat(0.0))) + min(max(qa.x, qa.y), 0.0) - r;
        }
        let box_d = length(max(q, Vec2::splat(0.0))) + min(max(q.x, q.y), 0.0);
        if qa.x <= 0.0 && qa.y <= 0.0 {
            return max(box_d, -r);
        }
        if qa.x <= 0.0 || qa.y <= 0.0 {
            return box_d;
        }
        let p_exp = pow(2.0, min(n, 5.0));
        let t = qa / max(r, 0.001);
        return max(box_d, superellipse_dist(t, p_exp, r));
    }

    #[fragment]
    #[location(0)]
    fn fs_main(input: VertexOutput) -> Vec4 {
        let p = input.uv;

        // Rounded rect clip, with the core shader's edge width.
        let mut clip_alpha = 1.0;
        if input.misc.z > 0.5 {
            let clip_d = sd_rounded_rect(p, input.clip_bounds.xy, input.clip_bounds.zw, input.clip_radius);
            clip_alpha = 1.0 - smoothstep(-0.75, 0.75, clip_d);
            if clip_alpha < 0.001 {
                discard!();
            }
        }

        // The shape's distance, then one pixel of edge.
        let mut d: f32;
        if input.misc.y > 0.5 {
            d = length(p - (input.bounds.xy + input.bounds.zw * 0.5)) - min(input.bounds.z, input.bounds.w) * 0.5;
        } else {
            d = sd_convex_rect(p, input.bounds.xy, input.bounds.zw, input.radius, input.shape);
        }
        let fill_alpha = 1.0 - smoothstep(-0.5, 0.5, d);
        if fill_alpha < 0.001 {
            discard!();
        }

        let mut fill = input.color;

        // A uniform border is the ring between the shape and the shape inset by its width. Inside
        // the inset the fill is untouched, so only the edge pays for the blend.
        let border = input.misc.x;
        if border > 0.0 && d > -border - 0.5 {
            let border_blend = smoothstep(-0.5, 0.5, d + border);
            let border_a = input.border_color.w * border_blend;
            let out_a = border_a + fill.w * (1.0 - border_a);
            let premult_rgb = input.border_color.xyz * border_a + fill.xyz * fill.w * (1.0 - border_a);
            let rgb = if out_a < 0.0001 { Vec3::splat(0.0) } else { premult_rgb / max(out_a, 0.0001) };
            fill = Vec4::new(rgb, out_a);
        }

        return Vec4::new(fill.xyz, fill.w * clip_alpha * fill_alpha);
    }
}
