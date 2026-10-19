//! Dumps ABI display-list records for a few scenes as raw little-endian f32.
use blinc_abi::context::{LayoutContext, Node, Style};
use blinc_abi::scene::blinc_core::{
    Affine2D, Brush, ClipLength, ClipPath, Color, CornerRadius, Gradient, GradientSpace, GradientSpread, GradientStop,
    MaskImage, OverflowFade, Point, Shadow, Transform,
};
use blinc_abi::scene::{PaintOptions, RenderProps, SceneEncoder};
use blinc_layout::element::{BorderSide, BorderSides};
use blinc_layout::element_style::CssFilter;
use blinc_layout::tree::TextMeasureContext;
use taffy::prelude::*;
use taffy::Overflow;

fn boxed(cx: &mut LayoutContext, w: f32, h: f32, props: RenderProps) -> Node {
    let n = cx
        .create_node(Style {
            size: Size { width: length(w), height: length(h) },
            ..Default::default()
        })
        .unwrap();
    cx.set_properties(n, props).unwrap();
    n
}

fn scene(name: &str, build: impl FnOnce(&mut LayoutContext) -> Node) {
    scaled(name, 1.0, build)
}

fn scaled(name: &str, scale: f32, build: impl FnOnce(&mut LayoutContext) -> Node) {
    let mut cx = LayoutContext::new();
    let content = build(&mut cx);
    let root = cx
        .create_node(Style {
            display: Display::Flex,
            align_items: Some(AlignItems::FLEX_START),
            size: Size { width: length(200.0), height: length(120.0) },
            padding: Rect::length(20.0),
            ..Default::default()
        })
        .unwrap();
    cx.insert_before(root, content, None).unwrap();
    cx.compute(root, 200.0, 120.0).unwrap();
    let mut enc = SceneEncoder::new();
    let info = enc.prepare(&cx, root, PaintOptions { scale, ..PaintOptions::default() }).unwrap();
    let mut out = vec![0f32; info.floats];
    enc.read(&cx, &mut out).unwrap();
    // The whole buffer: the records, then any stop tables after them.
    let bytes: Vec<u8> = out.iter().flat_map(|f| f.to_le_bytes()).collect();
    std::fs::write(format!("out/{name}.f32"), bytes).unwrap();
    println!("{name}: {} records, {} floats", info.count, info.floats);
}


/// A line of text in a padded page, with the mask atlas it draws from, cropped to the
/// region its glyphs use: `[width, height, x, y, w, h]` as u32, then the crop's bytes.
fn text_scene(name: &str, scale: f32, content: &str, size: f32, fade: bool) {
    let mut cx = LayoutContext::new();
    let text = cx
        .create_text(
            Style::default(),
            TextMeasureContext {
                content: content.into(),
                font_size: size,
                line_height: 1.2,
                letter_spacing: 0.0,
                wrap: false,
                font_name: None,
                generic_font: Default::default(),
                font_weight: 400,
                italic: false,
            },
        )
        .unwrap();
    let root = cx
        .create_node(Style {
            display: Display::Flex,
            align_items: Some(AlignItems::FLEX_START),
            size: Size { width: length(200.0), height: length(120.0) },
            padding: Rect::length(20.0),
            ..Default::default()
        })
        .unwrap();
    let content = if fade || name.starts_with("shape_text") {
        let parent = cx.create_node(Style {
            size: Size { width: length(100.0), height: length(60.0) },
            overflow: taffy::Point { x: Overflow::Clip, y: Overflow::Clip },
            ..Default::default()
        }).unwrap();
        let mut props = RenderProps::default();
        if fade { props.overflow_fade = OverflowFade::new(20.0, 32.0, 24.0, 28.0); }
        if name.starts_with("shape_text") {
            props.clip_path = Some(ClipPath::Ellipse {
                rx: Some(ClipLength::Px(26.0)), ry: Some(ClipLength::Px(13.0)),
                center: (ClipLength::Px(30.0), ClipLength::Px(15.0)),
            });
        }
        cx.set_properties(parent, props).unwrap();
        cx.insert_before(parent, text, None).unwrap();
        parent
    } else { text };
    cx.insert_before(root, content, None).unwrap();
    cx.compute(root, 200.0, 120.0).unwrap();
    let mut enc = SceneEncoder::new();
    let info = enc
        .prepare(&cx, root, PaintOptions { scale, ..PaintOptions::default() })
        .unwrap();
    let mut out = vec![0f32; info.floats];
    enc.read(&cx, &mut out).unwrap();
    out.truncate(info.count * 112);
    let bytes: Vec<u8> = out.iter().flat_map(|f| f.to_le_bytes()).collect();
    std::fs::write(format!("out/{name}.f32"), bytes).unwrap();

    let atlas = enc.atlas_info(false, 0).unwrap().expect("a mask atlas");
    let mut pixels = vec![0u8; atlas.bytes()];
    enc.read_atlas(false, 0, &mut pixels).unwrap().unwrap();
    assert_eq!((atlas.x, atlas.y, atlas.update_width, atlas.update_height), (0, 0, atlas.width, atlas.height));
    let (w, h) = (atlas.width as usize, atlas.height as usize);
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0usize, 0usize);
    for y in 0..h {
        for x in 0..w {
            if pixels[y * w + x] != 0 {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x + 1);
                y1 = y1.max(y + 1);
            }
        }
    }
    let (cw, ch) = (x1 - x0, y1 - y0);
    let mut file = Vec::new();
    for v in [w, h, x0, y0, cw, ch] {
        file.extend_from_slice(&(v as u32).to_le_bytes());
    }
    for y in 0..ch {
        file.extend_from_slice(&pixels[(y0 + y) * w + x0..(y0 + y) * w + x0 + cw]);
    }
    std::fs::write(format!("out/{name}.atlas"), file).unwrap();
    println!("{name}: {} records, atlas {w}x{h}, crop {cw}x{ch} at ({x0},{y0})", info.count);
}

