//! Dumps ABI display-list records for a few scenes as raw little-endian f32.
use blinc_abi::context::{LayoutContext, Node, Style};
use blinc_abi::scene::blinc_core::{
    Affine2D, Brush, Color, CornerRadius, Gradient, GradientSpace, GradientSpread, GradientStop,
    MaskImage, Point, Shadow, Transform,
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
fn text_scene(name: &str, scale: f32, content: &str, size: f32) {
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
    cx.insert_before(root, text, None).unwrap();
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
    text_scene("text_hello_x1", 1.0, "Hello", 20.0);
    text_scene("text_hello_x2", 2.0, "Hello", 20.0);
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
}
