//! `align-items: center` from CSS centres differently-sized widgets.
//!
//! Every cn widget built on `w_fit` carries `align_self: Start`, set to
//! stop a content-sized item stretching. Taffy gives `align_self` the
//! last word, so a row of small/medium/large buttons hung from their top
//! edges however the row was styled. The parent's stated alignment now
//! wins.
//!
//! Driven through CSS on purpose: the builder's `.items_center()` and a
//! stylesheet rule reach the taffy style by different routes, and only
//! the CSS one is what a `.blinc` author writes.
use blinc_dsl_core::BlincDsl;
use blinc_layout::div::div;
use blinc_layout::renderer::RenderTree;
use std::sync::{Arc, Mutex};

fn init() {
    static I: std::sync::Once = std::sync::Once::new();
    I.call_once(|| {
        blinc_theme::ThemeState::init_default();
        let s = blinc_animation::AnimationScheduler::new();
        blinc_animation::set_global_scheduler(s.handle());
        blinc_layout::render_state::set_global_scheduler(s.handle());
        Box::leak(Box::new(s));
        blinc_core::reactive::set_stateful_deps_notifier(|ids| {
            blinc_layout::check_stateful_deps(ids);
        });
        if !blinc_core::BlincContextState::is_initialized() {
            blinc_core::BlincContextState::init(
                blinc_core::reactive::global_graph(),
                Arc::new(Mutex::new(blinc_core::context_state::HookState::new())),
                Arc::new(std::sync::atomic::AtomicBool::new(false)),
            );
        }
    });
}

/// (row height, each child's y and height), with the DSL's own
/// stylesheet installed the way the app installs it.
fn row_geometry(src: &str, name: &str, kids: usize) -> (f32, Vec<(f32, f32)>) {
    init();
    let dsl = BlincDsl::new().expect("runtime init");
    blinc_cn_dsl::register_all(&dsl).expect("register");
    dsl.compile_source(src, name).expect("compile");

    let host = div().w(600.0).h(400.0).child_box(dsl.view_widget());
    let mut tree = RenderTree::from_element(&host);
    let css = dsl.compiled_stylesheets().join("\n");
    let sheet = blinc_layout::css_parser::Stylesheet::parse(&css).expect("the sheet parses");
    tree.set_stylesheet(sheet);
    tree.apply_stylesheet_base_styles();
    tree.apply_stylesheet_layout_overrides();
    tree.compute_layout(600.0, 400.0);

    let root = tree.root().expect("root");
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        let centred = tree
            .layout_tree
            .get_style(id)
            .is_some_and(|s| s.align_items == Some(taffy::style::AlignItems::Center));
        let children = tree.layout_tree.children(id);
        if centred && children.len() == kids {
            let row_h = tree.get_absolute_bounds(id).expect("row").height;
            let geo = children
                .iter()
                .map(|c| {
                    let b = tree.get_absolute_bounds(*c).expect("child");
                    (b.y, b.height)
                })
                .collect();
            return (row_h, geo);
        }
        stack.extend(children);
    }
    panic!("no centred row with {kids} children");
}

const SIZES: &str = r#"component C {
    style { .row { gap: 10px; flex-direction: row; align-items: center } }
    view {
      Div(class = "row") {
        cn.Button("small", size = "small")
        cn.Button("medium", size = "medium")
        cn.Button("large", size = "large")
      }
    }
  }
  view { C() }"#;

/// Each button sits centred on the row's cross axis, so a row of mixed
/// sizes reads as one bar rather than a staircase.
#[test]
fn differently_sized_buttons_centre_in_a_css_row() {
    let (row_h, kids) = row_geometry(SIZES, "css_row_sizes.blinc", 3);
    assert!(row_h > 0.0);
    for (i, (y, h)) in kids.iter().enumerate() {
        let expected = (row_h - h) / 2.0;
        assert!(
            (y - expected).abs() < 0.51,
            "button {i}: y={y}, expected ~{expected} (row {row_h}, child {h})",
        );
    }
}

/// The heights genuinely differ, so the test above is not passing
/// because every button happened to be the same size.
#[test]
fn the_sizes_actually_differ() {
    let (_, kids) = row_geometry(SIZES, "css_row_sizes_differ.blinc", 3);
    let heights: Vec<f32> = kids.iter().map(|(_, h)| *h).collect();
    assert!(
        heights[0] < heights[1] && heights[1] < heights[2],
        "small < medium < large: {heights:?}",
    );
}

/// A button beside a badge: the pair from the playground that showed the
/// problem first.
#[test]
fn a_button_and_a_badge_share_a_centre_line() {
    let (row_h, kids) = row_geometry(
        r#"signal ra: string = "left"
           component C {
             style { .row { gap: 10px; flex-direction: row; align-items: center } }
             view {
               Div(class = "row") {
                 cn.Button("Centre it", size = "small")
                 cn.Badge(ra, variant = "secondary")
               }
             }
           }
           view { C() }"#,
        "css_row_badge.blinc",
        2,
    );
    for (i, (y, h)) in kids.iter().enumerate() {
        let expected = (row_h - h) / 2.0;
        assert!(
            (y - expected).abs() < 0.51,
            "child {i}: y={y}, expected ~{expected}",
        );
    }
}

/// An overlay trigger centres against taller siblings.
///
/// `cn.Tooltip` and `cn.HoverCard` wrap their trigger in a `w_fit` div,
/// which is enough to stop it stretching. Both also named
/// `align_self_start()` on top, which made the value look authored and
/// so outrank the row — the trigger hung from the top while its
/// neighbours centred.
#[test]
fn an_overlay_trigger_centres_against_taller_siblings() {
    let (row_h, kids) = row_geometry(
        r#"component C {
             style { .row { gap: 10px; flex-direction: row; align-items: center } }
             view {
               Div(class = "row") {
                 cn.Tooltip(text = "a") { cn.Button("Delete", size = "small") }
                 cn.Tooltip(text = "b") { cn.Label("slow tip") }
               }
             }
           }
           view { C() }"#,
        "css_row_tooltip.blinc",
        2,
    );

    let (short_y, short_h) = kids[1];
    assert!(short_h < row_h, "the label trigger is the shorter one");
    let expected = (row_h - short_h) / 2.0;
    assert!(
        (short_y - expected).abs() < 0.51,
        "the trigger centres: y={short_y}, expected ~{expected} (row {row_h})",
    );
}

/// The same for a hover card, which carried the identical pair.
#[test]
fn a_hover_card_trigger_centres_too() {
    let (row_h, kids) = row_geometry(
        r#"component C {
             style { .row { gap: 10px; flex-direction: row; align-items: center } }
             view {
               Div(class = "row") {
                 cn.Button("tall", size = "large")
                 cn.HoverCard {
                   cn.HoverCardTrigger { cn.Label("hover me instead") }
                   cn.HoverCardContent { cn.Label("card") }
                 }
               }
             }
           }
           view { C() }"#,
        "css_row_hovercard.blinc",
        2,
    );

    let (short_y, short_h) = kids[1];
    assert!(short_h < row_h, "the hover-card trigger is the shorter one");
    let expected = (row_h - short_h) / 2.0;
    assert!(
        (short_y - expected).abs() < 0.51,
        "the trigger centres: y={short_y}, expected ~{expected} (row {row_h})",
    );
}
