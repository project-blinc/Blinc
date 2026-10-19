//! The display list decoder, on records `blinc_abi` produced.
//!
//! Each fixture is the records of a small scene at `DISPLAY_LIST_VERSION` 1:
//! a 200 by 120 root with 20px of padding holding one box of 100 by 60.
//! `fixtures/display_list/generate.rs` writes them; it builds against
//! `blinc_abi` outside this workspace, so the fixtures are checked in.
//!
//! These tests need no GPU.

use blinc_gpu::display_list::plan::{Item, plan};
use blinc_gpu::display_list::{
    DISPLAY_LIST_VERSION, DecodeError, Kind, RECORD_FLOATS, Record, Unsupported, decode, records,
    row,
};
use blinc_gpu::primitives::{ClipType, FillType, GpuPrimitive, PrimitiveType};

macro_rules! fixture {
    ($name:literal) => {
        floats(include_bytes!(concat!(
            "fixtures/display_list/",
            $name,
            ".f32"
        )))
    };
}

fn floats(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect()
}

fn first(list: &[f32], scale: f32) -> GpuPrimitive {
    decode(
        list,
        list.len() / RECORD_FLOATS,
        DISPLAY_LIST_VERSION,
        scale,
    )
    .unwrap()
    .primitives
    .remove(0)
}

fn near(a: &[f32], b: &[f32]) {
    assert_eq!(a.len(), b.len(), "{a:?} vs {b:?}");
    for (x, y) in a.iter().zip(b) {
        assert!((x - y).abs() < 1e-3, "{a:?} vs {b:?}");
    }
}

fn set_row(list: &mut [f32], record: usize, row: usize, value: [f32; 4]) {
    let at = record * RECORD_FLOATS + row * 4;
    list[at..at + 4].copy_from_slice(&value);
}

#[test]
fn a_list_of_another_version_is_refused() {
    let list = fixture!("box_solid");
    assert_eq!(
        records(&list, 1, DISPLAY_LIST_VERSION + 1).err(),
        Some(DecodeError::Version {
            found: DISPLAY_LIST_VERSION + 1
        })
    );
}

#[test]
fn a_list_shorter_than_its_count_is_refused() {
    let list = fixture!("box_solid");
    assert_eq!(
        records(&list, 2, DISPLAY_LIST_VERSION).err(),
        Some(DecodeError::Short {
            floats: RECORD_FLOATS,
            needed: 2 * RECORD_FLOATS
        })
    );
}

#[test]
fn a_solid_box_keeps_its_geometry() {
    let p = first(&fixture!("box_solid"), 1.0);
    near(&p.bounds, &[20.0, 20.0, 100.0, 60.0]);
    near(&p.corner_radius, &[8.0; 4]);
    near(&p.color, &[0.2, 0.4, 0.9, 1.0]);
    near(&p.local_affine, &[1.0, 0.0, 0.0, 1.0]);
    near(&p.clip_bounds, &[-10000.0, -10000.0, 100000.0, 100000.0]);
    assert_eq!(
        p.type_info,
        [PrimitiveType::Rect as u32, FillType::Solid as u32, 0, 0]
    );
}

#[test]
fn the_display_scale_takes_every_length_to_device_pixels() {
    // The record does not depend on the scale the scene was painted at.
    let list = fixture!("box_solid_x2");
    let p = first(&list, 2.0);
    near(&p.bounds, &[40.0, 40.0, 200.0, 120.0]);
    near(&p.corner_radius, &[16.0; 4]);
    near(&p.border, &[4.0, 0.0, 0.0, 0.0]);
}

#[test]
fn a_uniform_border_takes_the_uniform_form() {
    let p = first(&fixture!("box_border"), 1.0);
    near(&p.border, &[3.0, 0.0, 0.0, 0.0]);
    near(&p.border_color, &[0.9, 0.1, 0.1, 1.0]);
}

#[test]
fn sides_of_different_colours_each_keep_theirs() {
    let p = first(&fixture!("box_sides"), 1.0);
    near(&p.border, &[4.0, 2.0, 6.0, 3.0]);
    near(&p.side_color_top, &[0.9, 0.1, 0.1, 1.0]);
    near(&p.side_color_right, &[0.1, 0.7, 0.1, 1.0]);
    near(&p.side_color_bottom, &[0.1, 0.1, 0.9, 1.0]);
    near(&p.side_color_left, &[0.9, 0.7, 0.1, 1.0]);
}

