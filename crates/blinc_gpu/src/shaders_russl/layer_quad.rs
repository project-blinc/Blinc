//! The quad the layer effect passes draw over their target, with the texture coordinate flipped
//! to the texture's top-left origin.

russl::russl! {
    pub struct LayerQuadOut {
        #[builtin(position)] pub position: Vec4,
        #[location(0)] pub uv: Vec2,
    }

    pub fn layer_quad_vertex(vertex_index: u32) -> LayerQuadOut {
        let mut positions = [
            Vec2::new(-1.0, -1.0),
            Vec2::new( 1.0, -1.0),
            Vec2::new(-1.0,  1.0),
            Vec2::new( 1.0, -1.0),
            Vec2::new( 1.0,  1.0),
            Vec2::new(-1.0,  1.0),
        ];

        let mut uvs = [
            Vec2::new(0.0, 1.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(0.0, 0.0),
        ];

        let mut out: LayerQuadOut;
        out.position = Vec4::new(positions[vertex_index], 0.0, 1.0);
        out.uv = uvs[vertex_index];
        return out;
    }
}
