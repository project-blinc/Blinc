//! A scroll goes to the container under the pointer that scrolls its axis: a horizontal scroller
//! inside a vertical one takes sideways scrolls and lets vertical ones by to its parent.

use blinc_layout::div::div;
use blinc_layout::renderer::RenderTree;
use blinc_layout::scroll::ScrollDirection;
use blinc_layout::widgets::scroll::{SharedScrollPhysics, scroll};

struct Nested {
    tree: RenderTree,
    outer: SharedScrollPhysics,
    inner: SharedScrollPhysics,
}

fn nested() -> Nested {
    let inner = scroll()
        .id("inner")
        .direction(ScrollDirection::Horizontal)
        .w(200.0)
        .h(60.0)
        .child(div().w(800.0).min_w(800.0).h(60.0));
    let outer = scroll().id("outer").w(200.0).h(100.0);
    let (outer_physics, inner_physics) = (outer.physics(), inner.physics());
    let ui = outer.child(
        div()
            .w_full()
            .flex_col()
            .child(inner)
            .child(div().w_full().h(600.0)),
    );
    let mut tree = RenderTree::from_element(&ui);
    tree.compute_layout(200.0, 100.0);
    Nested {
        tree,
        outer: outer_physics,
        inner: inner_physics,
    }
}

impl Nested {
    fn scroll_over_inner(&mut self, dx: f32, dy: f32) {
        let outer = self.tree.query_by_id("outer").unwrap();
        let inner = self.tree.query_by_id("inner").unwrap();
        self.tree
            .dispatch_scroll_chain(inner, &[outer, inner], 50.0, 30.0, dx, dy);
    }
}

#[test]
fn a_vertical_scroll_over_a_horizontal_scroller_scrolls_its_parent() {
    let mut n = nested();
    n.scroll_over_inner(0.0, -40.0);
    assert!(
        n.outer.lock().unwrap().offset_y < 0.0,
        "the parent did not scroll"
    );
    assert_eq!(n.inner.lock().unwrap().offset_x, 0.0);
}

#[test]
fn a_sideways_scroll_over_a_horizontal_scroller_scrolls_it() {
    let mut n = nested();
    n.scroll_over_inner(-40.0, 0.0);
    let p = n.inner.lock().unwrap();
    assert!(
        p.offset_x < 0.0,
        "the horizontal scroller did not scroll: offset {} content {} viewport {}",
        p.offset_x,
        p.content_width,
        p.viewport_width
    );
    assert_eq!(n.outer.lock().unwrap().offset_y, 0.0);
}