#[test]
fn sides_of_one_colour_keep_their_own_widths() {
    let mut list = fixture!("box_sides");
    for side in 0..4 {
        set_row(&mut list, 0, row::SIDE_COLORS + side, [0.9, 0.1, 0.1, 1.0]);
    }
    let p = first(&list, 1.0);
    near(&p.border, &[4.0, 2.0, 6.0, 3.0]);
    // One colour is carried by `border_color` alone.
    near(&p.border_color, &[0.9, 0.1, 0.1, 1.0]);
    for side in [
        p.side_color_top,
        p.side_color_right,
        p.side_color_bottom,
        p.side_color_left,
    ] {
        near(&side, &[0.0; 4]);
    }
}

#[test]
fn a_shadow_is_its_own_primitive_before_the_box() {
    let list = fixture!("box_shadow");
    let out = decode(&list, 2, DISPLAY_LIST_VERSION, 1.0).unwrap();
    assert!(out.unsupported.is_empty());
    let (shadow, body) = (&out.primitives[0], &out.primitives[1]);
    assert_eq!(shadow.type_info[0], PrimitiveType::Shadow as u32);
    near(&shadow.shadow, &[4.0, 6.0, 12.0, 2.0]);
    near(&shadow.shadow_color, &[0.0, 0.0, 0.0, 0.4]);
    near(&shadow.corner_radius, &[10.0; 4]);
    assert_eq!(body.type_info[0], PrimitiveType::Rect as u32);
    near(&body.color, &[1.0, 1.0, 1.0, 1.0]);
}

#[test]
fn a_shadow_scales_with_the_display() {
    let list = fixture!("box_shadow");
    let out = decode(&list, 2, DISPLAY_LIST_VERSION, 2.0).unwrap();
    near(&out.primitives[0].shadow, &[8.0, 12.0, 24.0, 4.0]);
}

#[test]
fn a_turned_box_keeps_its_centre_and_size() {
    let p = first(&fixture!("box_rotated"), 1.0);
    near(&p.bounds, &[20.0, 20.0, 100.0, 60.0]);
    let (sin, cos) = (0.5f32.sin(), 0.5f32.cos());
    near(&p.local_affine, &[cos, sin, -sin, cos]);
    near(&p.rotation, &[sin, cos, 0.0, 1.0]);
}

#[test]
fn a_stretched_box_takes_the_uniform_scale_and_the_rest_in_the_affine() {
    let p = first(&fixture!("box_scaled"), 1.0);
    let k = (1.5f32 * 0.75).sqrt();
    // The centre stays at (70, 50).
    near(
        &[
            p.bounds[0] + p.bounds[2] / 2.0,
            p.bounds[1] + p.bounds[3] / 2.0,
        ],
        &[70.0, 50.0],
    );
    near(&[p.bounds[2], p.bounds[3]], &[100.0 * k, 60.0 * k]);
    near(&p.local_affine, &[1.5 / k, 0.0, 0.0, 0.75 / k]);
}

#[test]
fn a_linear_gradient_is_placed_from_the_boxs_corner() {
    let list = fixture!("box_gradient");
    let p = first(&list, 1.0);
    assert_eq!(p.type_info[1], FillType::LinearGradient as u32);
    near(&p.gradient_params, &[20.0, 20.0, 120.0, 20.0]);
    near(&p.color, &[1.0, 0.2, 0.2, 1.0]);
    near(&p.color2, &[0.2, 0.2, 1.0, 1.0]);
    let p = first(&list, 2.0);
    near(&p.gradient_params, &[40.0, 40.0, 240.0, 40.0]);
}

#[test]
fn a_radial_gradient_is_placed_from_the_boxs_corner() {
    let p = first(&fixture!("box_radial"), 1.0);
    assert_eq!(p.type_info[1], FillType::RadialGradient as u32);
    near(&p.gradient_params, &[70.0, 50.0, 40.0, 0.0]);
}

