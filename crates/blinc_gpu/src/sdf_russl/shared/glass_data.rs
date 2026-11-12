//! What the glass shaders share: the uniforms, the glass primitive and how a tier reads it, and
//! the backdrop texture. `TIER` 0 reads a storage buffer, 2 an `RGBA32F` texture with a row per
//! primitive and a texel per vec4 field.

russl::russl! {
    #[pipeline] const TIER: u32 = 0;

    pub struct GlassUniforms {
        pub viewport_size: Vec2,
        pub time: f32,
        pub clip_in_image: f32,
        pub sample_size: Vec2,
        pub origin: Vec2,
    }

    pub struct GlassPrimitive {
        // Bounds (x, y, width, height)
        pub bounds: Vec4,
        // Corner radii (top-left, top-right, bottom-right, bottom-left)
        pub corner_radius: Vec4,
        // Tint color (RGBA)
        pub tint_color: Vec4,
        // Glass parameters (blur_radius, saturation, brightness, noise_amount)
        pub params: Vec4,
        // Glass parameters 2 (border_thickness, light_angle, shadow_blur, shadow_opacity)
        pub params2: Vec4,
        // Type info (glass_type, shadow_offset_x_bits, shadow_offset_y_bits, refraction bits)
        pub type_info: UVec4,
        // Clip bounds (x, y, width, height) for clamping blur samples
        pub clip_bounds: Vec4,
        // Clip corner radii (for rounded rect clips)
        pub clip_radius: Vec4,
        // Border color (RGBA): when alpha > 0 a solid border replaces the light-based highlight
        pub border_color: Vec4,
    }

    #[group(0)] #[binding(0)] pub static GLASS_UNIFORMS: Uniform<GlassUniforms>;
    #[group(0)] #[binding(2)] pub static BACKDROP_TEXTURE: Texture2D<f32>;
    #[group(0)] #[binding(3)] pub static BACKDROP_SAMPLER: Sampler;

    #if TIER == 2
        #[group(0)] #[binding(1)] static PRIM_DATA: Texture2D<f32>;

        pub fn glass_primitive(index: u32) -> GlassPrimitive {
            let y = index as i32;
            let mut p: GlassPrimitive;
            p.bounds = PRIM_DATA.load(IVec2::new(0, y), 0);
            p.corner_radius = PRIM_DATA.load(IVec2::new(1, y), 0);
            p.tint_color = PRIM_DATA.load(IVec2::new(2, y), 0);
            p.params = PRIM_DATA.load(IVec2::new(3, y), 0);
            p.params2 = PRIM_DATA.load(IVec2::new(4, y), 0);
            let ti = PRIM_DATA.load(IVec2::new(5, y), 0);
            p.type_info = UVec4::new(ti.x.to_bits(), ti.y.to_bits(), ti.z.to_bits(), ti.w.to_bits());
            p.clip_bounds = PRIM_DATA.load(IVec2::new(6, y), 0);
            p.clip_radius = PRIM_DATA.load(IVec2::new(7, y), 0);
            p.border_color = PRIM_DATA.load(IVec2::new(8, y), 0);
            return p;
        }
    #else
        #[group(0)] #[binding(1)] static GLASS_PRIMITIVES: Storage<[GlassPrimitive]>;

        pub fn glass_primitive(index: u32) -> GlassPrimitive {
            return GLASS_PRIMITIVES[index];
        }
    #endif
}
