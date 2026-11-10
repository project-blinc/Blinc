//! Layer composition: a layer texture onto a destination with a blend mode and opacity, an
//! optional rounded clip, an optional perspective tilt and an in-plane rotation.

russl::russl! {
    struct LayerUniforms {
        // Source rectangle in the layer texture (normalised 0-1): x, y, width, height
        source_rect: Vec4,
        // Destination rectangle in the viewport (pixels): x, y, width, height
        dest_rect: Vec4,
        viewport_size: Vec2,
        // Layer opacity (0.0 - 1.0)
        opacity: f32,
        // Blend mode: 0 normal, 1 multiply, 2 screen, 3 overlay, 4 darken, 5 lighten,
        // 6 colour dodge, 7 colour burn, 8 hard light, 9 soft light, 10 difference, 11 exclusion
        blend_mode: u32,
        // Clip bounds (x, y, width, height) in pixels
        clip_bounds: Vec4,
        // Clip corner radii (top-left, top-right, bottom-right, bottom-left)
        clip_radius: Vec4,
        // Clip type: 0 none, 1 rect with optional rounded corners
        clip_type: u32,
        // 3D perspective transform (0 = disabled)
        perspective_d: f32,
        sin_rx: f32,
        cos_rx: f32,
        sin_ry: f32,
        cos_ry: f32,
        // In-plane (Z axis) rotation for the flat path, identity (0, 1): a motion-bound subtree
        // whose cached texture rotates each frame without being baked again.
        sin_rz: f32,
        cos_rz: f32,
    }

    #[group(0)] #[binding(0)] static UNIFORMS: Uniform<LayerUniforms>;
    #[group(0)] #[binding(1)] static LAYER_TEXTURE: Texture2D<f32>;
    #[group(0)] #[binding(2)] static LAYER_SAMPLER: Sampler;
    #[group(0)] #[binding(3)] static DEST_TEXTURE: Texture2D<f32>;
    #[group(0)] #[binding(4)] static DEST_SAMPLER: Sampler;

    struct VertexOutput {
        #[builtin(position)] position: Vec4,
        #[location(0)] uv: Vec2,
        // Fragment position in viewport pixels
        #[location(1)] frag_pos: Vec2,
    }

    // The signed distance to a rounded rectangle, for clipping.
    fn sd_rounded_rect_clip(p: Vec2, rect: Vec4, radii: Vec4) -> f32 {
        let center = rect.xy + rect.zw * 0.5;
        let half_size = rect.zw * 0.5;
        let q = abs(p - center) - half_size;

        // The corner radius of the quadrant: radii are top-left, top-right, bottom-right,
        // bottom-left.
        let mut r: f32;
        if p.x < center.x {
            if p.y < center.y {
                r = radii.x;
            } else {
                r = radii.w;
            }
        } else {
            if p.y < center.y {
                r = radii.y;
            } else {
                r = radii.z;
            }
        }

        // Clamp the radius to half the smaller dimension (CSS).
        r = min(r, min(half_size.x, half_size.y));

        let adjusted_q = q + r;
        return length(max(adjusted_q, Vec2::splat(0.0))) + min(max(adjusted_q.x, adjusted_q.y), 0.0) - r;
    }

    #[vertex]
    fn vs_main(#[builtin(vertex_index)] vertex_index: u32) -> VertexOutput {
        // The two triangles of a quad, from the vertex index.
        let mut positions = [
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(0.0, 1.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(0.0, 1.0),
        ];

        let local_pos = positions[vertex_index];

        // The source rectangle's UV.
        let uv = UNIFORMS.source_rect.xy + local_pos * UNIFORMS.source_rect.zw;

        let mut out: VertexOutput;

        if UNIFORMS.perspective_d > 0.0 {
            // 3D perspective: the layer was rendered flat, and rotate-x and rotate-y distort the
            // composite quad with a perspective projection.
            let center = UNIFORMS.dest_rect.xy + UNIFORMS.dest_rect.zw * 0.5;
            let half = UNIFORMS.dest_rect.zw * 0.5;

            // Local position relative to the centre, in pixels.
            let mut p = Vec3::new(
                (local_pos.x - 0.5) * 2.0 * half.x,
                (local_pos.y - 0.5) * 2.0 * half.y,
                0.0
            );

            // Rotate around Y.
            let ry_x = p.x * UNIFORMS.cos_ry - p.z * UNIFORMS.sin_ry;
            let ry_z = p.x * UNIFORMS.sin_ry + p.z * UNIFORMS.cos_ry;
            p.x = ry_x;
            p.z = ry_z;

            // Rotate around X.
            let rx_y = p.y * UNIFORMS.cos_rx - p.z * UNIFORMS.sin_rx;
            let rx_z = p.y * UNIFORMS.sin_rx + p.z * UNIFORMS.cos_rx;
            p.y = rx_y;
            p.z = rx_z;

            // Perspective projection.
            let d = UNIFORMS.perspective_d;
            let w = (d + p.z) / d;
            let screen = center + p.xy / w;

            // To NDC, with the perspective w kept for correct UV interpolation.
            let ndc = (screen / UNIFORMS.viewport_size) * 2.0 - 1.0;

            out.position = Vec4::new(ndc.x * w, -ndc.y * w, 0.0, w);
            out.frag_pos = screen;
        } else {
            // Flat compositing, with an in-plane Z rotation around the destination's centre:
            // shift the local position to a centred frame, rotate it, scale it to the destination
            // size and shift it back. The identity rotation leaves the position unchanged.
            let centered = local_pos - Vec2::new(0.5, 0.5);
            let half_size = UNIFORMS.dest_rect.zw * 0.5;
            let centered_px = centered * UNIFORMS.dest_rect.zw;
            let rotated_px = Vec2::new(
                centered_px.x * UNIFORMS.cos_rz - centered_px.y * UNIFORMS.sin_rz,
                centered_px.x * UNIFORMS.sin_rz + centered_px.y * UNIFORMS.cos_rz,
            );
            let dest_pos = UNIFORMS.dest_rect.xy + half_size + rotated_px;
            let ndc = (dest_pos / UNIFORMS.viewport_size) * 2.0 - 1.0;
            out.position = Vec4::new(ndc.x, -ndc.y, 0.0, 1.0);
            out.frag_pos = dest_pos;
        }

        out.uv = uv;
        return out;
    }

    // ------------------------------------------------------------------------
    // Blend modes
    // ------------------------------------------------------------------------

    fn blend_multiply(src: Vec3, dst: Vec3) -> Vec3 {
        return src * dst;
    }

    fn blend_screen(src: Vec3, dst: Vec3) -> Vec3 {
        return 1.0 - (1.0 - src) * (1.0 - dst);
    }

    fn blend_overlay_channel(s: f32, d: f32) -> f32 {
        if d < 0.5 {
            return 2.0 * s * d;
        } else {
            return 1.0 - 2.0 * (1.0 - s) * (1.0 - d);
        }
    }

    fn blend_overlay(src: Vec3, dst: Vec3) -> Vec3 {
        return Vec3::new(
            blend_overlay_channel(src.x, dst.x),
            blend_overlay_channel(src.y, dst.y),
            blend_overlay_channel(src.z, dst.z)
        );
    }

    fn blend_darken(src: Vec3, dst: Vec3) -> Vec3 {
        return min(src, dst);
    }

    fn blend_lighten(src: Vec3, dst: Vec3) -> Vec3 {
        return max(src, dst);
    }

    fn blend_color_dodge_channel(s: f32, d: f32) -> f32 {
        if s >= 1.0 {
            return 1.0;
        }
        return min(1.0, d / (1.0 - s));
    }

    fn blend_color_dodge(src: Vec3, dst: Vec3) -> Vec3 {
        return Vec3::new(
            blend_color_dodge_channel(src.x, dst.x),
            blend_color_dodge_channel(src.y, dst.y),
            blend_color_dodge_channel(src.z, dst.z)
        );
    }

    fn blend_color_burn_channel(s: f32, d: f32) -> f32 {
        if s <= 0.0 {
            return 0.0;
        }
        return 1.0 - min(1.0, (1.0 - d) / s);
    }

    fn blend_color_burn(src: Vec3, dst: Vec3) -> Vec3 {
        return Vec3::new(
            blend_color_burn_channel(src.x, dst.x),
            blend_color_burn_channel(src.y, dst.y),
            blend_color_burn_channel(src.z, dst.z)
        );
    }

    fn blend_hard_light(src: Vec3, dst: Vec3) -> Vec3 {
        // Hard light is overlay with the source and destination swapped.
        return Vec3::new(
            blend_overlay_channel(dst.x, src.x),
            blend_overlay_channel(dst.y, src.y),
            blend_overlay_channel(dst.z, src.z)
        );
    }

    fn blend_soft_light_channel(s: f32, d: f32) -> f32 {
        if s <= 0.5 {
            return d - (1.0 - 2.0 * s) * d * (1.0 - d);
        } else {
            let mut g: f32;
            if d <= 0.25 {
                g = ((16.0 * d - 12.0) * d + 4.0) * d;
            } else {
                g = sqrt(d);
            }
            return d + (2.0 * s - 1.0) * (g - d);
        }
    }

    fn blend_soft_light(src: Vec3, dst: Vec3) -> Vec3 {
        return Vec3::new(
            blend_soft_light_channel(src.x, dst.x),
            blend_soft_light_channel(src.y, dst.y),
            blend_soft_light_channel(src.z, dst.z)
        );
    }

    fn blend_difference(src: Vec3, dst: Vec3) -> Vec3 {
        return abs(src - dst);
    }

    fn blend_exclusion(src: Vec3, dst: Vec3) -> Vec3 {
        return src + dst - 2.0 * src * dst;
    }

    // The blend of `src` over `dst` for a mode; normal (0) is the source.
    fn apply_blend_mode(src: Vec3, dst: Vec3, mode: u32) -> Vec3 {
        let mut blended = src;
        match mode {
            1u32 => { blended = blend_multiply(src, dst); }
            2u32 => { blended = blend_screen(src, dst); }
            3u32 => { blended = blend_overlay(src, dst); }
            4u32 => { blended = blend_darken(src, dst); }
            5u32 => { blended = blend_lighten(src, dst); }
            6u32 => { blended = blend_color_dodge(src, dst); }
            7u32 => { blended = blend_color_burn(src, dst); }
            8u32 => { blended = blend_hard_light(src, dst); }
            9u32 => { blended = blend_soft_light(src, dst); }
            10u32 => { blended = blend_difference(src, dst); }
            11u32 => { blended = blend_exclusion(src, dst); }
            _ => {}
        }
        return blended;
    }

    // ------------------------------------------------------------------------
    // Fragment
    // ------------------------------------------------------------------------

    #[fragment]
    #[location(0)]
    fn fs_main(input: VertexOutput) -> Vec4 {
        if UNIFORMS.clip_type == 1u32 {
            let dist = sd_rounded_rect_clip(input.frag_pos, UNIFORMS.clip_bounds, UNIFORMS.clip_radius);
            if dist > 0.5 {
                discard!();
            }
        }

        // Level 0: the flow is no longer uniform after the discard above.
        let src = LAYER_TEXTURE.sample_level(LAYER_SAMPLER, input.uv, 0.0);

        let mut src_alpha = src.w * UNIFORMS.opacity;

        // An anti-aliased clip edge.
        if UNIFORMS.clip_type == 1u32 {
            let dist = sd_rounded_rect_clip(input.frag_pos, UNIFORMS.clip_bounds, UNIFORMS.clip_radius);
            let clip_alpha = 1.0 - smoothstep(-0.5, 0.5, dist);
            src_alpha *= clip_alpha;
        }

        // Fully transparent pixels draw nothing.
        if src_alpha < 0.001 {
            discard!();
        }

        // A blend mode other than normal reads the destination (a copy taken beforehand) and
        // applies the CSS blend function. The output is premultiplied, so hardware blending
        // `src + dst * (1 - src_a)` gives `mix(dst, blended, src_a)`.
        if UNIFORMS.blend_mode != 0u32 {
            let screen_uv = input.frag_pos / UNIFORMS.viewport_size;
            let dst = DEST_TEXTURE.sample_level(DEST_SAMPLER, screen_uv, 0.0);

            // Unpremultiply the source (src.a > 0, since src_alpha > 0.001 and opacity <= 1) and
            // the destination.
            let src_c = src.xyz / src.w;
            let dst_c = if dst.w < 0.001 { Vec3::splat(0.0) } else { dst.xyz / dst.w };

            let blended = apply_blend_mode(src_c, dst_c, UNIFORMS.blend_mode);

            return Vec4::new(blended * src_alpha, src_alpha);
        }

        // Normal blend: premultiplied for hardware blending.
        return Vec4::new(src.xyz * src_alpha, src_alpha);
    }
}
