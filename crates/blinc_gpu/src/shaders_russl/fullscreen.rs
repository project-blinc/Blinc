//! The fullscreen triangle the post-process passes draw, and the texture coordinate it carries.

russl::russl! {
    pub struct FullscreenOut {
        #[builtin(position)] pub position: Vec4,
        #[location(0)] pub uv: Vec2,
    }

    // A triangle that covers the attachment: vertex 0 at (-1, -1), 1 at (3, -1), 2 at (-1, 3).
    // `z` is the depth it is drawn at; `uv` has its origin at the top left.
    pub fn fullscreen_vertex(vi: u32, z: f32) -> FullscreenOut {
        let x = ((vi & 1u32) as i32) as f32 * 4.0 - 1.0;
        let y = ((vi >> 1u32) as i32) as f32 * 4.0 - 1.0;
        let mut out: FullscreenOut;
        out.position = Vec4::new(x, y, z, 1.0);
        out.uv = Vec2::new((x + 1.0) * 0.5, (1.0 - y) * 0.5);
        return out;
    }
}