fn solid(r: f32, g: f32, b: f32) -> Option<Brush> {
    Some(Brush::Solid(Color::rgb(r, g, b)))
}

/// An overflowing fill under an optionally cut, turned or layered fading clip.
fn fading(cx: &mut LayoutContext, cut: bool, turned: bool, layered: bool, shadow: bool) -> Node {
    let (w, h) = if cut { (100.0, 80.0) } else { (100.0, 60.0) };
    let parent = cx.create_node(Style {
        position: if cut { Position::Absolute } else { Position::Relative },
        inset: if cut { Rect { left: length(-10.0), top: length(-10.0), right: auto(), bottom: auto() } } else { Rect::auto() },
        size: Size { width: length(w), height: length(h) },
        overflow: taffy::Point { x: Overflow::Clip, y: Overflow::Clip },
        ..Default::default()
    }).unwrap();
    let mut props = RenderProps::default();
    props.overflow_fade = OverflowFade::new(20.0, 32.0, 24.0, 28.0);
    if turned { props.transform = Some(Transform::rotate(0.4)); }
    cx.set_properties(parent, props).unwrap();
    let mut fill = RenderProps::default();
    if shadow {
        fill.shadow = vec![Shadow { offset_x: 0.0, offset_y: 0.0, blur: 0.0, spread: 12.0, color: Color::rgb(0.2, 0.5, 0.9) }];
    } else {
        fill.background = solid(0.2, 0.5, 0.9);
    }
    let child = if shadow {
        let child = cx.create_node(Style {
            position: Position::Absolute,
            inset: Rect { left: length(20.0), top: length(20.0), right: auto(), bottom: auto() },
            size: Size { width: length(60.0), height: length(20.0) },
            ..Default::default()
        }).unwrap();
        cx.set_properties(child, fill).unwrap();
        child
    } else { boxed(cx, w, h, fill) };
    cx.insert_before(parent, child, None).unwrap();
    if !cut { return parent; }
    let outer = cx.create_node(Style {
        size: Size { width: length(70.0), height: length(60.0) },
        overflow: taffy::Point { x: Overflow::Clip, y: Overflow::Clip },
        ..Default::default()
    }).unwrap();
    let mut props = RenderProps::default();
    props.overflow_fade = OverflowFade::uniform(3.0);
    if layered {
        props.opacity = 0.5;
        props.filter = Some(CssFilter { brightness: 0.5, ..CssFilter::default() });
    }
    cx.set_properties(outer, props).unwrap();
    cx.insert_before(outer, parent, None).unwrap();
    outer
}

