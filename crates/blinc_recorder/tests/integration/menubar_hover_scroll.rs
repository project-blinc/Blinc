#![cfg(feature = "headless")]
//! Moving the pointer between the items of a hover menubar does not move the page's scroll.

use blinc_cn::prelude::*;
use blinc_core::Color;
use blinc_layout::div::div;
use blinc_layout::prelude::*;
use blinc_layout::selector::ScrollRef;
use blinc_layout::tree::LayoutNodeId;
use blinc_recorder::{Headless, MotionPaint};
use blinc_theme::{ColorScheme, HybridTheme};

const SPACER: f32 = 300.0;

fn page(reference: &ScrollRef, motion: MotionPaint) -> Option<Headless> {
    if !Headless::available() {
        return None;
    }
    let mut ui = Headless::with_motion_paint(520, 360, motion).ok()?;
    ui.theme(
        HybridTheme::bundle().with_css(blinc_cn::cn_styles::CN_STYLES),
        ColorScheme::Dark,
    );
    let bound = reference.clone();
    ui.mount(move |_| {
        div().w(520.0).h(360.0).flex_col().child(
            scroll()
                .w_full()
                .h(360.0)
                .viewport_cull(true)
                .bind(&bound)
                .child(
                    div()
                        .w_full()
                        .p(8.0)
                        .flex_col()
                        .gap(24.0)
                        .child(div().w_full().h(SPACER).bg(Color::rgb(0.2, 0.3, 0.6)))
                        .child(
                            div().flex_row().flex_wrap().child(
                                menubar()
                                    .trigger_mode(MenuTriggerMode::Hover)
                                    .menu("File", |m| {
                                        m.item("New", || {})
                                            .item("Open", || {})
                                            .separator()
                                            .item("Exit", || {})
                                    })
                                    .menu("Edit", |m| m.item("Undo", || {}).item("Redo", || {}))
                                    .menu("View", |m| {
                                        m.item("Zoom In", || {}).item("Zoom Out", || {})
                                    }),
                            ),
                        )
                        .child(div().w_full().h(900.0).bg(Color::rgb(0.3, 0.2, 0.5))),
                ),
        )
    });
    ui.settle();
    Some(ui)
}

fn triggers(ui: &Headless) -> Vec<LayoutNodeId> {
    let mut nodes: Vec<LayoutNodeId> = ui
        .tree()
        .element_registry()
        .class_to_nodes_index()
        .get("cn-menubar-trigger")
        .map(|set| set.iter().copied().collect())
        .unwrap_or_default();
    nodes.sort_by(|a, b| {
        let x = |n: &LayoutNodeId| {
            ui.tree()
                .layout_tree
                .get_absolute_bounds(*n)
                .map_or(0.0, |b| b.x)
        };
        x(a).total_cmp(&x(b))
    });
    nodes
}

fn centre(ui: &Headless, node: LayoutNodeId) -> (f32, f32) {
    let b = ui
        .tree()
        .layout_tree
        .get_absolute_bounds(node)
        .expect("bounds");
    (b.x + b.width / 2.0, b.y + b.height / 2.0)
}

fn moving_between_items_keeps_the_scroll(motion: MotionPaint) {
    let reference = ScrollRef::new();
    let Some(mut ui) = page(&reference, motion) else {
        return;
    };
    reference.set_scroll_offset(0.0, 250.0);
    ui.frame(16);
    ui.frame(16);
    let start = reference.offset().1;
    assert!(start > 200.0, "the page did not scroll: {start}");
    let items = triggers(&ui);
    assert_eq!(items.len(), 3, "the menubar has three triggers");
    let mut seen = vec![start];
    for round in 0..3 {
        for node in &items {
            // The trigger sits where the scroll put it, so its absolute position is in the
            // content; the pointer is in the window.
            let (x, y) = centre(&ui, *node);
            let y = y - reference.offset().1;
            ui.move_to(x, y);
            ui.advance(250);
            let open = blinc_layout::overlay_state::overlay_stack()
                .lock()
                .map(|s| s.has_visible_overlays())
                .unwrap_or(false);
            eprintln!(
                "{motion:?} round {round}: pointer ({x:.0}, {y:.0}), overlay open {open}, scroll {}",
                reference.offset().1
            );
            seen.push(reference.offset().1);
            assert!(
                (reference.offset().1 - start).abs() < 0.5,
                "{motion:?}: round {round}: the scroll went from {start} to {} (offsets {seen:?})",
                reference.offset().1
            );
        }
    }
}

#[test]
fn hovering_between_menubar_items_keeps_the_page_scrolled() {
    moving_between_items_keeps_the_scroll(MotionPaint::Vector);
}

#[test]
fn hovering_between_menubar_items_keeps_the_page_scrolled_when_promoted() {
    moving_between_items_keeps_the_scroll(MotionPaint::Promoted);
}

#[test]
fn hovering_between_menubar_items_keeps_the_page_scrolled_on_the_node_table() {
    moving_between_items_keeps_the_scroll(MotionPaint::NodeTable);
}

/// A menubar near the bottom edge of the window opens its dropdown inside the window, above the
/// trigger, not off the top of it.
#[test]
fn a_menubar_near_the_bottom_edge_opens_its_dropdown_inside_the_window() {
    if !Headless::available() {
        return;
    }
    let mut ui = Headless::with_motion_paint(520, 360, MotionPaint::Vector).expect("a session");
    ui.theme(
        HybridTheme::bundle().with_css(blinc_cn::cn_styles::CN_STYLES),
        ColorScheme::Dark,
    );
    ui.mount(move |_| {
        div()
            .w(520.0)
            .h(360.0)
            .flex_col()
            .child(div().w_full().h(300.0).bg(Color::rgb(0.2, 0.3, 0.6)))
            .child(
                menubar()
                    .trigger_mode(MenuTriggerMode::Hover)
                    .menu("File", |m| {
                        m.item("New", || {})
                            .item("Open", || {})
                            .separator()
                            .item("Exit", || {})
                    }),
            )
    });
    ui.settle();
    let trigger = *triggers(&ui).first().expect("a trigger");
    let (x, y) = centre(&ui, trigger);
    ui.move_to(x, y);
    ui.advance(250);
    let content = ui.find(".cn-menubar-content").expect("the dropdown opened");
    let b = ui
        .tree()
        .layout_tree
        .get_absolute_bounds(content)
        .expect("bounds");
    assert!(
        b.y >= 0.0 && b.y + b.height <= 360.0 + 1.0,
        "the dropdown is at y {} with height {}, outside a window 360 high",
        b.y,
        b.height
    );
}
