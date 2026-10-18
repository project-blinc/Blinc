//! Dumps ABI display-list records for a few scenes as raw little-endian f32.
use blinc_abi::context::{LayoutContext, Node, Style};
use blinc_abi::scene::blinc_core::{
    Affine2D, Brush, Color, CornerRadius, Gradient, Point, Shadow, Transform,
};
use blinc_abi::scene::{PaintOptions, RenderProps, SceneEncoder};
use blinc_layout::element::{BorderSide, BorderSides};
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
    out.truncate(info.count * 112);
    let bytes: Vec<u8> = out.iter().flat_map(|f| f.to_le_bytes()).collect();
    std::fs::write(format!("out/{name}.f32"), bytes).unwrap();
    println!("{name}: {} records", info.count);
}

fn solid(r: f32, g: f32, b: f32) -> Option<Brush> {
    Some(Brush::Solid(Color::rgb(r, g, b)))
}

fn main() {
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
}