/// Shapes and edge modifiers encoded by the ABI rather than hand-packed records.
fn notch_scene(cx: &mut LayoutContext, name: &str) -> Node {
    let mut props = RenderProps::default();
    props.background = solid(0.2, 0.5, 0.9);
    let mut shape = [[-18.0, -12.0, 10.0, 6.0], [0.0; 4], [0.0; 4]];
    let mut height = 60.0;
    match name {
        "notch_scoop" => shape = [[6.0; 4], [1.0, 40.0, 12.0, 4.0], [1.0, 24.0, 18.0, 3.0]],
        "notch_bulge" => shape = [[8.0; 4], [2.0, 50.0, 12.0, 4.0], [2.0, 38.0, 10.0, 3.0]],
        "notch_v" => shape = [[4.0; 4], [3.0, 36.0, 14.0, 0.0], [4.0, 30.0, 12.0, 0.0]],
        "notch_short" => { shape[0] = [-24.0, -16.0, 6.0, 6.0]; height = 32.0; }
        "notch_border" => {
            props.border_sides = BorderSides {
                top: Some(BorderSide::new(3.0, Color::rgba(0.8, 0.1, 0.2, 0.6))),
                right: Some(BorderSide::new(1.0, Color::rgb(0.1, 0.9, 0.1))),
                bottom: Some(BorderSide::new(5.0, Color::rgb(0.7, 0.1, 0.9))),
                left: Some(BorderSide::new(2.0, Color::rgb(0.1, 0.9, 0.9))),
            };
        }
        "notch_shadow" | "notch_inset" => {
            props.background = None;
            let shadow = Shadow { offset_x: 4.0, offset_y: 5.0, blur: 6.0, spread: 2.0, color: Color::rgba(0.2, 0.4, 0.8, 0.7) };
            if name == "notch_inset" { props.inner_shadow = vec![shadow]; } else { props.shadow = vec![shadow]; }
            props.transform = Some(Transform::rotate(0.2));
        }
        _ => {}
    }
    if name == "notch_gradient" || name == "notch_layer" {
        props.background = Some(Brush::Gradient(Gradient::Conic {
            center: Point::new(50.0, 30.0),
            start_angle: 0.3,
            space: GradientSpace::UserSpace,
            stops: vec![
                GradientStop::new(0.0, Color::rgba(1.0, 0.1, 0.1, 0.8)),
                GradientStop::new(0.2, Color::rgba(0.9, 0.9, 0.1, 0.6)),
                GradientStop::new(0.5, Color::rgb(0.1, 0.8, 0.2)),
                GradientStop::new(0.8, Color::rgb(0.1, 0.6, 1.0)),
                GradientStop::new(1.0, Color::rgb(0.6, 0.1, 0.9)),
            ],
        }));
        if name == "notch_layer" {
            props.opacity = 0.5;
            props.filter = Some(CssFilter { brightness: 0.8, ..CssFilter::default() });
        }
    }
    if name == "notch_affine" {
        props.transform = Some(Transform::Affine2D(Affine2D { elements: [1.2, 0.2, -0.15, 0.8, 0.0, 0.0] }));
    }
    let node = boxed(cx, 100.0, height, props);
    cx.set_notch(node, Some(shape)).unwrap();
    if name != "notch_clip" { return node; }
    let parent = cx.create_node(Style {
        size: Size { width: length(80.0), height: length(50.0) },
        overflow: taffy::Point { x: Overflow::Clip, y: Overflow::Clip },
        ..Default::default()
    }).unwrap();
    let mut clip = RenderProps::default();
    clip.border_radius = CornerRadius::uniform(10.0);
    clip.transform = Some(Transform::rotate(0.3));
    clip.overflow_fade = OverflowFade::new(8.0, 12.0, 6.0, 10.0);
    cx.set_properties(parent, clip).unwrap();
    cx.insert_before(parent, node, None).unwrap();
    parent
}

fn shape_path(name: &str) -> ClipPath {
    use ClipLength::{Percent as Pc, Px};
    match name {
        "shape_rect" => ClipPath::Inset { top: Px(5.0), right: Pc(12.0), bottom: Px(7.0), left: Px(9.0), round: Some(10.0) },
        "shape_polygon" | "shape_layer" | "shape_notch" => ClipPath::Polygon {
            points: vec![(Px(5.0), Px(7.0)), (Px(95.0), Px(4.0)), (Px(80.0), Px(52.0)), (Px(55.0), Px(32.0)), (Px(25.0), Px(57.0))],
        },
        "shape_hole_even" | "shape_hole_nonzero" => ClipPath::Path {
            vertices: vec![(0.0, 0.0), (100.0, 0.0), (100.0, 60.0), (0.0, 60.0), (0.0, 0.0), (1e30, 1e30),
                (30.0, 15.0), (70.0, 15.0), (70.0, 45.0), (30.0, 45.0), (30.0, 15.0)],
        },
        _ => ClipPath::Ellipse { rx: Some(Pc(42.0)), ry: Some(Pc(34.0)), center: (Pc(55.0), Pc(45.0)) },
    }
}

