use super::*;
#[test]
fn layer_text_clip_projection_roundtrip_and_degenerate_frames() {
    blinc_theme::ThemeState::init(
        blinc_theme::HybridTheme::bundle(),
        blinc_theme::ColorScheme::Light,
    );
    let tree = blinc_layout::RenderTree::from_element(&blinc_layout::div().w(140.0).h(90.0));
    let mut info = Transform3DLayerInfo {
        node_id: tree.root().unwrap(),
        layer_bounds: [17.25, 29.5, 140.5, 90.25],
        transform_3d: blinc_core::Transform3DParams {
            sin_rx: 0.0,
            cos_rx: 1.0,
            sin_ry: 0.0,
            cos_ry: 1.0,
            perspective_d: 500.0,
        },
        opacity: 1.0,
    };
    for (rx, ry) in [
        (0.0_f32, 0.0_f32),
        (20.0, 35.0),
        (-25.0, -40.0),
        (15.0, 120.0),
    ] {
        let t = &mut info.transform_3d;
        (t.sin_rx, t.cos_rx) = rx.to_radians().sin_cos();
        (t.sin_ry, t.cos_ry) = ry.to_radians().sin_cos();
        let [lx, ly, w, h] = info.layer_bounds;
        for [x, y] in [[0.0, 0.0], [w, 0.0], [0.0, h], [w, h], [w * 0.5, h * 0.5]] {
            let t = info.transform_3d;
            let (u, v) = (x - w * 0.5, y - h * 0.5);
            let z = u * t.sin_ry;
            let denom = 1.0 + (v * t.sin_rx + z * t.cos_rx) / t.perspective_d;
            let screen = [
                lx + w * 0.5 + u * t.cos_ry / denom,
                ly + h * 0.5 + (v * t.cos_rx - z * t.sin_rx) / denom,
            ];
            let actual = layer_point(&info, screen).unwrap();
            assert!(
                (actual[0] - x).abs() < 0.001 && (actual[1] - y).abs() < 0.001,
                "{actual:?} expected {:?}",
                [x, y]
            );
        }
    }
    let clip = ClipPath::Path {
        vertices: vec![(0.0, 0.0), (50.0, 0.0), (0.0, 60.0)],
    };
    let shape = CollectedTextPath::capture(
        &clip,
        [30.5, 40.25, 100.0, 80.0],
        None,
        (1.0, 1.0),
        (0.0, 0.0),
        None,
        1.0,
        Some(info.node_id),
    )
    .unwrap();
    let local = shape.in_layer(&info);
    let g = GpuGlyph {
        bounds: [0.0, 0.0, 20.0, 20.0],
        ..Default::default()
    };
    let rows = local.prepare(&[g], None, (141, 91)).aux_data;
    assert_eq!(&rows[3][..2], &[13.25, 10.75]);
    assert_eq!(shape.frame[4..], [30.5, 40.25]);
    let outside = CollectedTextPath::capture(
        &clip,
        [30.5, 40.25, 100.0, 80.0],
        None,
        (1.0, 1.0),
        (0.0, 0.0),
        None,
        1.0,
        None,
    )
    .unwrap();
    let rows = outside
        .in_layer(&info)
        .prepare(&[g], None, (141, 91))
        .aux_data;
    assert_eq!(&rows[3][..2], &layer_point(&info, [30.5, 40.25]).unwrap());
    assert!(layer_point(&info, [f32::NAN, 0.0]).is_none());
    info.transform_3d = blinc_core::Transform3DParams {
        sin_rx: 0.0,
        cos_rx: 1.0,
        sin_ry: 1.0,
        cos_ry: 0.0,
        perspective_d: 500.0,
    };
    assert!(layer_point(&info, [87.5, 74.625]).is_none());
}