#[test]
fn an_overflow_clip_reaches_the_children_it_cuts() {
    let list = fixture!("clip_child");
    let out = decode(&list, 2, DISPLAY_LIST_VERSION, 1.0).unwrap();
    let (parent, child) = (&out.primitives[0], &out.primitives[1]);
    assert_eq!(parent.type_info[2], ClipType::None as u32);
    assert_eq!(child.type_info[2], ClipType::Rect as u32);
    near(&child.clip_bounds, &[20.0, 20.0, 100.0, 60.0]);
    near(&child.clip_radius, &[16.0; 4]);
    near(&child.clip_corner_shape, &[1.0; 4]);
    // The child's own box is not changed by the clip.
    near(&child.bounds, &[10.0, 10.0, 60.0, 40.0]);
}

#[test]
fn what_a_primitive_cannot_carry_is_named() {
    let base = fixture!("box_solid");
    let cases: [(&str, usize, [f32; 4], Unsupported); 6] = [
        (
            "backdrop",
            row::TYPES,
            [42.0, 0.0, 0.0, 1.0],
            Unsupported::Kind(42.0),
        ),
        (
            "canvas",
            row::TYPES,
            [33.0, 0.0, 0.0, 1.0],
            Unsupported::Kind(33.0),
        ),
        (
            "fill of another kind",
            row::TYPES,
            [0.0, 4.0, 0.0, 1.0],
            Unsupported::Gradient,
        ),
        (
            "clip path",
            row::SHAPE + 1,
            [0.0, 0.0, 1.0, 0.0],
            Unsupported::ClipShape,
        ),
        (
            "notch",
            row::NOTCH + 1,
            [0.0, 8.0, 4.0, 0.0],
            Unsupported::Notch,
        ),
        ("no area", row::AFFINE, [0.0; 4], Unsupported::Degenerate),
    ];
    for (name, at, value, why) in cases {
        let mut list = base.clone();
        set_row(&mut list, 0, at, value);
        let record = Record::new(list[..RECORD_FLOATS].try_into().unwrap());
        assert_eq!(record.unsupported(), Some(why), "{name}");
        assert_eq!(record.to_primitive(1.0).err(), Some(why), "{name}");
    }
}

#[test]
fn a_list_reports_the_records_it_left_out_by_index() {
    let one = fixture!("box_solid");
    let mut canvas = one.clone();
    set_row(&mut canvas, 0, row::TYPES, [33.0, 0.0, 0.0, 1.0]);
    let list: Vec<f32> = [one.clone(), canvas, one].concat();
    let out = decode(&list, 3, DISPLAY_LIST_VERSION, 1.0).unwrap();
    assert_eq!(out.primitives.len(), 2);
    assert_eq!(out.unsupported, vec![(1, Unsupported::Kind(33.0))]);
}

#[test]
fn kinds_are_named() {
    let list = fixture!("box_shadow");
    let kinds: Vec<Kind> = records(&list, 2, DISPLAY_LIST_VERSION)
        .unwrap()
        .map(|r| r.kind())
        .collect();
    assert_eq!(kinds, [Kind::Shadow, Kind::Box]);
}

#[test]
fn a_glyph_is_a_quad_over_its_atlas_rect() {
    let list = fixture!("text_hello_x1");
    let out = decode(&list, 5, DISPLAY_LIST_VERSION, 1.0).unwrap();
    assert!(out.unsupported.is_empty());
    let h = &out.primitives[0];
    assert_eq!(
        h.type_info,
        [PrimitiveType::Text as u32, 0, ClipType::None as u32, 0]
    );
    near(&h.bounds, &[20.0, 25.0, 14.0, 15.0]);
    near(&h.gradient_params, &[0.0, 0.0, 14.0, 15.0]);
    near(&h.color, &[0.0, 0.0, 0.0, 1.0]);
    near(&out.primitives[1].gradient_params, &[16.0, 0.0, 28.0, 12.0]);
}

#[test]
fn the_display_scale_moves_the_quad_and_not_the_atlas_rect() {
    // At scale 2 the records are laid out in logical pixels and the glyphs
    // rasterized twice as large, so a quad and its atlas rect agree in device pixels.
    let list = fixture!("text_hello_x2");
    let out = decode(&list, 5, DISPLAY_LIST_VERSION, 2.0).unwrap();
    let h = &out.primitives[0];
    near(&h.bounds, &[41.0, 50.0, 26.0, 29.0]);
    near(&h.gradient_params, &[0.0, 0.0, 26.0, 29.0]);
}

