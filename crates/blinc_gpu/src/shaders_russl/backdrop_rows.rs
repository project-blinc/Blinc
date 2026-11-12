//! The horizontal pass of the backdrop blur, reading its taps from an auxiliary texture, and a
//! raw premultiplied copy.

russl::russl! {
    struct Params {
        screen: Vec4,
        source: Vec4,
        taps: Vec4,
        record: [Vec4; 28],
    }

    #[group(0)] #[binding(0)] static PARAMS: Uniform<Params>;
    #[group(0)] #[binding(1)] static SOURCE: Texture2D<f32>;
    #[group(0)] #[binding(2)] static LINEAR_SAMPLER: Sampler;
    #[group(0)] #[binding(3)] static AUXILIARY: Texture2D<f32>;

    #[vertex]
    #[builtin(position)]
    fn vs_main(#[builtin(vertex_index)] index: u32) -> Vec4 {
        let mut corners = [
            Vec2::new(-1.0, -1.0), Vec2::new(1.0, -1.0), Vec2::new(-1.0, 1.0),
            Vec2::new(1.0, -1.0), Vec2::new(1.0, 1.0), Vec2::new(-1.0, 1.0),
        ];
        return Vec4::new(corners[index], 0.0, 1.0);
    }

    #[fragment]
    #[location(0)]
    fn fs_rows(#[builtin(position)] position: Vec4) -> Vec4 {
        let size = SOURCE.dimensions().as_vec2();
        let mut sum = Vec4::splat(0.0);
        for i in 0u32..((PARAMS.source.z) as u32) {
            let at = ((PARAMS.taps.x) as u32) + i;
            let tap = AUXILIARY.load(IVec2::new((at % 1024u32) as i32, (at / 1024u32) as i32), 0).xy;
            sum += SOURCE.sample_level(LINEAR_SAMPLER, (position.xy + Vec2::new(tap.x, 0.0)) / size, 0.0) * tap.y;
        }
        return sum;
    }

    // A raw premultiplied copy, including transparent pixels.
    #[fragment]
    #[location(0)]
    fn fs_copy(#[builtin(position)] position: Vec4) -> Vec4 {
        return SOURCE.load(position.xy.as_ivec2(), 0);
    }
}
