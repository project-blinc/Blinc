//! Widgets in an `align-items: center` row must line their text up.
use blinc_dsl_core::BlincDsl;
use blinc_layout::div::div;
use blinc_layout::renderer::RenderTree;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

fn init() {
    static I: std::sync::Once = std::sync::Once::new();
    I.call_once(|| {
        blinc_core::reactive::set_stateful_deps_notifier(|ids| {
            blinc_layout::check_stateful_deps(ids);
        });
        blinc_theme::ThemeState::init_default();
        if !blinc_animation::is_scheduler_initialized() {
            let s = blinc_animation::AnimationScheduler::new();
            blinc_animation::set_global_scheduler(s.handle());
            // Two globals hold a scheduler: this one, and
            // `blinc_layout::render_state`'s, which a real app fills
            // from `RenderState::new`. Widgets that animate read the
            // second, so a test that sets only the first panics with a
            // message naming neither.
            blinc_layout::render_state::set_global_scheduler(s.handle());
            Box::leak(Box::new(s));
        }
        if !blinc_core::BlincContextState::is_initialized() {
            blinc_core::BlincContextState::init(
                blinc_core::reactive::global_graph(),
                Arc::new(std::sync::Mutex::new(
                    blinc_core::context_state::HookState::new(),
                )),
                Arc::new(AtomicBool::new(false)),
            );
        }
    });
}

/// Vertical centre of every leaf, in absolute coordinates.
fn leaf_centres(tree: &RenderTree) -> Vec<f32> {
    fn walk(tree: &RenderTree, id: blinc_layout::LayoutNodeId, y: f32, out: &mut Vec<f32>) {
        let l = match tree.layout_tree.get_layout(id) {
            Some(l) => l,
            None => return,
        };
        let top = y + l.location.y;
        let kids = tree.layout_tree.children(id);
        if kids.is_empty() {
            if l.size.height > 0.0 {
                out.push(top + l.size.height / 2.0);
            }
            return;
        }
        for c in kids {
            walk(tree, c, top, out);
        }
    }
    let mut out = Vec::new();
    walk(tree, tree.root().unwrap(), 0.0, &mut out);
    out
}

#[test]
fn a_bound_chip_and_label_share_a_baseline() {
    init();
    let dsl = BlincDsl::new().expect("dsl init");
    blinc_cn_dsl::register_all(&dsl).expect("register cn.*");
    dsl.compile_source(
        r#"
        signal row_text: string
        component Row {
            style { .row { gap: 10px; flex-direction: row; align-items: center } }
            view {
                Div(class = "row") {
                    cn.Badge(label = row_text, variant = "secondary")
                    cn.Label(row_text)
                }
            }
        }
        view { Row() }
        "#,
        "row_align.blinc",
    )
    .expect("compile");
    dsl.set_signal_string("row_text", "Hello World");

    // CN_STYLES too: without it the chip has no padding, both boxes are
    // the same height and any misalignment inside the pill is invisible.
    // Literal padding, so the chip is taller than the label and a
    // misalignment between them actually shows. CN_STYLES states this
    // through `var(--space-…)`, which needs the app's theme bundle.
    let mut css: String = ".cn-badge { padding: 2px 10px }\n".to_string();
    css.push_str(blinc_cn::cn_styles::CN_STYLES);
    css.push('\n');
    css.push_str(
        &blinc_core::BlincContextState::get()
            .drain_stylesheets()
            .join("\n"),
    );
    let host = div().w(600.0).h(120.0).child_box(dsl.view_widget());
    let mut tree = RenderTree::from_element(&host);
    tree.set_stylesheet(blinc_layout::css_parser::Stylesheet::parse(&css).expect("css"));
    tree.apply_stylesheet_layout_overrides();
    tree.apply_stylesheet_base_styles();
    tree.compute_layout(600.0, 120.0);

    let centres = leaf_centres(&tree);
    println!("LEAF CENTRES {centres:?}");
    let (min, max) = centres
        .iter()
        .fold((f32::MAX, f32::MIN), |(lo, hi), c| (lo.min(*c), hi.max(*c)));
    assert!(
        (max - min) < 1.0,
        "text in an align-items:center row must share a centre line, spread {min}..{max}"
    );
}