#[test]
fn a_colour_glyph_selects_the_colour_atlas() {
    let mut list = fixture!("text_hello_x1");
    set_row(&mut list, 0, row::TYPES, [7.0, 1.0, 0.0, 1.0]);
    assert_eq!(first(&list, 1.0).type_info[1], 1);
}

#[test]
fn a_colour_glyphs_shape_is_not_a_glyph() {
    let mut list = fixture!("text_hello_x1");
    set_row(&mut list, 0, row::TYPES, [7.0, 2.0, 0.0, 1.0]);
    let record = Record::new(list[..RECORD_FLOATS].try_into().unwrap());
    assert_eq!(record.unsupported(), Some(Unsupported::ColorGlyphShape));
}

fn records_of(parts: &[Vec<f32>]) -> (Vec<f32>, usize) {
    let list: Vec<f32> = parts.concat();
    let count = list.len() / RECORD_FLOATS;
    (list, count)
}

#[test]
fn an_image_sits_between_the_primitive_runs_around_it() {
    let (list, count) = records_of(&[
        fixture!("box_solid"),
        fixture!("image_plain"),
        fixture!("text_hello_x1"),
    ]);
    let planned = plan(&list, count, DISPLAY_LIST_VERSION, 1.0).unwrap();
    assert!(planned.unsupported.is_empty());
    let shape: Vec<String> = planned
        .items
        .iter()
        .map(|i| match i {
            Item::Primitives(run) => format!("{} primitives", run.primitives.len()),
            Item::Image(d) => format!("image {}", d.slot),
            Item::Layer(_) => "layer".to_string(),
        })
        .collect();
    assert_eq!(shape, ["1 primitives", "image 7", "5 primitives"]);
}

#[test]
fn an_image_is_stretched_over_its_box_at_the_display_scale() {
    let list = fixture!("image_plain");
    for (scale, dst) in [
        (1.0, [20.0, 20.0, 40.0, 30.0]),
        (2.0, [40.0, 40.0, 80.0, 60.0]),
    ] {
        let planned = plan(&list, 1, DISPLAY_LIST_VERSION, scale).unwrap();
        let [Item::Image(draw)] = &planned.items[..] else {
            panic!("one image");
        };
        near(&draw.instance.dst_rect, &dst);
        near(&draw.instance.src_uv, &[0.0, 0.0, 1.0, 1.0]);
        near(&draw.instance.tint, &[1.0; 4]);
        near(&draw.instance.transform, &[1.0, 0.0, 0.0, 1.0]);
        assert_eq!(draw.instance.params[1], 1.0);
    }
}

#[test]
fn an_images_opacity_and_clip_reach_its_instance() {
    let faded = plan(&fixture!("image_faded"), 1, DISPLAY_LIST_VERSION, 1.0).unwrap();
    let [Item::Image(draw)] = &faded.items[..] else {
        panic!("one image");
    };
    assert_eq!(draw.instance.params[1], 0.5);

    let clipped = plan(&fixture!("image_clipped"), 1, DISPLAY_LIST_VERSION, 2.0).unwrap();
    let [Item::Image(draw)] = &clipped.items[..] else {
        panic!("one image");
    };
    near(&draw.instance.clip_bounds, &[40.0, 40.0, 200.0, 120.0]);
    near(&draw.instance.clip_radius, &[32.0; 4]);
}

#[test]
fn an_image_is_not_a_primitive() {
    let list = fixture!("image_plain");
    let record = Record::new(list[..RECORD_FLOATS].try_into().unwrap());
    assert_eq!(record.kind(), Kind::Image);
    assert_eq!(
        record.to_primitive(1.0).err(),
        Some(Unsupported::Kind(32.0))
    );
}

