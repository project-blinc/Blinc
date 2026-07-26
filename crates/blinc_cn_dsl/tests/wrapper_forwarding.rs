//! Every DSL wrapper must forward the `ElementBuilder` identity methods
//! to the widget it wraps.
//!
//! Without forwarding, the renderer queries the wrapper and gets the
//! trait defaults (`None` / `&[]`). For a CSS-class-driven widget that
//! means its selectors never match: `cn.Badge` and `cn.Alert` rendered
//! with no fill, no border, and whatever text colour they inherited.
//! Widgets styled inline from theme tokens (Spinner, Progress,
//! Skeleton) looked fine, which is why this hid for so long -- only
//! `cn.Button` forwarded, so only it was visibly correct.

use blinc_dsl_core::BlincDsl;
use blinc_layout::div::ElementBuilder;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

fn init() {
    static I: std::sync::Once = std::sync::Once::new();
    I.call_once(|| {
        blinc_theme::ThemeState::init_default();
        if !blinc_animation::is_scheduler_initialized() {
            let scheduler = blinc_animation::AnimationScheduler::new();
            blinc_animation::set_global_scheduler(scheduler.handle());
            Box::leak(Box::new(scheduler));
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

fn classes_for(src: &str, file: &str) -> Vec<String> {
    let _ = tracing_subscriber::fmt::try_init();
    init();
    let dsl = BlincDsl::new().expect("dsl init");
    blinc_cn_dsl::register_all(&dsl).expect("register cn.*");
    dsl.compile_source(src, file).expect("compile");
    let widget = dsl.view_widget();
    fn walk(b: &dyn ElementBuilder, out: &mut Vec<String>) {
        out.extend(b.element_classes().iter().map(|c| c.to_string()));
        for c in b.children_builders() {
            walk(c.as_ref(), out);
        }
    }
    let mut out = Vec::new();
    walk(widget.as_ref(), &mut out);
    out
}

#[test]
fn css_driven_widgets_expose_their_classes() {
    for (src, needle) in [
        (r#"view { cn.Badge("x", variant = "success") }"#, "cn-badge"),
        (r#"view { cn.Alert("x", variant = "warning") }"#, "cn-alert"),
        (r#"view { cn.Button("x") }"#, "cn-button"),
        (r#"view { cn.Card { Text("x") } }"#, "cn-card"),
    ] {
        let classes = classes_for(src, "probe.blinc");
        assert!(
            classes.iter().any(|c| c.contains(needle)),
            "expected a `{needle}` class to reach the renderer, got {classes:?}"
        );
    }
}

/// A wrapper that reports no builder children while `build()` creates
/// layout children leaves the renderer unable to walk into them: they
/// never collect render props. Surfaced as
/// "N layout children but 0 builder children" and, visibly, as
/// cn.Avatar's fallback initials rendering unstyled.
#[test]
fn wrappers_report_the_children_they_build() {
    use blinc_layout::tree::LayoutTree;

    init();
    let dsl = BlincDsl::new().expect("dsl init");
    blinc_cn_dsl::register_all(&dsl).expect("register cn.*");
    dsl.compile_source(r#"view { cn.Avatar(fallback = "AB") }"#, "avatar.blinc")
        .expect("compile");

    let widget = dsl.view_widget();
    let mut tree = LayoutTree::new();
    let root = widget.build(&mut tree);

    // Walk both sides in lockstep: any node with layout children must
    // expose builder children too.
    fn check(b: &dyn ElementBuilder, id: blinc_layout::tree::LayoutNodeId, tree: &LayoutTree) {
        let layout_kids = tree.children(id).len();
        let builder_kids = b.children_builders().len();
        assert!(
            layout_kids == 0 || builder_kids > 0,
            "node has {layout_kids} layout children but 0 builder children"
        );
        for (c, cid) in b.children_builders().iter().zip(tree.children(id)) {
            check(c.as_ref(), cid, tree);
        }
    }
    check(widget.as_ref(), root, &tree);
}
