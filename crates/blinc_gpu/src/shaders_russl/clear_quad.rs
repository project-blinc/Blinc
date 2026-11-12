//! Compositor damage-rect scissored clear: one fullscreen triangle that writes opaque black to
//! every pixel inside the active scissor rectangle. A scissored draw is the portable way to clear
//! a region, because a clear load op ignores the scissor; the pipeline's blend state is REPLACE.

russl::russl! {
    use super::fullscreen::{FullscreenOut, fullscreen_vertex};

    #[vertex]
    fn vs_main(#[builtin(vertex_index)] vi: u32) -> FullscreenOut {
        return fullscreen_vertex(vi, 0.0);
    }

    #[fragment]
    #[location(0)]
    fn fs_main() -> Vec4 {
        return Vec4::new(0.0, 0.0, 0.0, 1.0);
    }
}
