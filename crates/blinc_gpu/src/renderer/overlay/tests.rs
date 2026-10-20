use super::*;
use crate::primitives::GpuPrimitive;

#[test]
fn layer_commands_preserve_nested_primitive_and_path_ranges() {
    let mut batch = PrimitiveBatch::new();
    batch.push(GpuPrimitive::rect(0.0, 0.0, 1.0, 1.0));
    batch.push_layer_command(LayerCommand::Push {
        config: LayerConfig::new().opacity(0.5),
    });
    batch.push(GpuPrimitive::rect(2.0, 0.0, 1.0, 1.0));
    batch.push_layer_command(LayerCommand::Push {
        config: LayerConfig::new().opacity(0.25),
    });
    batch.push_path(crate::path::tessellate_fill(
        &blinc_core::Path::rect(blinc_core::Rect::new(10.0, 10.0, 5.0, 5.0)),
        &blinc_core::Color::RED.into(),
    ));
    batch.push_layer_command(LayerCommand::Pop);
    batch.push_layer_command(LayerCommand::Pop);
    batch.push(GpuPrimitive::rect(4.0, 0.0, 1.0, 1.0));
    let nodes = nodes(&batch);
    assert!(matches!(
        nodes[0],
        Node::Run(Cursor { primitive: 0, .. }, Cursor { primitive: 1, .. })
    ));
    let Node::Layer(config, children) = &nodes[1] else {
        panic!("outer layer")
    };
    assert_eq!(config.opacity, 0.5);
    assert!(matches!(
        children[0],
        Node::Run(Cursor { primitive: 1, .. }, Cursor { primitive: 2, .. })
    ));
    let Node::Layer(config, children) = &children[1] else {
        panic!("inner layer")
    };
    assert_eq!(config.opacity, 0.25);
    let Node::Run(start, end) = children[0] else {
        panic!("path run")
    };
    assert_eq!((start.primitive, end.primitive), (2, 2));
    assert_eq!((start.vertex, end.vertex), (0, batch.paths.vertices.len()));
    assert_eq!((start.index, end.index), (0, batch.paths.indices.len()));
    assert!(matches!(
        nodes[2],
        Node::Run(Cursor { primitive: 2, .. }, Cursor { primitive: 3, .. })
    ));
}

#[test]
fn moving_mesh_payloads_preserves_colours_and_local_polygon_points() {
    let mut batch = PrimitiveBatch::new();
    batch.aux_data = vec![
        [20.0, 30.0, 60.0, 30.0],
        [20.0, 70.0, 0.0, 0.0],
        [1.0, 0.0, 0.0, 1.0],
        [0.0, 1.0, 0.0, 1.0],
        [0.0, 0.0, 1.0, 1.0],
        [0.0, 0.0, 40.0, 0.0],
        [0.0, 40.0, 0.0, 0.0],
    ];
    let mut p = GpuPrimitive::rect(20.0, 30.0, 40.0, 40.0);
    p.type_info = [PrimitiveType::Mesh as u32, 0, ClipType::Polygon as u32, 1];
    p.border[2] = 0.0;
    p.clip_radius = [1.0, 0.0, 3.0, 5.0];
    batch.primitives = vec![p, p];
    let moved = translated(
        &batch,
        Cursor::default(),
        Cursor {
            primitive: 2,
            ..Default::default()
        },
        [10.0, 20.0],
    );
    assert_eq!(
        moved.aux_data[..2],
        [[10.0, 10.0, 50.0, 10.0], [10.0, 50.0, 0.0, 0.0]]
    );
    assert_eq!(moved.aux_data[2..], batch.aux_data[2..]);
    assert_eq!(moved.primitives[0].clip_radius, p.clip_radius);
}

#[test]
fn path_style_and_uv_gradient_frames_survive_range_translation() {
    use blinc_core::{Color, Gradient, Path, Point, Rect};
    let mut batch = PrimitiveBatch::new();
    batch.push_path(crate::path::tessellate_fill(
        &Path::rect(Rect::new(20.0, 30.0, 40.0, 50.0)),
        &blinc_core::Brush::Gradient(Gradient::linear(
            Point::new(20.0, 30.0),
            Point::new(60.0, 30.0),
            Color::RED,
            Color::BLUE,
        )),
    ));
    batch.paths.use_gradient_texture = true;
    batch.paths.gradient_stops = Some(vec![blinc_core::GradientStop::new(0.0, Color::RED)]);
    let moved = translated(
        &batch,
        Cursor::default(),
        Cursor {
            vertex: batch.paths.vertices.len(),
            index: batch.paths.indices.len(),
            ..Default::default()
        },
        [10.0, 20.0],
    );
    for (original, moved) in batch.paths.vertices.iter().zip(&moved.paths.vertices) {
        assert_eq!(
            moved.position,
            [original.position[0] - 10.0, original.position[1] - 20.0]
        );
        assert_eq!(moved.uv, original.uv);
        assert_eq!(moved.gradient_params, original.gradient_params);
    }
    assert!(moved.paths.use_gradient_texture);
    assert_eq!(moved.paths.gradient_stops.as_ref().unwrap().len(), 1);
}

#[test]
fn zero_radius_blur_is_a_noop() {
    assert!(active_effects(&[LayerEffect::blur(0.0)]).is_empty());
    assert_eq!(
        active_effects(&[LayerEffect::blur(0.0), LayerEffect::blur(4.0)]).len(),
        1
    );
}

#[test]
fn alpha_shader_validates() {
    let module = naga::front::wgsl::parse_str(super::alpha::SHADER).unwrap();
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap();
}

#[test]
fn composite_scratch_path_only_footprint_includes_effects() {
    let mut batch = PrimitiveBatch::new();
    let mut config = LayerConfig::new().opacity(0.5);
    config.effects.push(LayerEffect::blur(2.0));
    batch.push_layer_command(LayerCommand::Push { config });
    batch.push_path(crate::path::tessellate_fill(
        &blinc_core::Path::rect(blinc_core::Rect::new(20.0, 30.0, 40.0, 50.0)),
        &blinc_core::Color::RED.into(),
    ));
    assert_eq!(
        GpuRenderer::composite_subtree_bounds(&batch, [100.0, 100.0]),
        Some([16.0, 26.0, 48.0, 58.0])
    );
}
