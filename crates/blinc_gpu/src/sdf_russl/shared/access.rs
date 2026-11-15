//! How a primitive is read, and the bind group around it. `TIER` is the device tier the shader is
//! built for: 0 reads storage buffers, 1 the same with the vertex stage's instance fields in a
//! vertex buffer, 2 `RGBA32F` data textures (WebGL2, no storage buffers). Tier 0 is the default;
//! a specialisation sets the others. The auxiliary data is in `aux`.

russl::russl! {
    use super::primitive::{Primitive, Uniforms};
    use super::node_table::{NodeRow, apply_node, scales_node, snap_glyph};

    #[pipeline] const TIER: u32 = 0;
    // 1 binds the animated node table and applies a node's delta to the primitives that follow it;
    // 0 leaves the primitives as they are stored.
    #[pipeline] const NODES: u32 = 0;

    #[group(0)] #[binding(0)] pub static UNIFORMS: Uniform<Uniforms>;
    // Glyph atlas textures for unified text rendering
    #[group(0)] #[binding(2)] pub static GLYPH_ATLAS: Texture2D<f32>;
    #[group(0)] #[binding(3)] pub static GLYPH_SAMPLER: Sampler;
    #[group(0)] #[binding(4)] pub static COLOR_GLYPH_ATLAS: Texture2D<f32>;

    #if TIER == 2
        // Primitive rows, one RGBA32F texel per vec4 field.
        #[group(0)] #[binding(1)] pub static PRIM_DATA: Texture2D<f32>;

        // Primitive `index` as stored: each row is one primitive, the column is the field.
        pub fn prim_rest(index: u32) -> Primitive {
            let y = index as i32;
            let mut p: Primitive;
            p.bounds = PRIM_DATA.load(IVec2::new(0, y), 0);
            p.corner_radius = PRIM_DATA.load(IVec2::new(1, y), 0);
            p.color = PRIM_DATA.load(IVec2::new(2, y), 0);
            p.color2 = PRIM_DATA.load(IVec2::new(3, y), 0);
            p.border = PRIM_DATA.load(IVec2::new(4, y), 0);
            p.border_color = PRIM_DATA.load(IVec2::new(5, y), 0);
            p.shadow = PRIM_DATA.load(IVec2::new(6, y), 0);
            p.shadow_color = PRIM_DATA.load(IVec2::new(7, y), 0);
            p.clip_bounds = PRIM_DATA.load(IVec2::new(8, y), 0);
            p.clip_radius = PRIM_DATA.load(IVec2::new(9, y), 0);
            p.clip_corner_shape = PRIM_DATA.load(IVec2::new(10, y), 0);
            p.gradient_params = PRIM_DATA.load(IVec2::new(11, y), 0);
            p.rotation = PRIM_DATA.load(IVec2::new(12, y), 0);
            p.local_affine = PRIM_DATA.load(IVec2::new(13, y), 0);
            p.perspective = PRIM_DATA.load(IVec2::new(14, y), 0);
            p.sdf_3d = PRIM_DATA.load(IVec2::new(15, y), 0);
            p.light = PRIM_DATA.load(IVec2::new(16, y), 0);
            p.filter_a = PRIM_DATA.load(IVec2::new(17, y), 0);
            p.filter_b = PRIM_DATA.load(IVec2::new(18, y), 0);
            p.mask_params = PRIM_DATA.load(IVec2::new(19, y), 0);
            p.mask_info = PRIM_DATA.load(IVec2::new(20, y), 0);
            p.corner_shape = PRIM_DATA.load(IVec2::new(21, y), 0);
            p.clip_fade = PRIM_DATA.load(IVec2::new(22, y), 0);
            let ti = PRIM_DATA.load(IVec2::new(23, y), 0);
            p.type_info = UVec4::new(ti.x.to_bits(), ti.y.to_bits(), ti.z.to_bits(), ti.w.to_bits());
            p.side_color_top = PRIM_DATA.load(IVec2::new(24, y), 0);
            p.side_color_right = PRIM_DATA.load(IVec2::new(25, y), 0);
            p.side_color_bottom = PRIM_DATA.load(IVec2::new(26, y), 0);
            p.side_color_left = PRIM_DATA.load(IVec2::new(27, y), 0);
            p.gradient_table = PRIM_DATA.load(IVec2::new(28, y), 0);
            p.local_clip_bounds = PRIM_DATA.load(IVec2::new(29, y), 0);
            p.local_clip_radius = PRIM_DATA.load(IVec2::new(30, y), 0);
            p.clip_fade_bounds = PRIM_DATA.load(IVec2::new(31, y), 0);
            p.notch_info = PRIM_DATA.load(IVec2::new(32, y), 0);
            p.clip_shape = PRIM_DATA.load(IVec2::new(33, y), 0);
            p.node_slot = PRIM_DATA.load(IVec2::new(34, y), 0);
            return p;
        }
    #else
        #[group(0)] #[binding(1)] pub static PRIMITIVES: Storage<[Primitive]>;

        // Primitive `index` as stored.
        pub fn prim_rest(index: u32) -> Primitive {
            return PRIMITIVES[index];
        }
    #endif

    #if NODES == 1
        #if TIER == 2
            // Node rows, three RGBA32F texels each: motion, pivot, link.
            #[group(0)] #[binding(6)] pub static NODE_DATA: Texture2D<f32>;

            pub fn node_row(slot: u32) -> NodeRow {
                let y = slot as i32;
                let mut r: NodeRow;
                r.motion = NODE_DATA.load(IVec2::new(0, y), 0);
                r.pivot = NODE_DATA.load(IVec2::new(1, y), 0);
                r.link = NODE_DATA.load(IVec2::new(2, y), 0);
                return r;
            }
        #else
            #[group(0)] #[binding(6)] pub static NODE_ROWS: Storage<[NodeRow]>;

            pub fn node_row(slot: u32) -> NodeRow {
                return NODE_ROWS[slot];
            }
        #endif

        // Primitive `index` as it is drawn: under its node's delta, then each parent's, up to four,
        // and a glyph under a row that scales put on a whole device pixel.
        pub fn prim(index: u32) -> Primitive {
            let mut p = prim_rest(index);
            let mut slot = p.node_slot.x as u32;
            let mut scaled = false;
            for depth in 0u32..4u32 {
                if slot != 0u32 {
                    let row = node_row(slot - 1u32);
                    if scales_node(row) {
                        scaled = true;
                    }
                    p = apply_node(p, row, depth);
                    slot = row.link.x as u32;
                }
            }
            if scaled {
                p = snap_glyph(p);
            }
            return p;
        }
    #else
        pub fn prim(index: u32) -> Primitive {
            return prim_rest(index);
        }
    #endif
}
