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

/// Absolute position of the deepest node. `location` is
/// parent-relative, and a bound label sits one level deeper than a
/// static one, so only accumulated offsets compare.
fn deepest_origin(tree: &RenderTree) -> (f32, f32) {
    let (mut x, mut y) = (0.0f32, 0.0f32);
    let mut id = tree.root().expect("root");
    loop {
        if let Some(l) = tree.layout_tree.get_layout(id) {
            x += l.location.x;
            y += l.location.y;
        }
        match tree.layout_tree.children(id).first() {
            Some(&c) => id = c,
            None => break,
        }
    }
    (x, y)
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

    // A bound label carries the variant's THEME colour, set in Rust
    // rather than inherited, so it is right on the first frame whether
    // or not a stylesheet pass has run over the rebuilt subtree. It
    // therefore does NOT follow a CSS override of the class -- the
    // trade-off documented on `Badge::reactive_label`.
    let secondary = blinc_theme::ThemeState::get().color(blinc_theme::ColorToken::TextSecondary);
    let expected = [secondary.r, secondary.g, secondary.b, secondary.a];
    assert_eq!(
        first,
        Some(expected),
        "a bound chip must carry the variant's colour on the FIRST frame"
    );
    assert_eq!(second, first, "and must keep it across a rebuild");
}

/// The label must sit in the same place whether it is bound or not.
///
/// A bound label is wrapped by the stateful's content div, and a plain
/// `div()` left-aligns its child rather than centring it, so the chip's
/// `justify_center` stopped reaching the text.
#[test]
fn a_bound_label_sits_where_a_static_one_does() {
    init();
    let stat = build(blinc_cn::badge("Hello World").variant(blinc_cn::BadgeVariant::Secondary));
    let label = State::new(
        signal::<String>("Hello World".to_string()),
        global_graph(),
        global_dirty_flag(),
    );
    let bound = build(
        blinc_cn::badge("")
            .variant(blinc_cn::BadgeVariant::Secondary)
            .reactive_label(Reactive::Bound(label)),
    );

    // (chip width, text width, text x within the chip)
    let geom = |tree: &RenderTree| {
        let chip = tree.layout_tree.children(tree.root().unwrap())[0];
        let chip_w = tree.layout_tree.get_layout(chip).unwrap().size.width;
        let t = deepest(tree);
        let l = tree.layout_tree.get_layout(t).unwrap();
        (chip_w, l.size.width, l.location.x)
    };
    let (sw, stw, sx) = geom(&stat);
    let (bw, btw, bx) = geom(&bound);
    println!("STATIC chip={sw} text={stw} x={sx}");
    println!("BOUND  chip={bw} text={btw} x={bx}");

    assert!(
        (sw - bw).abs() < 1.0,
        "the chip must be the same width: static {sw}, bound {bw}"
    );
    assert!(
        (stw - btw).abs() < 1.0,
        "the label must measure the same: static {stw}, bound {btw}"
    );

    // Position, not just size: the stateful's wrappers must leave the
    // label where the text node they replace would have been.
    let (sox, soy) = deepest_origin(&stat);
    let (box_, boy) = deepest_origin(&bound);
    println!("STATIC origin=({sox}, {soy})  BOUND origin=({box_}, {boy})");
    assert!(
        (soy - boy).abs() < 1.0,
        "the label must sit at the same y: static {soy}, bound {boy}"
    );
    assert!(
        (sox - box_).abs() < 1.0,
        "and the same x: static {sox}, bound {box_}"
    );
}
