//! A content-sized row whose items carry `flex-shrink: 0` and a negative
//! main-axis margin measures the sum of its items.
//!
//! Three 40px avatars overlapped by 10px are 100 wide. taffy 0.6 measured
//! the row 0 wide at max-content, because its shrink factor was divided by
//! one quantity and multiplied back by another; the same row measured
//! against definite space was unaffected. Fixed in taffy 0.14.

use blinc_layout::LayoutTree;
use taffy::prelude::*;

#[test]
fn an_overlapped_avatar_row_measures_its_real_width() {
    let mut tree = LayoutTree::new();

    // A content-sized row: no explicit width, so taffy measures it.
    let row = tree.create_node(Style {
        display: Display::Flex,
        flex_direction: FlexDirection::Row,
        ..Default::default()
    });

    for i in 0..3 {
        let avatar = tree.create_node(Style {
            size: Size {
                width: length(40.0_f32),
                height: length(40.0_f32),
            },
            flex_shrink: 0.0,
            margin: Rect {
                left: if i == 0 {
                    length(0.0_f32)
                } else {
                    length(-10.0_f32)
                },
                right: length(0.0_f32),
                top: length(0.0_f32),
                bottom: length(0.0_f32),
            },
            ..Default::default()
        });
        tree.add_child(row, avatar);
    }

    // An outer box wide enough that the row is free to take its content
    // width, so what is measured is the row's own contribution.
    let outer = tree.create_node(Style {
        display: Display::Flex,
        flex_direction: FlexDirection::Column,
        align_items: Some(AlignItems::START),
        ..Default::default()
    });
    tree.add_child(outer, row);

    // MaxContent is the branch the defect is reported in:
    // determine_container_main_size's max-content path.
    tree.compute_layout(
        outer,
        Size {
            width: AvailableSpace::MaxContent,
            height: AvailableSpace::MaxContent,
        },
    );

    let w = tree.get_absolute_bounds(row).expect("laid out").width;
    assert!(
        (w - 100.0).abs() < 0.5,
        "row measured {w}, expected 100 (3 x 40 less two 10px overlaps)"
    );
}
