//! A fixed screen-space gradient behind a 3D scene. It is pinned to the screen, not a sky dome, so
//! it does not move with orbit, zoom or pan; the reflection cubemap is a separate texture.

russl::russl! {
    use super::fullscreen::{FullscreenOut, fullscreen_vertex};

    #[vertex]
    fn vs_main(#[builtin(vertex_index)] vi: u32) -> FullscreenOut {
        return fullscreen_vertex(vi, 1.0);
    }

    #[fragment]
    #[location(0)]
    fn fs_main(input: FullscreenOut) -> Vec4 {
        // Neutral studio gradient: dark edges, subtly brighter centre. uv.y = 0 at the top.
        let y = input.uv.y;

        // Vertical gradient: a parabola peaking at y = 0.5.
        let vert = 1.0 - 4.0 * (y - 0.5) * (y - 0.5);

        // Horizontal vignette: slightly dimmer at the edges.
        let x = input.uv.x;
        let horiz = 1.0 - 2.0 * (x - 0.5) * (x - 0.5);

        let brightness = 0.08 + 0.12 * vert * horiz;
        let color = Vec3::new(brightness * 0.9, brightness * 0.92, brightness);

        return Vec4::new(color, 1.0);
    }
}
