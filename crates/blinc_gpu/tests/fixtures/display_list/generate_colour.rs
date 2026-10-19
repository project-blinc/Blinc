//! Producer glyph records and cropped atlases. Run through generate_colour.py.
use blinc_abi::context::{LayoutContext, Style};
use blinc_abi::scene::blinc_core::{ClipLength, ClipPath, Color, OverflowFade, Shadow, Transform};
use blinc_abi::scene::{PaintOptions, RenderProps, SceneEncoder};
use blinc_layout::element_style::CssFilter;
use blinc_layout::tree::TextMeasureContext;
use taffy::prelude::*;

fn dump(name: &str, scale: f32) {
    let mut cx = LayoutContext::new();
    let text = cx
        .create_text(
            Style::default(),
            TextMeasureContext {
                content: "A😀B🎉".into(),
                font_size: 30.0,
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
    cx.set_properties(
        text,
        RenderProps {
            text_color: Some([
                0.8,
                0.15,
                0.4,
                if name == "colour_opacity" { 0.35 } else { 1.0 },
            ]),
            ..Default::default()
        },
    )
    .unwrap();
    let parent = cx
        .create_node(Style {
            size: Size {
                width: length(124.0),
                height: length(65.0),
            },
            overflow: if name == "colour_clip" || name == "colour_transform" {
                taffy::Point {
                    x: taffy::Overflow::Clip,
                    y: taffy::Overflow::Clip,
                }
            } else {
                taffy::Point {
                    x: taffy::Overflow::Visible,
                    y: taffy::Overflow::Visible,
                }
            },
            ..Default::default()
        })
        .unwrap();
    let mut props = RenderProps::default();
    if name == "colour_clip" || name == "colour_transform" {
        props.overflow_fade = OverflowFade::new(8.0, 22.0, 16.0, 19.0);
        props.clip_path = Some(ClipPath::Ellipse {
            rx: Some(ClipLength::Px(52.0)),
            ry: Some(ClipLength::Px(24.0)),
            center: (ClipLength::Px(62.0), ClipLength::Px(28.0)),
        });
    }
    if name == "colour_transform" {
        props.transform = Some(Transform::rotate(0.19));
    }
    if name == "colour_layer" {
        props.opacity = 0.6;
        props.filter = Some(CssFilter {
            brightness: 0.8,
            ..Default::default()
        });
    }
    cx.set_properties(parent, props).unwrap();
    if name != "colour_plain" && name != "colour_opacity" {
        cx.set_text_shadow(
            parent,
            Some(vec![
                Shadow {
                    offset_x: 4.0,
                    offset_y: 6.0,
                    blur: if name == "colour_blur" { 4.0 } else { 0.0 },
                    spread: 0.0,
                    color: Color::rgba(0.15, 0.65, 0.9, 0.7),
                },
                Shadow {
                    offset_x: -5.0,
                    offset_y: 10.0,
                    blur: 0.0,
                    spread: 0.0,
                    color: Color::rgba(0.7, 0.3, 0.1, 0.45),
                },
            ]),
        )
        .unwrap();
    }
    cx.insert_before(parent, text, None).unwrap();
    let root = cx
        .create_node(Style {
            size: Size {
                width: length(200.0),
                height: length(120.0),
            },
            align_items: Some(AlignItems::FLEX_START),
            padding: Rect::length(25.0),
            ..Default::default()
        })
        .unwrap();
    cx.insert_before(root, parent, None).unwrap();
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
    let mut records = vec![0.0; info.floats];
    encoder.read(&cx, &mut records).unwrap();
    assert!(
        records[..info.count * 112]
            .chunks_exact(112)
            .any(|r| r[44] == 7.0 && r[45] == 1.0)
    );
    std::fs::write(
        format!("out/{name}_x{scale}.f32"),
        records
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect::<Vec<_>>(),
    )
    .unwrap();
    for color in [false, true] {
        let info = encoder.atlas_info(color, 0).unwrap().unwrap();
        let mut bytes = vec![0; info.bytes()];
        encoder.read_atlas(color, 0, &mut bytes).unwrap().unwrap();
        let channels = if color { 4 } else { 1 };
        let (w, h) = (info.width as usize, info.height as usize);
        assert_eq!(
            (info.x, info.y, info.update_width, info.update_height),
            (0, 0, info.width, info.height)
        );
        let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
        for y in 0..h {
            for x in 0..w {
                if bytes[(y * w + x) * channels + channels - 1] != 0 {
                    x0 = x0.min(x);
                    y0 = y0.min(y);
                    x1 = x1.max(x + 1);
                    y1 = y1.max(y + 1);
                }
            }
        }
        let mut crop = Vec::new();
        for v in [w, h, x0, y0, x1 - x0, y1 - y0] {
            crop.extend_from_slice(&(v as u32).to_le_bytes());
        }
        for y in y0..y1 {
            crop.extend_from_slice(&bytes[(y * w + x0) * channels..(y * w + x1) * channels]);
        }
        std::fs::write(
            format!(
                "out/{name}_x{scale}.{}",
                if color { "rgba" } else { "atlas" }
            ),
            crop,
        )
        .unwrap();
    }
    println!("{name}_x{scale}: {} records", info.count);
}
fn main() {
    std::fs::create_dir_all("out").unwrap();
    for name in [
        "colour_plain",
        "colour_opacity",
        "colour_shadow",
        "colour_clip",
        "colour_transform",
        "colour_layer",
        "colour_blur",
    ] {
        for scale in [1.0, 2.0] {
            dump(name, scale);
        }
    }
}
