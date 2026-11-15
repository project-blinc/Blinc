#![cfg(feature = "headless")]
//! A page with a scroll container inside it that is never painted is not stale at rest: a frame
//! that follows an input or an animation is served from the cache, not painted in full.

use blinc_core::Color;
use blinc_layout::div::div;
use blinc_layout::prelude::*;
use blinc_layout::selector::ScrollRef;
use blinc_recorder::{Headless, MotionPaint};
use blinc_theme::{ColorScheme, HybridTheme};

fn page(page_scroll: &ScrollRef) -> Option<Headless> {
    if !Headless::available() {
        return None;
    }
    let mut ui = Headless::with_motion_paint(300, 200, MotionPaint::Vector).ok()?;
    ui.theme(HybridTheme::bundle(), ColorScheme::Dark);
    let bound = page_scroll.clone();
    ui.mount(move |_| {
        div().w(300.0).h(200.0).child(
            scroll()
                .w_full()
                .h(200.0)
                .viewport_cull(true)
                .bind(&bound)
                .child(
                    div()
                        .w_full()
                        .flex_col()
                        .gap(8.0)
                        .child(div().w_full().h(120.0).bg(Color::rgb(0.2, 0.3, 0.6)))
                        // Far below the viewport: culled, so its scroll container is never
                        // painted.
                        .child(div().w_full().h(2000.0))
                        .child(
                            scroll()
                                .w_full()
                                .h(80.0)
                                .child(div().w_full().h(400.0).bg(Color::rgb(0.5, 0.2, 0.2))),
                        ),
                ),
        )
    });
    ui.settle();
    Some(ui)
}

#[test]
fn a_scroll_container_that_was_never_painted_does_not_make_the_page_stale() {
    let reference = ScrollRef::new();
    let Some(mut ui) = page(&reference) else {
        return;
    };
    ui.snapshot().expect("a frame");
    assert!(
        !ui.tree().scroll_paint_stale(),
        "a page at rest is stale because a container below the viewport was never painted"
    );
}

#[test]
fn a_page_scrolled_since_it_was_painted_is_not_served_from_the_cache() {
    let reference = ScrollRef::new();
    let Some(mut ui) = page(&reference) else {
        return;
    };
    // Off rows, a container that scrolled has to be painted again.
    ui.tree().set_scroll_rows(false);
    let before = ui.snapshot().expect("a frame");
    reference.set_scroll_offset(0.0, 60.0);
    ui.frame(16);
    let after = ui.snapshot_fast().expect("a frame");
    assert!(
        before.data != after.data,
        "the page scrolled and the fast path drew the frame it had"
    );
}