fn shape_scene(cx: &mut LayoutContext, name: &str) -> Node {
    let mut p = RenderProps::default();
    p.background = solid(0.9, 0.2, 0.2);
    p.clip_path = Some(shape_path(name));
    if name == "shape_affine" { p.transform = Some(Transform::Affine2D(Affine2D { elements: [1.1, 0.3, -0.2, 0.9, 0.0, 0.0] })); }
    if name == "shape_inherited" {
        p.transform = Some(Transform::rotate(0.35));
        let parent = boxed(cx, 100.0, 60.0, p);
        let mut child_props = RenderProps::default();
        child_props.background = solid(0.1, 0.7, 0.4);
        child_props.transform = Some(Transform::Affine2D(Affine2D { elements: [0.95, -0.2, 0.3, 1.1, 0.0, 0.0] }));
        let child = boxed(cx, 140.0, 70.0, child_props);
        cx.insert_before(parent, child, None).unwrap();
        return parent;
    }
    if name == "shape_shadow" {
        p.background = None;
        let parent = boxed(cx, 100.0, 60.0, p);
        let mut shadow = RenderProps::default();
        shadow.shadow = vec![Shadow { offset_x: 4.0, offset_y: 3.0, blur: 0.0, spread: 8.0, color: Color::rgba(0.2, 0.4, 0.8, 0.7) }];
        let child = cx.create_node(Style {
            position: Position::Absolute,
            inset: Rect { left: length(20.0), top: length(20.0), right: auto(), bottom: auto() },
            size: Size { width: length(50.0), height: length(20.0) }, ..Default::default()
        }).unwrap();
        cx.set_properties(child, shadow).unwrap();
        cx.insert_before(parent, child, None).unwrap();
        return parent;
    }
    if name == "shape_layer" {
        let mut props = RenderProps::default();
        props.clip_path = Some(shape_path(name));
        props.background = Some(Brush::Gradient(Gradient::Conic {
            center: Point::new(50.0, 30.0), start_angle: 0.3, space: GradientSpace::UserSpace,
            stops: vec![GradientStop::new(0.0, Color::rgb(1.0, 0.1, 0.1)), GradientStop::new(0.2, Color::rgb(0.9, 0.9, 0.1)), GradientStop::new(0.5, Color::rgb(0.1, 0.8, 0.2)), GradientStop::new(0.8, Color::rgb(0.1, 0.6, 1.0)), GradientStop::new(1.0, Color::rgb(0.6, 0.1, 0.9))],
        }));
        props.filter = Some(CssFilter { brightness: 0.8, ..CssFilter::default() });
        props.opacity = 0.5;
        props.transform = Some(Transform::rotate(0.2));
        return boxed(cx, 100.0, 60.0, props);
    }
    if name == "shape_flatten" {
        let outer = boxed(cx, 150.0, 60.0, RenderProps::default());
        for i in 0..3 {
            let mut props = RenderProps::default();
            props.background = solid(0.2 + i as f32 * 0.2, 0.5, 0.8);
            props.clip_path = Some(shape_path("shape_ellipse"));
            if i == 1 {
                props.clip_path = Some(shape_path("shape_polygon"));
                props.mask_image = Some(MaskImage::Gradient(Gradient::Linear {
                    start: Point::new(0.0, 0.0), end: Point::new(1.0, 0.0), space: GradientSpace::ObjectBoundingBox,
                    spread: GradientSpread::Pad,
                    stops: (0..17).map(|n| GradientStop::new(n as f32 / 16.0, Color::rgba(0.0, 0.0, 0.0, n as f32 / 16.0))).collect(),
                }));
            }
            let node = boxed(cx, 45.0, 40.0, props);
            cx.insert_before(outer, node, None).unwrap();
        }
        return outer;
    }
    let node = boxed(cx, 100.0, 60.0, p);
    if name == "shape_notch" { cx.set_notch(node, Some([[-18.0, -12.0, 10.0, 6.0], [0.0; 4], [0.0; 4]])).unwrap(); }
    if name == "shape_hole_even" { cx.set_clip_even_odd(node, true).unwrap(); }
    if name == "shape_image" { cx.set_resource(node, Some(7), false).unwrap(); }
    node
}

fn clipped_image(cx: &mut LayoutContext, name: &str) -> Node {
    let local = name == "image_local" || name == "image_clip_layer" || name == "image_clip_nested";
    let cut = name == "image_cut_fade";
    let shape = name.strip_prefix("image_shape_");
    let parent = cx.create_node(Style {
        position: if cut { Position::Absolute } else { Position::Relative },
        inset: if cut { Rect { left: length(-10.0), top: length(-8.0), right: auto(), bottom: auto() } } else { Rect::auto() },
        size: Size { width: length(100.0), height: length(60.0) },
        overflow: taffy::Point { x: Overflow::Clip, y: Overflow::Clip },
        ..Default::default()
    }).unwrap();
    let mut props = RenderProps::default();
    props.border_radius = CornerRadius::uniform(18.0);
    if local { props.transform = Some(Transform::Affine2D(Affine2D { elements: [1.1, 0.3, -0.2, 0.9, 0.0, 0.0] })); }
    if cut || name.contains("clip_") { props.overflow_fade = OverflowFade::new(12.0, 22.0, 14.0, 18.0); }
    if let Some(shape) = shape { props.clip_path = Some(shape_path(&format!("shape_{shape}"))); }
    if shape == Some("affine") { props.transform = Some(Transform::rotate(0.25)); }
    if name.contains("clip_") { props.clip_path = Some(shape_path("shape_polygon")); }
    if name == "image_corner" { props.corner_shape = blinc_core::CornerShape::BEVEL; }
    cx.set_properties(parent, props).unwrap();
    if shape == Some("hole_even") { cx.set_clip_even_odd(parent, true).unwrap(); }
    let child = cx.create_node(Style {
        position: Position::Absolute,
        inset: Rect { left: length(-12.0), top: length(-9.0), right: auto(), bottom: auto() },
        size: Size { width: length(140.0), height: length(80.0) },
        ..Default::default()
    }).unwrap();
    if shape == Some("affine") {
        let mut props = RenderProps::default();
        props.transform = Some(Transform::Affine2D(Affine2D { elements: [0.95, -0.2, 0.3, 1.1, 0.0, 0.0] }));
        cx.set_properties(child, props).unwrap();
    }
    cx.set_resource(child, Some(7), false).unwrap();
    cx.insert_before(parent, child, None).unwrap();
    let mut node = parent;
    if cut {
        let outer = cx.create_node(Style {
            size: Size { width: length(70.0), height: length(50.0) },
            overflow: taffy::Point { x: Overflow::Clip, y: Overflow::Clip },
            ..Default::default()
        }).unwrap();
        cx.insert_before(outer, node, None).unwrap();
        node = outer;
    }
    let levels = if name == "image_clip_nested" { 2 } else if name == "image_clip_layer" { 1 } else { 0 };
    for _ in 0..levels {
        let mut props = RenderProps::default();
        props.opacity = 0.6;
        props.filter = Some(CssFilter { brightness: 0.8, ..CssFilter::default() });
        let outer = boxed(cx, 120.0, 80.0, props);
        cx.insert_before(outer, node, None).unwrap();
        node = outer;
    }
    if levels > 0 {
        let mut props = RenderProps::default();
        props.transform = Some(Transform::translate(32.0, 24.0));
        let holder = boxed(cx, 160.0, 100.0, props);
        cx.insert_before(holder, node, None).unwrap();
        node = holder;
    }
    node
}