#[test]
fn a_mapped_image_and_an_oddly_rounded_clip_are_named() {
    let mut list = fixture!("image_plain");
    set_row(&mut list, 0, row::TYPES, [32.0, 3.0, 0.0, 1.0]);
    let planned = plan(&list, 1, DISPLAY_LIST_VERSION, 1.0).unwrap();
    assert!(planned.items.is_empty());
    assert_eq!(planned.unsupported, vec![(0, Unsupported::ImageMapping)]);

    let mut list = fixture!("image_clipped");
    set_row(&mut list, 0, row::TYPES, [32.0, 0.0, 1.0, 2.0]);
    let planned = plan(&list, 1, DISPLAY_LIST_VERSION, 1.0).unwrap();
    assert_eq!(planned.unsupported, vec![(0, Unsupported::ClipCorner)]);
}

fn layer_record(list: &[f32], index: usize) -> Vec<f32> {
    list[index * RECORD_FLOATS..(index + 1) * RECORD_FLOATS].to_vec()
}

#[test]
fn a_group_with_opacity_is_a_layer_of_its_items() {
    let list = fixture!("layer_opacity");
    let planned = plan(&list, 4, DISPLAY_LIST_VERSION, 1.0).unwrap();
    assert!(planned.unsupported.is_empty());
    let [Item::Layer(layer)] = &planned.items[..] else {
        panic!("one layer");
    };
    assert_eq!(layer.fx.opacity, 0.5);
    assert!(matches!(&layer.items[..], [Item::Primitives(run)] if run.primitives.len() == 2));
}

#[test]
fn a_layer_inside_a_layer_nests() {
    let inner = fixture!("layer_opacity");
    let list = [
        layer_record(&inner, 0),
        inner.clone(),
        layer_record(&inner, 3),
    ]
    .concat();
    let planned = plan(&list, 6, DISPLAY_LIST_VERSION, 1.0).unwrap();
    assert!(planned.unsupported.is_empty());
    let [Item::Layer(outer)] = &planned.items[..] else {
        panic!("one layer");
    };
    assert!(matches!(&outer.items[..], [Item::Layer(inner)] if inner.fx.opacity == 0.5));
}

fn the_layer_of(list: &[f32], count: usize) -> blinc_gpu::display_list::effects::LayerFx {
    let planned = plan(list, count, DISPLAY_LIST_VERSION, 1.0).unwrap();
    assert!(planned.unsupported.is_empty(), "{:?}", planned.unsupported);
    let [Item::Layer(layer)] = &planned.items[..] else {
        panic!("one layer");
    };
    layer.fx
}

#[test]
fn a_layers_blur_colour_filter_and_shadow_are_read() {
    let blur = the_layer_of(&fixture!("layer_blur"), 3);
    assert_eq!(blur.blur, 4.0);
    assert_eq!(blur.opacity, 1.0);
    assert_eq!(blur.shadow, None);

    let grey = the_layer_of(&fixture!("layer_grayscale"), 3);
    assert_eq!(grey.blur, 0.0);
    near(&grey.matrix[0], &[0.2126, 0.7152, 0.0722, 0.0]);
    near(&grey.matrix[2], &[0.2126, 0.7152, 0.0722, 0.0]);

    let shadow = the_layer_of(&fixture!("layer_shadow"), 3).shadow.unwrap();
    near(&shadow.offset, &[6.0, 5.0]);
    assert_eq!(shadow.deviation, 4.0);
    near(&shadow.color, &[0.0, 0.0, 0.0, 0.6]);
}

#[test]
fn a_layer_with_no_opening_or_no_end_is_named_and_its_content_kept() {
    let list = fixture!("layer_opacity");
    // Its end alone.
    let planned = plan(&layer_record(&list, 3), 1, DISPLAY_LIST_VERSION, 1.0).unwrap();
    assert_eq!(planned.unsupported, vec![(0, Unsupported::Kind(41.0))]);
    // Its beginning and content, never closed.
    let planned = plan(&list[..3 * RECORD_FLOATS], 3, DISPLAY_LIST_VERSION, 1.0).unwrap();
    assert_eq!(planned.unsupported, vec![(0, Unsupported::Kind(40.0))]);
    assert!(matches!(&planned.items[..], [Item::Primitives(run)] if run.primitives.len() == 2));
}

fn first_of(list: &[f32]) -> (GpuPrimitive, Vec<[f32; 4]>) {
    let out = decode(list, 1, DISPLAY_LIST_VERSION, 1.0).unwrap();
    assert!(out.unsupported.is_empty(), "{:?}", out.unsupported);
    (out.primitives[0], out.aux)
}

