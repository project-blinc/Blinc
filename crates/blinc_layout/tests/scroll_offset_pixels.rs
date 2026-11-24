//! The scroll offset a frame paints is rounded to whole device pixels, not whole points.

use blinc_layout::div::div;
use blinc_layout::renderer::RenderTree;
use blinc_layout::widgets::scroll::scroll;

fn offset_after(scale: f32, delta: f32) -> f32 {
    let ui = scroll()
        .id("list")
        .w(200.0)
        .h(100.0)
        .child(div().w_full().h(2000.0));
    let mut tree = RenderTree::from_element(&ui);
    tree.set_scale_factor(scale);
    tree.compute_layout(200.0, 100.0);
    let list = tree.query_by_id("list").unwrap();
    tree.dispatch_scroll_chain(list, &[list], 50.0, 50.0, 0.0, delta);
    tree.get_scroll_offset(list).1
}

/// At 3x a glide moves in thirds of a point. Rounded to whole points it advanced two points a
/// frame, then one, which read as judder as it slowed.
#[test]
fn a_scroll_offset_lands_on_a_device_pixel() {
    for delta in [-10.2_f32, -10.4, -10.6, -33.3] {
        let at_3x = offset_after(3.0, delta);
        let device = at_3x * 3.0;
        assert!(
            (device - device.round()).abs() < 1e-3,
            "{delta}: {at_3x} is not a whole number of device pixels"
        );
        assert!(
            (at_3x - delta).abs() <= 1.0 / 6.0 + 1e-3,
            "{delta} became {at_3x}"
        );
    }
    // At 1x it is still a whole pixel.
    assert_eq!(offset_after(1.0, -10.4), -10.0);
}