fn main() {
    for name in ["image_local", "image_cut_fade", "image_shape_ellipse", "image_shape_affine", "image_shape_rect", "image_shape_polygon", "image_shape_hole_even", "image_shape_hole_nonzero", "image_corner", "image_clip_layer", "image_clip_nested"] {
        scene(name, |cx| clipped_image(cx, name));
    }
    scene("image_clip_pair", |cx| {
        let parent = boxed(cx, 160.0, 80.0, RenderProps::default());
        for (name, x) in [("image_shape_polygon", 0.0), ("image_shape_rect", 50.0)] {
            let node = clipped_image(cx, name);
            cx.set_style(node, Style {
                position: Position::Absolute,
                inset: Rect { left: length(x), top: length(0.0), right: auto(), bottom: auto() },
                size: Size { width: length(100.0), height: length(60.0) },
                overflow: taffy::Point { x: Overflow::Clip, y: Overflow::Clip },
                ..Default::default()
            }).unwrap();
            cx.insert_before(parent, node, None).unwrap();
        }
        parent
    });
    for name in ["shape_ellipse", "shape_rect", "shape_polygon", "shape_hole_even", "shape_hole_nonzero", "shape_affine", "shape_inherited", "shape_shadow", "shape_notch", "shape_layer", "shape_flatten", "shape_image"] {
        scene(name, |cx| shape_scene(cx, name));
    }
    text_scene("shape_text_x1", 1.0, "Hello", 20.0, false);
    text_scene("shape_text_x2", 2.0, "Hello", 20.0, false);

    for name in ["notch_corners", "notch_scoop", "notch_bulge", "notch_v", "notch_short", "notch_border", "notch_gradient", "notch_layer", "notch_shadow", "notch_inset", "notch_affine", "notch_clip"] {
        scene(name, |cx| notch_scene(cx, name));
    }

    text_scene("fade_text_x1", 1.0, "Hello", 20.0, true);
    text_scene("fade_text_x2", 2.0, "Hello", 20.0, true);
    scene("fade_image", |cx| {
        let parent = fading(cx, false, false, false, false);
        let image = boxed(cx, 40.0, 30.0, RenderProps::default());
        cx.set_resource(image, Some(7), false).unwrap();
        cx.insert_before(parent, image, None).unwrap();
        parent
    });
    scene("fade_plain", |cx| fading(cx, false, false, false, false));
    scene("fade_cut", |cx| fading(cx, true, false, false, false));
    scene("fade_turned", |cx| fading(cx, false, true, false, false));
    scene("fade_layer", |cx| fading(cx, true, false, true, false));
    scene("fade_shadow", |cx| fading(cx, false, false, false, true));
    scene("box_solid", |cx| {
        let mut p = RenderProps::default();
        p.background = solid(0.2, 0.4, 0.9);
        p.border_radius = CornerRadius::uniform(8.0);
        boxed(cx, 100.0, 60.0, p)
    });
    scene("box_border", |cx| {
        let mut p = RenderProps::default();
        p.background = solid(0.2, 0.4, 0.9);
        p.border_radius = CornerRadius::uniform(12.0);
        p.border_width = 3.0;
        p.border_color = Some(Color::rgb(0.9, 0.1, 0.1));
        boxed(cx, 100.0, 60.0, p)
    });
    scene("box_sides", |cx| {
        let mut p = RenderProps::default();
        p.background = solid(0.95, 0.95, 0.95);
        p.border_sides = BorderSides {
            top: Some(BorderSide::new(4.0, Color::rgb(0.9, 0.1, 0.1))),
            right: Some(BorderSide::new(2.0, Color::rgb(0.1, 0.7, 0.1))),
            bottom: Some(BorderSide::new(6.0, Color::rgb(0.1, 0.1, 0.9))),
            left: Some(BorderSide::new(3.0, Color::rgb(0.9, 0.7, 0.1))),
        };
        boxed(cx, 100.0, 60.0, p)
    });
    scene("box_shadow", |cx| {
        let mut p = RenderProps::default();
        p.background = solid(1.0, 1.0, 1.0);
        p.border_radius = CornerRadius::uniform(10.0);
        p.shadow = vec![Shadow {
            offset_x: 4.0,
            offset_y: 6.0,
            blur: 12.0,
            spread: 2.0,
            color: Color::rgba(0.0, 0.0, 0.0, 0.4),
        }];
        boxed(cx, 100.0, 60.0, p)
    });
    scene("box_rotated", |cx| {
        let mut p = RenderProps::default();
        p.background = solid(0.2, 0.7, 0.4);
        p.border_radius = CornerRadius::uniform(6.0);
        p.transform = Some(Transform::rotate(0.5));
        boxed(cx, 100.0, 60.0, p)
    });
    scene("box_scaled", |cx| {
        let mut p = RenderProps::default();
        p.background = solid(0.7, 0.3, 0.6);
        p.transform = Some(Transform::Affine2D(Affine2D {
            elements: [1.5, 0.0, 0.0, 0.75, 0.0, 0.0],
        }));
        boxed(cx, 100.0, 60.0, p)
    });
    scene("box_gradient", |cx| {
        let mut p = RenderProps::default();
        p.background = Some(Brush::Gradient(Gradient::linear(
            Point::new(0.0, 0.0),
            Point::new(100.0, 0.0),
            Color::rgb(1.0, 0.2, 0.2),
            Color::rgb(0.2, 0.2, 1.0),
        )));
        p.border_radius = CornerRadius::uniform(8.0);
        boxed(cx, 100.0, 60.0, p)
    });
    scene("box_radial", |cx| {
        let mut p = RenderProps::default();
        p.background = Some(Brush::Gradient(Gradient::radial(
            Point::new(50.0, 30.0),
            40.0,
            Color::rgb(1.0, 1.0, 0.2),
            Color::rgb(0.1, 0.1, 0.5),
        )));
        boxed(cx, 100.0, 60.0, p)
    });
    scene("clip_child", |cx| {
        let mut outer = RenderProps::default();
        outer.background = solid(0.9, 0.9, 0.9);
        outer.border_radius = CornerRadius::uniform(16.0);
        outer.clips_content = true;
        let parent = cx
            .create_node(Style {
                size: Size { width: length(100.0), height: length(60.0) },
                overflow: taffy::Point { x: Overflow::Clip, y: Overflow::Clip },
                ..Default::default()
            })
            .unwrap();
        cx.set_properties(parent, outer).unwrap();
        let mut inner = RenderProps::default();
        inner.background = solid(0.9, 0.2, 0.2);
        let child = cx
            .create_node(Style {
                position: Position::Absolute,
                inset: Rect { left: length(-10.0), top: length(-10.0), right: auto(), bottom: auto() },
                size: Size { width: length(60.0), height: length(40.0) },
                ..Default::default()
            })
            .unwrap();
        cx.set_properties(child, inner).unwrap();
        cx.insert_before(parent, child, None).unwrap();
        parent
    });
    scaled("box_solid_x2", 2.0, |cx| {
        let mut p = RenderProps::default();
        p.background = solid(0.2, 0.4, 0.9);
        p.border_radius = CornerRadius::uniform(8.0);
        p.border_width = 2.0;
        p.border_color = Some(Color::rgb(0.9, 0.1, 0.1));
        boxed(cx, 100.0, 60.0, p)
    });
    text_scene("text_hello_x1", 1.0, "Hello", 20.0, false);
    text_scene("text_hello_x2", 2.0, "Hello", 20.0, false);
    scene("image_plain", |cx| {
        let n = boxed(cx, 40.0, 30.0, RenderProps::default());
        cx.set_resource(n, Some(7), false).unwrap();
        n
    });
    scene("image_clipped", |cx| {
        let parent = cx
            .create_node(Style {
                size: Size { width: length(100.0), height: length(60.0) },
                overflow: taffy::Point { x: Overflow::Clip, y: Overflow::Clip },
                ..Default::default()
            })
            .unwrap();
        let mut outer = RenderProps::default();
        outer.border_radius = CornerRadius::uniform(16.0);
        cx.set_properties(parent, outer).unwrap();
        let image = boxed(cx, 100.0, 60.0, RenderProps::default());
        cx.set_resource(image, Some(7), false).unwrap();
        cx.insert_before(parent, image, None).unwrap();
        parent
    });
    scene("image_faded", |cx| {
        let mut p = RenderProps::default();
        p.opacity = 0.5;
        let n = boxed(cx, 40.0, 30.0, p);
        cx.set_resource(n, Some(7), false).unwrap();
        n
    });
    scene("layer_opacity", |cx| {
        let mut group = RenderProps::default();
        group.opacity = 0.5;
        let parent = boxed(cx, 100.0, 60.0, group);
        let mut a = RenderProps::default();
        a.background = solid(0.9, 0.1, 0.1);
        let red = boxed(cx, 60.0, 40.0, a);
        let mut b = RenderProps::default();
        b.background = solid(0.1, 0.8, 0.1);
        let green = cx
            .create_node(Style {
                position: Position::Absolute,
                inset: Rect { left: length(30.0), top: length(20.0), right: auto(), bottom: auto() },
                size: Size { width: length(60.0), height: length(40.0) },
                ..Default::default()
            })
            .unwrap();
        cx.set_properties(green, b).unwrap();
        cx.insert_before(parent, red, None).unwrap();
        cx.insert_before(parent, green, None).unwrap();
        parent
    });
    scene("layer_blur", |cx| {
        let mut p = RenderProps::default();
        p.background = solid(0.9, 0.1, 0.1);
        p.filter = Some(CssFilter { blur: 4.0, ..CssFilter::default() });
        boxed(cx, 40.0, 30.0, p)
    });
    scene("layer_grayscale", |cx| {
        let mut p = RenderProps::default();
        p.background = solid(0.9, 0.1, 0.1);
        p.filter = Some(CssFilter { grayscale: 1.0, ..CssFilter::default() });
        boxed(cx, 40.0, 30.0, p)
    });
    scene("layer_shadow", |cx| {
        let mut p = RenderProps::default();
        p.background = solid(0.9, 0.1, 0.1);
        p.filter = Some(CssFilter {
            drop_shadow: Some(Shadow {
                offset_x: 6.0,
                offset_y: 5.0,
                blur: 8.0,
                spread: 0.0,
                color: Color::rgba(0.0, 0.0, 0.0, 0.6),
            }),
            ..CssFilter::default()
        });
        boxed(cx, 40.0, 30.0, p)
    });

    let c = |r: f32, g: f32, b: f32| Color::rgb(r, g, b);
    let stops3 = vec![
        GradientStop::new(0.0, c(1.0, 0.1, 0.1)),
        GradientStop::new(0.5, c(1.0, 0.9, 0.1)),
        GradientStop::new(1.0, c(0.1, 0.1, 1.0)),
    ];
    let stops5 = vec![
        GradientStop::new(0.0, c(1.0, 0.1, 0.1)),
        GradientStop::new(0.2, c(1.0, 0.9, 0.1)),
        GradientStop::new(0.5, c(0.1, 0.8, 0.2)),
        GradientStop::new(0.8, c(0.1, 0.6, 1.0)),
        GradientStop::new(1.0, c(0.6, 0.1, 0.9)),
    ];
    let line = |stops: Vec<GradientStop>, spread: GradientSpread, to: f32| {
        let mut g = Gradient::linear_with_stops(Point::new(0.0, 0.0), Point::new(to, 0.0), stops);
        if let Gradient::Linear { spread: s, .. } = &mut g {
            *s = spread;
        }
        g
    };
    let paint = |g: Gradient| {
        let mut p = RenderProps::default();
        p.background = Some(Brush::Gradient(g));
        p
    };
    scene("grad_three", |cx| boxed(cx, 100.0, 60.0, paint(line(stops3.clone(), GradientSpread::Pad, 100.0))));
    scene("grad_offsets", |cx| {
        let stops = vec![
            GradientStop::new(0.25, c(1.0, 0.1, 0.1)),
            GradientStop::new(0.75, c(0.1, 0.1, 1.0)),
        ];
        boxed(cx, 100.0, 60.0, paint(line(stops, GradientSpread::Pad, 100.0)))
    });
    scene("grad_five", |cx| boxed(cx, 100.0, 60.0, paint(line(stops5.clone(), GradientSpread::Pad, 100.0))));
    scene("grad_repeat", |cx| {
        let stops = vec![
            GradientStop::new(0.0, c(1.0, 0.1, 0.1)),
            GradientStop::new(1.0, c(0.1, 0.1, 1.0)),
        ];
        boxed(cx, 100.0, 60.0, paint(line(stops, GradientSpread::Repeat, 40.0)))
    });
    scene("grad_reflect_radial", |cx| {
        let mut g = Gradient::radial_with_stops(Point::new(50.0, 30.0), 20.0, stops3.clone());
        if let Gradient::Radial { spread: s, .. } = &mut g {
            *s = GradientSpread::Reflect;
        }
        boxed(cx, 100.0, 60.0, paint(g))
    });
    scene("grad_radial_stops", |cx| {
        boxed(cx, 100.0, 60.0, paint(Gradient::radial_with_stops(Point::new(50.0, 30.0), 45.0, stops3.clone())))
    });
    scene("grad_conic", |cx| {
        let g = Gradient::Conic {
            center: Point::new(50.0, 30.0),
            stops: vec![
                GradientStop::new(0.0, c(1.0, 0.1, 0.1)),
                GradientStop::new(1.0, c(0.1, 0.1, 1.0)),
            ],
            space: blinc_abi::scene::blinc_core::GradientSpace::UserSpace,
            start_angle: 0.0,
        };
        boxed(cx, 100.0, 60.0, paint(g))
    });
    scene("grad_conic_stops", |cx| {
        let g = Gradient::Conic {
            center: Point::new(50.0, 30.0),
            stops: stops5.clone(),
            space: blinc_abi::scene::blinc_core::GradientSpace::UserSpace,
            start_angle: 0.5,
        };
        boxed(cx, 100.0, 60.0, paint(g))
    });

    let alpha = |o: f32, a: f32| GradientStop::new(o, Color::rgba(0.0, 0.0, 0.0, a));
    let masked = |g: Gradient| {
        let mut p = RenderProps::default();
        p.background = solid(0.9, 0.1, 0.1);
        p.mask_image = Some(MaskImage::Gradient(g));
        p
    };
    let obb_line = |stops: Vec<GradientStop>| {
        let mut g = Gradient::linear_with_stops(Point::new(0.0, 0.0), Point::new(1.0, 0.0), stops);
        if let Gradient::Linear { space, .. } = &mut g {
            *space = GradientSpace::ObjectBoundingBox;
        }
        g
    };
    scene("mask_linear", |cx| boxed(cx, 100.0, 60.0, masked(obb_line(vec![alpha(0.0, 1.0), alpha(1.0, 0.0)]))));
    scene("mask_stops", |cx| {
        let stops = vec![alpha(0.0, 1.0), alpha(0.25, 0.2), alpha(0.5, 0.8), alpha(0.75, 0.0), alpha(1.0, 1.0)];
        boxed(cx, 100.0, 60.0, masked(obb_line(stops)))
    });
    scene("mask_radial", |cx| {
        let mut g = Gradient::radial_with_stops(
            Point::new(0.5, 0.5),
            0.5,
            vec![alpha(0.0, 1.0), alpha(1.0, 0.0)],
        );
        if let Gradient::Radial { space, .. } = &mut g {
            *space = GradientSpace::ObjectBoundingBox;
        }
        boxed(cx, 100.0, 60.0, masked(g))
    });
    scene("mask_conic", |cx| {
        let g = Gradient::Conic {
            center: Point::new(0.5, 0.5),
            stops: vec![alpha(0.0, 0.1), alpha(1.0, 1.0)],
            space: GradientSpace::ObjectBoundingBox,
            start_angle: 0.0,
        };
        boxed(cx, 100.0, 60.0, masked(g))
    });
    scene("mask_rotated", |cx| {
        let mut p = masked(obb_line(vec![alpha(0.0, 1.0), alpha(1.0, 0.0)]));
        p.transform = Some(Transform::rotate(0.4));
        boxed(cx, 100.0, 60.0, p)
    });
    scaled("mask_linear_x2", 2.0, |cx| {
        boxed(cx, 100.0, 60.0, masked(obb_line(vec![alpha(0.0, 1.0), alpha(1.0, 0.0)])))
    });

    // A turned card that clips its children to its rounded box, one of them overflowing it.
    scene("clip_turned", |cx| {
        let mut outer = RenderProps::default();
        outer.background = solid(0.85, 0.85, 0.85);
        outer.border_radius = CornerRadius::uniform(16.0);
        outer.transform = Some(Transform::rotate(0.4));
        let parent = cx
            .create_node(Style {
                size: Size { width: length(100.0), height: length(60.0) },
                overflow: taffy::Point { x: Overflow::Clip, y: Overflow::Clip },
                ..Default::default()
            })
            .unwrap();
        cx.set_properties(parent, outer).unwrap();
        let mut inner = RenderProps::default();
        inner.background = solid(0.9, 0.2, 0.2);
        let child = cx
            .create_node(Style {
                position: Position::Absolute,
                inset: Rect { left: length(-10.0), top: length(-10.0), right: auto(), bottom: auto() },
                size: Size { width: length(60.0), height: length(40.0) },
                ..Default::default()
            })
            .unwrap();
        cx.set_properties(child, inner).unwrap();
        cx.insert_before(parent, child, None).unwrap();
        parent
    });
    // A rounded clip cut on its right by a plain clip around it.
    scene("clip_cut", |cx| {
        let outer = cx
            .create_node(Style {
                size: Size { width: length(70.0), height: length(60.0) },
                overflow: taffy::Point { x: Overflow::Clip, y: Overflow::Clip },
                ..Default::default()
            })
            .unwrap();
        let mut round = RenderProps::default();
        round.border_radius = CornerRadius::uniform(24.0);
        let middle = cx
            .create_node(Style {
                position: Position::Absolute,
                inset: Rect { left: length(30.0), top: length(0.0), right: auto(), bottom: auto() },
                size: Size { width: length(60.0), height: length(60.0) },
                overflow: taffy::Point { x: Overflow::Clip, y: Overflow::Clip },
                ..Default::default()
            })
            .unwrap();
        cx.set_properties(middle, round).unwrap();
        let mut fill = RenderProps::default();
        fill.background = solid(0.2, 0.5, 0.9);
        let child = boxed(cx, 60.0, 60.0, fill);
        cx.insert_before(middle, child, None).unwrap();
        cx.insert_before(outer, middle, None).unwrap();
        outer
    });
}