/// The offsets of a table's stops, and its header.
fn stops_of(aux: &[[f32; 4]]) -> (f32, f32, Vec<f32>) {
    let count = aux[0][0] as usize;
    assert_eq!(
        aux.len(),
        2 + 2 * count,
        "a header, an average, two rows a stop"
    );
    (
        aux[0][0],
        aux[0][1],
        (0..count).map(|i| aux[3 + 2 * i][0]).collect(),
    )
}

#[test]
fn two_stops_from_nought_to_one_need_no_table() {
    let (p, aux) = first_of(&fixture!("box_gradient"));
    assert_eq!(p.gradient_table, [0.0; 4]);
    assert!(aux.is_empty());
}

#[test]
fn a_middle_stop_is_a_table_of_three() {
    let (p, aux) = first_of(&fixture!("grad_three"));
    assert_eq!(
        p.gradient_table[0], 1.0,
        "the table is at offset 0 in aux_data"
    );
    assert_eq!(stops_of(&aux), (3.0, 0.0, vec![0.0, 0.5, 1.0]));
    near(&aux[2], &[1.0, 0.1, 0.1, 1.0]);
    near(&aux[4], &[1.0, 0.9, 0.1, 1.0]);
    near(&aux[6], &[0.1, 0.1, 1.0, 1.0]);
}

#[test]
fn two_stops_between_other_offsets_are_a_table_of_two() {
    let (p, aux) = first_of(&fixture!("grad_offsets"));
    assert_eq!(p.gradient_table[0], 1.0);
    assert_eq!(stops_of(&aux), (2.0, 0.0, vec![0.25, 0.75]));
}

#[test]
fn a_table_the_producer_made_is_copied_with_its_spread() {
    let (p, aux) = first_of(&fixture!("grad_five"));
    assert_eq!(p.gradient_table[0], 1.0);
    assert_eq!(stops_of(&aux), (5.0, 0.0, vec![0.0, 0.2, 0.5, 0.8, 1.0]));
    let (_, repeat, _) = stops_of(&first_of(&fixture!("grad_repeat")).1);
    let (_, reflect, _) = stops_of(&first_of(&fixture!("grad_reflect_radial")).1);
    assert_eq!((repeat, reflect), (1.0, 2.0));
}

#[test]
fn a_conic_gradient_keeps_its_angle_when_the_display_scales() {
    let list = fixture!("grad_conic_stops");
    let out = decode(&list, 1, DISPLAY_LIST_VERSION, 2.0).unwrap();
    let p = out.primitives[0];
    assert_eq!(p.type_info[1], FillType::ConicGradient as u32);
    // The centre is placed from the box's corner and scaled, the angle is not.
    near(&p.gradient_params, &[40.0 + 100.0, 40.0 + 60.0, 0.5, 0.0]);
}

#[test]
fn a_table_that_is_not_in_the_list_is_named() {
    let list = fixture!("grad_five");
    let planned = plan(&list[..RECORD_FLOATS], 1, DISPLAY_LIST_VERSION, 1.0).unwrap();
    assert_eq!(planned.unsupported, vec![(0, Unsupported::Gradient)]);
    // And a single-record decode, with nowhere to keep one, says so.
    let record = Record::new(list[..RECORD_FLOATS].try_into().unwrap());
    assert_eq!(
        record.to_primitive(1.0).err(),
        Some(Unsupported::GradientTable)
    );
}

#[test]
fn tables_in_one_run_follow_each_other() {
    // Two records over the one table in the buffer's tail, moved to follow them.
    let five = fixture!("grad_five");
    let (record, tail) = (&five[..RECORD_FLOATS], &five[RECORD_FLOATS..]);
    let mut list = [record, record, tail].concat();
    // The tail now begins 56 vec4s in, so the marker is two more.
    for r in 0..2 {
        list[r * RECORD_FLOATS + row::GRADIENT_OFFSETS * 4 + 3] = 2.0 + 56.0;
    }
    let planned = plan(&list, 2, DISPLAY_LIST_VERSION, 1.0).unwrap();
    assert!(planned.unsupported.is_empty(), "{:?}", planned.unsupported);
    let [Item::Primitives(run)] = &planned.items[..] else {
        panic!("one run");
    };
    assert_eq!(run.aux.len(), 24, "two tables of twelve rows");
    assert_eq!(run.primitives[0].gradient_table[0], 1.0);
    assert_eq!(run.primitives[1].gradient_table[0], 13.0);
    assert_eq!(run.aux[..12], run.aux[12..]);
}

