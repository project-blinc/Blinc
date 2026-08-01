//! A `with` region keeps its content's natural size.
//!
//! The region's wrapper is a flex child like any other. Left at the
//! defaults it was a ROW, which stretches what it holds to its own
//! height, and it shrank, which compresses that content to fit a bounded
//! parent. Inside a scroll viewport the two together pinned a tall page
//! to the viewport and collapsed its trailing rows to zero height —
//! they were laid out, at the right position, measuring nothing.
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
        if !blinc_core::BlincContextState::is_initialized() {
            blinc_core::BlincContextState::init(
                blinc_core::reactive::global_graph(),
                Arc::new(Mutex::new(blinc_core::context_state::HookState::new())),
                Arc::new(std::sync::atomic::AtomicBool::new(false)),
            );
        }
    });
}

/// A row at the end of a region taller than its bounded parent keeps its
/// declared height, rather than being squeezed to nothing.
#[test]
fn a_regions_trailing_row_keeps_its_height_in_a_bounded_parent() {
    init();
    let dsl = BlincDsl::new().expect("runtime init");
    blinc_cn_dsl::register_all(&dsl).expect("register");
    dsl.compile_source(
        r#"signal shown: bool = true

           component Page {
             style { .tall { flex-direction: column; gap: 8px } }
             view {
               Div(class = "tall") {
                 cn.Skeleton(w = 300.0, h = 400.0)
                 cn.Skeleton(w = 300.0, h = 400.0)
                 cn.Skeleton(w = 140.0, h = 140.0)
               }
             }
           }

           view {
             Div(class = "viewport", overflow_scroll = true) {
               with { Page() }
             }
           }"#,
        "region_sizing.blinc",
    )
    .expect("compile");

    let _ = dsl.view_widget();
    let css: String = blinc_core::BlincContextState::get()
        .drain_stylesheets()
        .join("\n");

    // Deliberately shorter than the content: 940 of skeletons in 300.
    let host = div().w(600.0).h(300.0).child_box(dsl.view_widget());
    let mut tree = RenderTree::from_element(&host);
    tree.set_stylesheet(blinc_layout::css_parser::Stylesheet::parse(&css).expect("css"));
    tree.apply_stylesheet_base_styles();
    tree.apply_stylesheet_layout_overrides();
    tree.compute_layout(600.0, 300.0);

    let mut sizes: Vec<(f32, f32)> = Vec::new();
    let mut stack = vec![tree.root().expect("root")];
    while let Some(id) = stack.pop() {
        if let Some(l) = tree.layout_tree.get_layout(id) {
            sizes.push((l.size.width, l.size.height));
        }
        stack.extend(tree.layout_tree.children(id).iter().copied());
    }

    assert!(
        sizes.contains(&(140.0, 140.0)),
        "the last row keeps its declared size rather than collapsing: {sizes:?}"
    );
    assert_eq!(
        sizes
            .iter()
            .filter(|(w, h)| *w == 300.0 && *h == 400.0)
            .count(),
        2,
        "and so do the two before it: {sizes:?}"
    );
}
