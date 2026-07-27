//! Where does a bound Badge's label get its colour, and when?
use blinc_core::reactive::{State, global_dirty_flag, global_graph, signal};
use blinc_layout::binding::Reactive;
use blinc_layout::div::div;
use blinc_layout::renderer::RenderTree;
use std::sync::Arc;

fn init() {
    static I: std::sync::Once = std::sync::Once::new();
    I.call_once(|| {
        blinc_theme::ThemeState::init_default();
        if !blinc_animation::is_scheduler_initialized() {
            let s = blinc_animation::AnimationScheduler::new();
            blinc_animation::set_global_scheduler(s.handle());
            Box::leak(Box::new(s));
        }
        if !blinc_core::BlincContextState::is_initialized() {
            blinc_core::BlincContextState::init(
                global_graph(),
                Arc::new(std::sync::Mutex::new(
                    blinc_core::context_state::HookState::new(),
                )),
                Arc::new(std::sync::atomic::AtomicBool::new(false)),
            );
        }
    });
}

/// Deepest node reachable by always taking the first child: the label.
fn deepest(tree: &RenderTree) -> blinc_layout::LayoutNodeId {
    let mut id = tree.root().expect("root");
    while let Some(&c) = tree.layout_tree.children(id).first() {
        id = c;
    }
    id
}

fn build(el: impl blinc_layout::div::ElementBuilder + 'static) -> RenderTree {
    let host = div().w(400.0).h(100.0).child(el);
    let mut tree = RenderTree::from_element(&host);
    // A literal colour, not `var(--…)`: theme variables live in the
    // app's stylesheet bundle, and this test is about whether the class
    // reaches the label at all, not about var resolution.
    let css = ".cn-badge--soft-secondary { color: #ff0000 }";
    tree.set_stylesheet(blinc_layout::css_parser::Stylesheet::parse(css).expect("cn css"));
    tree.apply_stylesheet_layout_overrides();
    tree.apply_stylesheet_base_styles();
    tree.compute_layout(400.0, 100.0);
    tree
}

fn dump(tag: &str, tree: &RenderTree) {
    let mut stack = vec![(tree.root().unwrap(), 0usize)];
    while let Some((id, d)) = stack.pop() {
        println!(
            "{tag} {:indent$}node kids={} text_color={:?}",
            "",
            tree.layout_tree.children(id).len(),
            tree.resolved_text_color(id),
            indent = d * 2
        );
        for c in tree.layout_tree.children(id).into_iter().rev() {
            stack.push((c, d + 1));
        }
    }
}

#[test]
fn bound_and_static_labels_resolve_the_same_colour() {
    init();
    let stat = build(blinc_cn::badge("secondary").variant(blinc_cn::BadgeVariant::Secondary));
    dump("STATIC", &stat);
    let want = stat.resolved_text_color(deepest(&stat));
    println!("STATIC label colour: {want:?}");

    let label = State::new(
        signal::<String>("secondary".to_string()),
        global_graph(),
        global_dirty_flag(),
    );
    let mut tree = build(
        blinc_cn::badge("")
            .variant(blinc_cn::BadgeVariant::Secondary)
            .reactive_label(Reactive::Bound(label)),
    );
    dump("BOUND", &tree);
    let first = tree.resolved_text_color(deepest(&tree));
    println!("BOUND label colour, first frame: {first:?}");

    tree.process_pending_subtree_rebuilds();
    tree.apply_stylesheet_base_styles();
    tree.compute_layout(400.0, 100.0);
    let second = tree.resolved_text_color(deepest(&tree));
    println!("BOUND label colour, after rebuild: {second:?}");

    assert!(want.is_some(), "sanity: the static chip resolves a colour");
    assert_eq!(
        first, want,
        "a bound chip must resolve the same colour on the FIRST frame"
    );
}