fn mask_of(list: &[f32]) -> blinc_gpu::display_list::effects::LayerMask {
    the_layer_of(list, 3).mask.expect("a mask")
}

#[test]
fn a_layers_mask_is_read_from_its_rows() {
    let m = mask_of(&fixture!("mask_linear"));
    assert_eq!(m.kind, 1.0);
    near(&m.geometry, &[0.0, 0.0, 1.0, 0.0]);
    near(&m.frame, &[20.0, 20.0, 100.0, 60.0]);
    near(&m.matrix, &[1.0, 0.0, 0.0, 1.0]);
    near(&m.translate, &[0.0, 0.0, 1.0]);
    assert_eq!((m.spread, m.count), (0.0, 2));
    near(&m.stops[0], &[0.0, 1.0]);
    near(&m.stops[1], &[1.0, 0.0]);

    let radial = mask_of(&fixture!("mask_radial"));
    assert_eq!(radial.kind, 2.0);
    near(&radial.geometry, &[0.5, 0.5, 0.5, 0.0]);
}

#[test]
fn a_mask_with_a_table_has_its_alphas_and_offsets_from_it() {
    let m = mask_of(&fixture!("mask_stops"));
    assert_eq!(m.count, 5);
    let want = [[0.0, 1.0], [0.25, 0.2], [0.5, 0.8], [0.75, 0.0], [1.0, 1.0]];
    for (got, want) in m.stops.iter().zip(want) {
        near(got, &want);
    }
    let conic = mask_of(&fixture!("mask_conic"));
    assert_eq!((conic.kind, conic.count), (3.0, 2));
    near(&conic.stops[0], &[0.0, 0.1]);
}

#[test]
fn a_masks_transform_and_display_scale_are_carried() {
    let turned = mask_of(&fixture!("mask_rotated"));
    near(&turned.matrix, &[0.921061, 0.389418, -0.389418, 0.921061]);
    near(&turned.translate, &[24.9967, -23.3123, 1.0]);
    assert_eq!(mask_of(&fixture!("mask_linear_x2")).translate[2], 2.0);
}

