//! Backdrop records from the ABI encoder, without a GPU.

use blinc_gpu::display_list::plan::{Item, plan};
use blinc_gpu::display_list::{
    DISPLAY_LIST_VERSION, Kind, RECORD_FLOATS, Record, Unsupported, records, row,
};

macro_rules! fixture {
    ($name:literal) => {
        include_bytes!(concat!("fixtures/display_list/", $name, ".f32"))
            .chunks_exact(4)
            .map(|v| f32::from_le_bytes(v.try_into().unwrap()))
            .collect::<Vec<_>>()
    };
}

#[test]
fn backdrop_blur_is_already_in_device_pixels_while_geometry_stays_logical() {
    for (list, scale) in [
        (fixture!("backdrop_blur_x1"), 1.0),
        (fixture!("backdrop_blur_x2"), 2.0),
    ] {
        let planned = plan(&list, 2, DISPLAY_LIST_VERSION, scale).unwrap();
        assert!(planned.unsupported.is_empty());
        let [Item::Image(_), Item::Backdrop(draw)] = &planned.items[..] else {
            panic!("image followed by backdrop")
        };
        assert_eq!(draw.rows[row::BOUNDS], [40.0, 30.0, 90.0, 48.0]);
        assert!((draw.rows[row::FILL][0] - 3.2 * scale).abs() < 1e-5);
        assert!((draw.rows[row::FILL][1] - 9.6).abs() < 1e-5);
        assert_eq!(draw.scale, scale);
        assert_eq!(draw.rows[row::FILL_END], [0.75, 0.0, 0.0, 0.025]);
        assert!(draw.aux.is_empty());
        let record = records(&list, 2, DISPLAY_LIST_VERSION)
            .unwrap()
            .nth(1)
            .unwrap();
        assert_eq!(record.kind(), Kind::Backdrop);
        assert_eq!(
            record.to_primitive(scale).err(),
            Some(Unsupported::Kind(42.0))
        );
    }
}

#[test]
fn liquid_glass_preserves_signed_bevel_border_and_colour_matrix() {
    let list = fixture!("backdrop_border_x2");
    let record = records(&list, 2, DISPLAY_LIST_VERSION)
        .unwrap()
        .nth(1)
        .unwrap();
    let draw = record.backdrop(2.0, &list).unwrap();
    assert_eq!(draw.rows[row::GRADIENT][..3], [1.0, -0.7, 1.2]);
    assert_eq!(draw.rows[row::GRADIENT_OFFSETS][0], 0.7);
    assert_eq!(draw.rows[row::GRADIENT_VIA], [0.2, 0.5, 0.8, 0.15]);
    assert_eq!(draw.rows[row::SIDE_COLORS], [0.1, 0.8, 0.6, 0.7]);
    assert!(draw.rows[row::FILL][1] > 50.0);
    let matrix = fixture!("backdrop_matrix_x1");
    let record = records(&matrix, 2, DISPLAY_LIST_VERSION)
        .unwrap()
        .nth(1)
        .unwrap();
    let draw = record.backdrop(1.0, &matrix).unwrap();
    assert_ne!(draw.rows[row::FILL_END], [1.0, 0.0, 0.0, 0.0]);
    assert!(draw.rows[row::FILL_END][1] != 0.0 && draw.rows[row::FILL_END][3] != 0.0);
}

#[test]
fn backdrop_clips_and_notches_keep_the_producers_frames() {
    let list = fixture!("backdrop_clip_x2");
    let record = records(&list, 2, DISPLAY_LIST_VERSION)
        .unwrap()
        .nth(1)
        .unwrap();
    let draw = record.backdrop(2.0, &list).unwrap();
    assert!(draw.rows[row::TYPES][2] as u32 & 2 != 0);
    assert_eq!(draw.rows[row::FADE], [12.0, 20.0, 10.0, 16.0]);
    assert_eq!(draw.aux[2], [3.0, 6.0, 0.0, 0.0]);
    assert_eq!(draw.aux[0], record.row(row::SHAPE).map(|v| v / 2.0));
    let planned = plan(&list[..2 * RECORD_FLOATS], 2, DISPLAY_LIST_VERSION, 2.0).unwrap();
    assert_eq!(planned.unsupported, vec![(1, Unsupported::ClipShape)]);
    let list = fixture!("backdrop_notch_x1");
    let record = records(&list, 2, DISPLAY_LIST_VERSION)
        .unwrap()
        .nth(1)
        .unwrap();
    let draw = record.backdrop(1.0, &list).unwrap();
    assert_eq!(draw.rows[row::NOTCH], [-12.0, -8.0, 6.0, 4.0]);
    assert_eq!(draw.rows[row::NOTCH + 1], [3.0, 28.0, 8.0, 0.0]);
}

#[test]
fn overlapping_backdrops_separate_runs_and_stay_inside_their_layers() {
    let planned = plan(&fixture!("backdrop_order_x1"), 4, DISPLAY_LIST_VERSION, 1.0).unwrap();
    assert!(planned.unsupported.is_empty());
    assert!(matches!(
        &planned.items[..],
        [
            Item::Image(_),
            Item::Backdrop(_),
            Item::Primitives(_),
            Item::Backdrop(_)
        ]
    ));
    let planned = plan(
        &fixture!("backdrop_layers_x1"),
        7,
        DISPLAY_LIST_VERSION,
        1.0,
    )
    .unwrap();
    let [Item::Image(_), Item::Layer(first)] = &planned.items[..] else {
        panic!("outer layer")
    };
    let [Item::Layer(second)] = &first.items[..] else {
        panic!("inner layer")
    };
    assert!(matches!(
        &second.items[..],
        [Item::Primitives(_), Item::Backdrop(_)]
    ));
}

#[test]
fn backdrop_readers_report_wrong_kinds_and_degenerate_frames() {
    let list = fixture!("backdrop_blur_x1");
    let image = Record::new(list[..RECORD_FLOATS].try_into().unwrap());
    assert_eq!(
        image.backdrop(1.0, &list).err(),
        Some(Unsupported::Kind(32.0))
    );
    let mut r: [f32; RECORD_FLOATS] = list[RECORD_FLOATS..2 * RECORD_FLOATS].try_into().unwrap();
    r[row::AFFINE * 4..row::AFFINE * 4 + 4].copy_from_slice(&[1.0, 0.0, 0.0, 0.0]);
    assert_eq!(
        Record::new(&r).backdrop(1.0, &list).err(),
        Some(Unsupported::Degenerate)
    );
}

#[test]
fn invalid_backdrop_numbers_are_rejected_before_gaussian_weights_are_built() {
    let list = fixture!("backdrop_blur_x1");
    let original: [f32; RECORD_FLOATS] = list[RECORD_FLOATS..2 * RECORD_FLOATS].try_into().unwrap();
    for (at, value) in [
        (row::FILL * 4, f32::INFINITY),
        (row::FILL * 4, f32::NAN),
        (row::FILL * 4, -1.0),
        (row::FILL * 4 + 1, -1.0),
        (row::FILL_END * 4, f32::NAN),
    ] {
        let mut record = original;
        record[at] = value;
        assert_eq!(
            Record::new(&record).backdrop(1.0, &list).err(),
            Some(Unsupported::BackdropEffect)
        );
    }
    for scale in [0.0, -1.0, f32::INFINITY, f32::NAN] {
        assert_eq!(
            Record::new(&original).backdrop(scale, &list).err(),
            Some(Unsupported::Degenerate)
        );
    }
}
