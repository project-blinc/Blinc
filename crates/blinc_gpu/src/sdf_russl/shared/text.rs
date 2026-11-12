//! The glyph shader: grayscale text from one atlas and colour emoji from another. `TIER` 0 reads
//! the glyphs from a storage buffer, 2 from a data texture (six texels per glyph, one row each).

russl::russl! {
    use super::access::{UNIFORMS, GLYPH_ATLAS, GLYPH_SAMPLER, COLOR_GLYPH_ATLAS};
    use super::clip::abi_shape_clip_alpha;

    #[pipeline] const TIER: u32 = 0;

    struct VertexOutput {
        #[builtin(position)] position: Vec4,
        // Atlas pixel coordinates; the fragment stage divides by the atlas size.
        #[location(0)] uv: Vec2,
        #[location(1)] color: Vec4,
        #[location(2)] world_pos: Vec2,
        #[location(3)] #[flat] clip_bounds: Vec4,
        #[location(4)] #[flat] clip_fade: Vec4,
        #[location(5)] #[flat] is_color: f32,
        #[location(6)] #[flat] clip_shape: f32,
    }

    struct GlyphInstance {
        // Position and size (x, y, width, height)
        bounds: Vec4,
        // Atlas pixel coordinates (x_min, y_min, x_max, y_max)
        uv_bounds: Vec4,
        // Text color
        color: Vec4,
        // Clip bounds (x, y, width, height); a very negative x means no clip
        clip_bounds: Vec4,
        // Overflow fade distances (top, right, bottom, left) in pixels
        clip_fade: Vec4,
        // Flags: [is_color, clip_shape, unused, unused]
        flags: Vec4,
    }

    #if TIER == 2
        #[group(0)] #[binding(1)] static GLYPH_DATA: Texture2D<f32>;

        fn glyph(index: u32) -> GlyphInstance {
            let y = index as i32;
            let mut g: GlyphInstance;
            g.bounds = GLYPH_DATA.load(IVec2::new(0, y), 0);
            g.uv_bounds = GLYPH_DATA.load(IVec2::new(1, y), 0);
            g.color = GLYPH_DATA.load(IVec2::new(2, y), 0);
            g.clip_bounds = GLYPH_DATA.load(IVec2::new(3, y), 0);
            g.clip_fade = GLYPH_DATA.load(IVec2::new(4, y), 0);
            g.flags = GLYPH_DATA.load(IVec2::new(5, y), 0);
            return g;
        }
    #else
        #[group(0)] #[binding(1)] static GLYPHS: Storage<[GlyphInstance]>;

        fn glyph(index: u32) -> GlyphInstance {
            return GLYPHS[index];
        }
    #endif

    #[vertex]
    fn vs_main(
        #[builtin(vertex_index)] vertex_index: u32,
        #[builtin(instance_index)] instance_index: u32,
    ) -> VertexOutput {
        let mut out: VertexOutput;
        let g = glyph(instance_index);

        // Two triangles over the bounds. A match rather than an indexed array: the PowerVR Vulkan
        // driver compiles indexing a constant array wrongly (see `sdf_core`).
        let mut local_uv: Vec2;
        match vertex_index {
            0u32 => { local_uv = Vec2::new(0.0, 0.0); }
            1u32 => { local_uv = Vec2::new(1.0, 0.0); }
            2u32 => { local_uv = Vec2::new(0.0, 1.0); }
            3u32 => { local_uv = Vec2::new(1.0, 0.0); }
            4u32 => { local_uv = Vec2::new(1.0, 1.0); }
            _ => { local_uv = Vec2::new(0.0, 1.0); }
        }

        let pos = Vec2::new(
            g.bounds.x + local_uv.x * g.bounds.z,
            g.bounds.y + local_uv.y * g.bounds.w
        );

        // The atlas is bound to the fragment stage only, so its size is not available here.
        let pixel_uv = Vec2::new(
            g.uv_bounds.x + local_uv.x * (g.uv_bounds.z - g.uv_bounds.x),
            g.uv_bounds.y + local_uv.y * (g.uv_bounds.w - g.uv_bounds.y)
        );

        let clip_pos = Vec2::new(
            (pos.x / UNIFORMS.viewport_size.x) * 2.0 - 1.0,
            1.0 - (pos.y / UNIFORMS.viewport_size.y) * 2.0
        );

        out.position = Vec4::new(clip_pos.x, clip_pos.y, 0.0, 1.0);
        out.uv = pixel_uv;
        out.color = g.color;
        out.world_pos = pos;
        out.clip_bounds = g.clip_bounds;
        out.clip_fade = g.clip_fade;
        out.is_color = g.flags.x;
        out.clip_shape = g.flags.y;

        return out;
    }

    // Alpha of the rectangular clip region, with an overflow fade on any side that sets one.
    fn calculate_clip_alpha(p: Vec2, clip_bounds: Vec4, clip_fade: Vec4) -> f32 {
        if clip_bounds.x < -5000.0 {
            return 1.0;
        }

        let clip_min = clip_bounds.xy;
        let clip_max = clip_bounds.xy + clip_bounds.zw;

        let d_left = p.x - clip_min.x;
        let d_right = clip_max.x - p.x;
        let d_top = p.y - clip_min.y;
        let d_bottom = clip_max.y - p.y;

        // Distance to the nearest edge, negative outside; one pixel of soft edge.
        let d = min(min(d_left, d_right), min(d_top, d_bottom));
        let mut alpha = clamp(d + 0.5, 0.0, 1.0);

        if clip_fade.x > 0.0 { alpha = alpha * saturate(d_top / clip_fade.x); }
        if clip_fade.y > 0.0 { alpha = alpha * saturate(d_right / clip_fade.y); }
        if clip_fade.z > 0.0 { alpha = alpha * saturate(d_bottom / clip_fade.z); }
        if clip_fade.w > 0.0 { alpha = alpha * saturate(d_left / clip_fade.w); }

        return alpha;
    }

    // Both atlases are sampled at level 0, explicitly: `is_color` is flat per glyph and can differ
    // between neighbouring quads, so the branch is not uniform control flow and an implicit-LOD
    // sample inside it is rejected by some compilers. The atlases have a single level.
    #[fragment]
    #[location(0)]
    fn fs_main(input: VertexOutput) -> Vec4 {
        let clip_alpha = calculate_clip_alpha(input.world_pos, input.clip_bounds, input.clip_fade)
            * abi_shape_clip_alpha(input.world_pos, input.clip_shape);
        if clip_alpha < 0.001 {
            discard!();
        }

        if input.is_color > 0.5 {
            // Colour emoji: the atlas colour as it is.
            let atlas_size = textureDimensions(COLOR_GLYPH_ATLAS).as_vec2();
            let emoji = textureSampleLevel(COLOR_GLYPH_ATLAS, GLYPH_SAMPLER, input.uv / atlas_size, 0.0);
            return Vec4::new(emoji.x, emoji.y, emoji.z, emoji.w * clip_alpha);
        } else {
            // Grayscale text: coverage tinted by the glyph colour. The exponent brightens the
            // mid-tones so strokes look crisper.
            let atlas_size = textureDimensions(GLYPH_ATLAS).as_vec2();
            let coverage = textureSampleLevel(GLYPH_ATLAS, GLYPH_SAMPLER, input.uv / atlas_size, 0.0).x;
            let aa_alpha = pow(coverage, 0.7);
            return Vec4::new(input.color.x, input.color.y, input.color.z, input.color.w * aa_alpha * clip_alpha);
        }
    }
}