#[test]
fn a_mask_with_more_stops_than_there_is_room_for_is_drawn_flat_and_named() {
    // The records of a masked layer, then a table of seventeen stops after them.
    let mut list = fixture!("mask_stops")[..3 * RECORD_FLOATS].to_vec();
    let mut table = vec![17.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
    for i in 0..17 {
        table.extend_from_slice(&[0.0, 0.0, 0.0, 1.0, i as f32 / 16.0, 0.0, 0.0, 0.0]);
    }
    list.extend(table);
    // It begins after the three records, 84 vec4s in.
    list[2 * RECORD_FLOATS + row::GRADIENT_OFFSETS * 4 + 3] = 2.0 + 84.0;
    let planned = plan(&list, 3, DISPLAY_LIST_VERSION, 1.0).unwrap();
    assert_eq!(planned.unsupported, vec![(2, Unsupported::LayerEffect)]);
    assert!(matches!(&planned.items[..], [Item::Primitives(run)] if run.primitives.len() == 1));
}

#[test]
fn a_turned_parents_clip_reaches_its_child_in_the_childs_own_frame() {
    let list = fixture!("clip_turned");
    for (scale, k) in [(1.0, 1.0), (2.0, 2.0)] {
        let out = decode(&list, 2, DISPLAY_LIST_VERSION, scale).unwrap();
        assert!(out.unsupported.is_empty(), "{:?}", out.unsupported);
        let (parent, child) = (&out.primitives[0], &out.primitives[1]);
        assert_eq!(
            parent.local_clip_bounds, [0.0; 4],
            "the card itself is not clipped"
        );
        // The card's box, in the child's coordinates: the child begins 10 inside it.
        near(
            &child.local_clip_bounds,
            &[10.0 * k, 10.0 * k, 100.0 * k, 60.0 * k],
        );
        near(&child.local_clip_radius, &[16.0 * k; 4]);
        near(&child.clip_corner_shape, &[1.0; 4]);
        assert_eq!(
            child.type_info[2],
            ClipType::None as u32,
            "no clip on screen"
        );
    }
}

#[test]
fn a_rounded_clip_cut_by_a_plain_one_has_both() {
    let out = decode(&fixture!("clip_cut"), 1, DISPLAY_LIST_VERSION, 1.0).unwrap();
    let p = &out.primitives[0];
    assert_eq!(p.type_info[2], ClipType::Rect as u32);
    near(&p.clip_bounds, &[20.0, 20.0, 70.0, 60.0]);
    near(&p.local_clip_bounds, &[0.0, 0.0, 60.0, 60.0]);
    near(&p.local_clip_radius, &[24.0; 4]);
}

#[test]
fn an_image_under_a_clip_in_its_own_frame_is_named() {
    let mut list = fixture!("image_plain");
    set_row(&mut list, 0, row::TYPES, [32.0, 0.0, 2.0, 1.0]);
    let planned = plan(&list, 1, DISPLAY_LIST_VERSION, 1.0).unwrap();
    assert!(planned.items.is_empty());
    assert_eq!(planned.unsupported, vec![(0, Unsupported::LocalClip)]);
}

#[test]
fn fading_clips_keep_their_own_screen_rectangle_and_distances() {
    for scale in [1.0, 2.0] {
        let p = first(&fixture!("fade_cut"), scale);
        near(&p.clip_bounds, &[20.0, 20.0, 70.0, 60.0].map(|v| v * scale));
        near(
            &p.clip_fade_bounds,
            &[10.0, 10.0, 100.0, 80.0].map(|v| v * scale),
        );
        near(&p.clip_fade, &[20.0, 32.0, 24.0, 28.0].map(|v| v * scale));
        let turned = first(&fixture!("fade_turned"), scale);
        assert_eq!(turned.type_info[2], ClipType::None as u32);
        assert!(turned.local_clip_bounds[2] > 0.0);
        assert!(turned.clip_fade_bounds[2] > 100.0 * scale);
        let shadow = first(&fixture!("fade_shadow"), scale);
        assert_eq!(shadow.type_info[0], PrimitiveType::Shadow as u32);
        near(&shadow.clip_fade, &p.clip_fade);
    }
    assert_eq!(
        first(&fixture!("box_solid"), 1.0).clip_fade_bounds,
        [0.0; 4]
    );
}

#[test]
fn glyphs_take_the_fade_in_display_pixels_not_atlas_pixels() {
    for (list, scale) in [
        (fixture!("fade_text_x1"), 1.0),
        (fixture!("fade_text_x2"), 2.0),
    ] {
        let decoded = decode(&list, 5, DISPLAY_LIST_VERSION, scale).unwrap();
        assert!(decoded.unsupported.is_empty());
        for p in decoded.primitives {
            assert_eq!(p.type_info[0], PrimitiveType::Text as u32);
            near(
                &p.clip_fade_bounds,
                &[20.0, 20.0, 100.0, 60.0].map(|v| v * scale),
            );
            near(&p.clip_fade, &[20.0, 32.0, 24.0, 28.0].map(|v| v * scale));
        }
    }
}

#[test]
fn images_under_fading_clips_are_still_named() {
    let list = fixture!("fade_image");
    let planned = plan(&list, 2, DISPLAY_LIST_VERSION, 1.0).unwrap();
    assert_eq!(planned.unsupported, vec![(1, Unsupported::ClipFade)]);
    assert!(matches!(&planned.items[..], [Item::Primitives(run)] if run.primitives.len() == 1));
}

#[test]
fn a_fading_clip_inside_a_layer_keeps_its_screen_coordinates_until_drawing() {
    let list = fixture!("fade_layer");
    let planned = plan(&list, 3, DISPLAY_LIST_VERSION, 1.0).unwrap();
    assert!(planned.unsupported.is_empty());
    let [Item::Layer(layer)] = &planned.items[..] else {
        panic!("a layer")
    };
    let [Item::Primitives(run)] = &layer.items[..] else {
        panic!("a primitive run")
    };
    near(
        &run.primitives[0].clip_fade_bounds,
        &[10.0, 10.0, 100.0, 80.0],
    );
}
