//! Large mask tables from the unchanged encoder; run generate_masks.py.
use blinc_abi::context::{LayoutContext, Node, Style};
use blinc_abi::scene::blinc_core::{
    Affine2D, Color, Gradient, GradientSpace, GradientSpread, GradientStop, MaskImage, Point,
    Transform,
};
use blinc_abi::scene::{PaintOptions, RenderProps, SceneEncoder};
use blinc_layout::element_style::CssFilter;
use taffy::prelude::*;

fn node(cx: &mut LayoutContext, w: f32, h: f32, props: RenderProps) -> Node {
    let n = cx
        .create_node(Style {
            size: Size {
                width: length(w),
                height: length(h),
            },
            ..Default::default()
        })
        .unwrap();
    cx.set_properties(n, props).unwrap();
    n
}
fn gradient(name: &str, count: usize) -> Gradient {
    let mut stops: Vec<_> = (0..count)
        .map(|i| {
            let position = i as f32 / (count - 1) as f32;
            let offset = if name.contains("repeat") || name.contains("reflect") {
                0.17 + position * 0.43
            } else {
                position
            };
            let alpha = if i + 1 == count {
                0.95
            } else {
                ((i * 7 + 3) % 13) as f32 / 13.0
            };
            GradientStop::new(offset, Color::rgba(0.3, 0.8, 0.1, alpha))
        })
        .collect();
    if name.contains("hard") {
        for i in (1..count).step_by(2) {
            stops[i].offset = stops[i - 1].offset;
        }
    }
    let mut g = if name.contains("radial") {
        Gradient::radial_with_stops(Point::new(0.38, 0.6), 0.65, stops)
    } else if name.contains("conic") {
        Gradient::Conic {
            center: Point::new(0.4, 0.55),
            start_angle: 0.37,
            stops,
            space: GradientSpace::ObjectBoundingBox,
        }
    } else {
        Gradient::linear_with_stops(Point::new(0.1, 0.2), Point::new(0.85, 0.75), stops)
    };
    match &mut g {
        Gradient::Linear { space, spread, .. } | Gradient::Radial { space, spread, .. } => {
            *space = GradientSpace::ObjectBoundingBox;
            *spread = if name.contains("reflect") {
                GradientSpread::Reflect
            } else if name.contains("repeat") {
                GradientSpread::Repeat
            } else {
                GradientSpread::Pad
            };
        }
        _ => {}
    }
    g
}
fn emit(name: &str, count: usize, scale: f32) {
    let mut cx = LayoutContext::new();
    let root = node(&mut cx, 200.0, 120.0, RenderProps::default());
    let content = node(
        &mut cx,
        105.0,
        60.0,
        RenderProps {
            mask_image: Some(MaskImage::Gradient(gradient(name, count))),
            opacity: 0.75,
            filter: Some(CssFilter {
                brightness: 0.8,
                ..Default::default()
            }),
            transform: if name.contains("affine") {
                Some(Transform::Affine2D(Affine2D {
                    elements: [1.08, 0.19, -0.23, 0.92, 0.3, 0.4],
                }))
            } else {
                None
            },
            ..Default::default()
        },
    );
    cx.set_resource(content, Some(7), false).unwrap();
    let mut container = content;
    if name.contains("nested") {
        let parent = node(
            &mut cx,
            125.0,
            80.0,
            RenderProps {
                opacity: 0.65,
                filter: Some(CssFilter {
                    brightness: 0.9,
                    ..Default::default()
                }),
                mask_image: Some(MaskImage::Gradient(gradient("reflect", 33))),
                ..Default::default()
            },
        );
        let mut style = cx.style(parent).unwrap();
        style.padding = Rect::length(7.0);
        cx.set_style(parent, style).unwrap();
        cx.insert_before(parent, content, None).unwrap();
        container = parent;
    }
    let mut style = cx.style(container).unwrap();
    style.position = Position::Absolute;
    style.inset = Rect {
        left: length(30.0),
        top: length(24.0),
        right: auto(),
        bottom: auto(),
    };
    cx.set_style(container, style).unwrap();
    cx.insert_before(root, container, None).unwrap();
    cx.compute(root, 200.0, 120.0).unwrap();
    let mut encoder = SceneEncoder::new();
    let info = encoder
        .prepare(
            &cx,
            root,
            PaintOptions {
                scale,
                ..Default::default()
            },
        )
        .unwrap();
    let mut list = vec![0.0; info.floats];
    encoder.read(&cx, &mut list).unwrap();
    let masks: Vec<_> = list[..info.count * 112]
        .chunks_exact(112)
        .filter(|r| r[44] == 41.0)
        .map(|r| {
            let at = (r[59] - 2.0) as usize * 4;
            list[at] as usize
        })
        .collect();
    assert!(masks.contains(&count), "encoder preserved mask stop count");
    std::fs::write(
        format!("out/{name}_x{scale}.f32"),
        list.iter()
            .flat_map(|v| v.to_le_bytes())
            .collect::<Vec<_>>(),
    )
    .unwrap();
    println!("{name}_x{scale}: {} records, masks {masks:?}", info.count);
}
fn main() {
    std::fs::create_dir_all("out").unwrap();
    for (name, count) in [
        ("mask_many_16", 16),
        ("mask_many_17", 17),
        ("mask_many_33", 33),
        ("mask_many_rows", 4099),
        ("mask_many_repeat", 19),
        ("mask_many_reflect", 21),
        ("mask_many_hard", 18),
        ("mask_many_radial", 25),
        ("mask_many_conic", 27),
        ("mask_many_affine_nested", 37),
    ] {
        for scale in [1.0, 2.0] {
            emit(name, count, scale);
        }
    }
}
