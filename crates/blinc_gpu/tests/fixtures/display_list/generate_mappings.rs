//! Encoder fixtures; run generate_mappings.py to expose mapping state in a scratch copy.
use blinc_abi::context::{LayoutContext, Node, Style};
use blinc_abi::scene::{PaintOptions, RenderProps, SceneEncoder};
use blinc_abi::scene::blinc_core::{Affine2D, Brush, CornerRadius, ImageBrush, ImageFit, Transform, OverflowFade, ClipLength, ClipPath};
use taffy::prelude::*;
use taffy::Overflow;

fn node(cx: &mut LayoutContext, w: f32, h: f32, props: RenderProps) -> Node {
    let node = cx.create_node(Style { size: Size { width: length(w), height: length(h) }, ..Default::default() }).unwrap();
    cx.set_properties(node, props).unwrap(); node
}
fn emit(name: &str, ratio: f32) {
    let mut cx = LayoutContext::new();
    let root = node(&mut cx, 200.0, 120.0, RenderProps::default());
    let mut props = RenderProps::default();
    props.opacity = 0.7;
    props.text_color = Some([0.8,0.25,0.6,0.55]);
    let background = name.starts_with("mapping_bg");
    let tile = name.contains("tile") || name.contains("repeat");
    let fit = if name.contains("cover") { 0 } else if name.contains("contain") { 1 } else if name.contains("fill") { 2 } else if tile { 3 } else if name.contains("down") { 5 } else { 4 };
    if background {
        props.background = Some(Brush::Image(ImageBrush::new("probe".to_string()).fit(match fit { 0 => ImageFit::Cover, 1 => ImageFit::Contain, 2 => ImageFit::Fill, _ => ImageFit::Tile }).opacity(0.65)));
    }
    let child = node(&mut cx, 110.0, 65.0, props);
    cx.set_style(child, Style {
        position: Position::Absolute,
        inset: Rect { left: length(35.0), top: length(25.0), right: auto(), bottom: auto() },
        size: Size { width: length(110.0), height: length(65.0) },
        ..Default::default()
    }).unwrap();
    if !background { cx.set_resource(child, Some(7), false).unwrap(); }
    else { cx.set_image_source("probe".into(),fit.min(3),Some(7)).unwrap(); }
    let position = if tile { [0.25,0.7,-9.375,3.625] } else { [0.125,0.875,-0.375,1.625] };
    if background {
        let repeat = if name.contains("repeat_x") { 1 } else if name.contains("repeat_y") { 2 } else if name.contains("repeat_both") { 3 } else { 0 };
        cx.probe_background(child,position,repeat);
    } else if !name.contains("legacy") && !name.contains("mask") { cx.probe_mapping(child,fit,position); }
    if name.contains("affine") {
        let mut props=cx.properties(child).unwrap();
        props.transform=Some(Transform::Affine2D(Affine2D { elements: [1.1,0.25,-0.2,0.85,0.3,0.4] }));
        cx.set_properties(child,props).unwrap();
    }
    let mut content=child;
    if name.contains("clip") || name.contains("layer") {
        let mut props=RenderProps::default();
        props.border_radius=CornerRadius::uniform(16.0);
        props.overflow_fade=OverflowFade::new(8.0,12.0,10.0,9.0);
        props.clip_path=Some(ClipPath::Ellipse { rx: Some(ClipLength::Px(52.0)), ry: Some(ClipLength::Px(32.0)), center: (ClipLength::Px(60.0),ClipLength::Px(38.0)) });
        let parent=node(&mut cx,120.0,80.0,props);
        let mut style=cx.style(parent).unwrap();
        style.overflow=taffy::Point { x: Overflow::Clip, y: Overflow::Clip };
        style.position=Position::Absolute;
        style.inset=Rect { left: length(25.0), top: length(18.0), right: auto(), bottom: auto() };
        cx.set_style(parent,style).unwrap();
        let mut style=cx.style(child).unwrap(); style.inset.left=length(-8.0); style.inset.top=length(-5.0); cx.set_style(child,style).unwrap();
        cx.insert_before(parent,child,None).unwrap(); content=parent;
    }
    if name.contains("layer") {
        for _ in 0..2 {
            let mut props=RenderProps::default(); props.opacity=0.6;
            props.filter=Some(blinc_layout::element_style::CssFilter { brightness:0.8, ..Default::default() });
            let outer=node(&mut cx,180.0,110.0,props); cx.insert_before(outer,content,None).unwrap(); content=outer;
        }
    }
    if name == "mapping_pair" {
        let mut props=RenderProps::default(); props.opacity=0.4;
        let second=node(&mut cx,55.0,35.0,props);
        let mut style=cx.style(second).unwrap(); style.position=Position::Absolute;
        style.inset=Rect { left:length(65.0),top:length(45.0),right:auto(),bottom:auto() };
        cx.set_style(second,style).unwrap(); cx.set_resource(second,Some(7),false).unwrap();
        cx.probe_mapping(second,0,[0.9,0.1,2.25,-1.75]);
        cx.insert_before(root,content,None).unwrap(); content=second;
    }
    cx.insert_before(root,content,None).unwrap(); cx.compute(root,200.0,120.0).unwrap();
    let mut encoder=SceneEncoder::new(); let info=encoder.prepare(&cx,root,PaintOptions { scale:ratio,..Default::default() }).unwrap();
    let mut data=vec![0.0f32;info.floats]; encoder.read(&cx,&mut data).unwrap();
    std::fs::write(format!("out/{name}_x{ratio}.f32"),data.iter().flat_map(|v|v.to_le_bytes()).collect::<Vec<_>>()).unwrap();
    println!("{name}_x{ratio}: {} records",info.count);
}
fn main() {
    std::fs::create_dir_all("out").unwrap();
    for name in ["mapping_pair","mapping_tile_clip_affine","mapping_bg_repeat_x_layer","mapping_cover","mapping_contain","mapping_fill","mapping_none","mapping_down","mapping_tile","mapping_contain_affine","mapping_tile_clip","mapping_none_layer","mapping_mask","mapping_legacy","mapping_bg_cover","mapping_bg_contain","mapping_bg_repeat_x","mapping_bg_repeat_y","mapping_bg_repeat_both"] {
        for scale in [1.0,2.0] { emit(name,scale); }
    }
}
