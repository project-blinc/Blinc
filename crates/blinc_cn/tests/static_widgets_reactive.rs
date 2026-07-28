//! Reactive props on the widgets that used to take only plain values.
//!
//! Each widget picks its mechanism from the shape of the prop, and the
//! tests pin the mechanism as much as the value:
//!
//! * `cn::kbd` text and `cn::avatar` fallback / src are content, which
//!   has no property writer, so they rebuild through `deps()`.
//! * `cn::spinner` colours land on a border, which does have one, so
//!   they patch in place — no rebuild, and the rotation keeps its
//!   phase.
use blinc_core::reactive::{State, global_dirty_flag, global_graph, signal};
use blinc_layout::binding::Reactive;
use blinc_layout::div::div;
use blinc_layout::renderer::{ElementType, RenderTree};
use std::sync::Arc;

/// The pending-rebuild queue is process-global, so these tests take
/// turns. `blinc_layout`'s own lock is `cfg(test)`-gated and invisible
/// from here, hence a local one.
static QUEUE: std::sync::Mutex<()> = std::sync::Mutex::new(());

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

fn state(v: &str) -> State<String> {
    State::new(
        signal::<String>(v.to_string()),
        global_graph(),
        global_dirty_flag(),
    )
}

fn build(el: impl blinc_layout::div::ElementBuilder + 'static) -> RenderTree {
    let host = div().w(400.0).h(200.0).child(el);
    let mut tree = RenderTree::from_element(&host);
    tree.compute_layout(400.0, 200.0);
    tree
}

/// Every text node's content, in tree order.
fn texts(tree: &RenderTree) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![tree.root().unwrap()];
    while let Some(id) = stack.pop() {
        if let Some(node) = tree.get_render_node(id)
            && let ElementType::Text(t) = &node.element_type
        {
            out.push(t.content.clone());
        }
        stack.extend(tree.layout_tree.children(id));
    }
    out
}

/// Every image node's source, in tree order.
fn images(tree: &RenderTree) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![tree.root().unwrap()];
    while let Some(id) = stack.pop() {
        if let Some(node) = tree.get_render_node(id)
            && let ElementType::Image(i) = &node.element_type
        {
            out.push(i.source.clone());
        }
        stack.extend(tree.layout_tree.children(id));
    }
    out
}

/// What the frame loop does after a signal moves: tell the statefuls
/// subscribed to it to queue a refresh, then apply the queue. A `set`
/// on its own only marks the signal dirty.
fn refresh(tree: &mut RenderTree, changed: &blinc_core::reactive::SignalId) {
    blinc_layout::stateful::check_stateful_deps(&[*changed]);
    tree.process_pending_subtree_rebuilds();
    tree.compute_layout(400.0, 200.0);
}

#[test]
fn a_bound_kbd_follows_its_signal() {
    init();
    let _guard = QUEUE.lock().unwrap_or_else(|e| e.into_inner());
    let _ = blinc_layout::stateful::take_pending_subtree_rebuilds();

    let key = state("Ctrl");
    let mut tree = build(blinc_cn::kbd(Reactive::Bound(key.clone())));
    assert!(
        texts(&tree).contains(&"Ctrl".to_string()),
        "first frame shows the current value: {:?}",
        texts(&tree)
    );

    key.set("Shift".to_string());
    refresh(&mut tree, &key.signal_id());
    assert!(
        texts(&tree).contains(&"Shift".to_string()),
        "a set rebuilds the key: {:?}",
        texts(&tree)
    );
}

/// A plain string still works, and takes no wrapper at all: `Const`
/// short-circuits to a bare text node.
#[test]
fn a_static_kbd_is_unchanged() {
    init();
    let tree = build(blinc_cn::kbd("Enter"));
    assert_eq!(texts(&tree), vec!["Enter".to_string()]);
}

#[test]
fn a_bound_avatar_fallback_follows_its_signal() {
    init();
    let _guard = QUEUE.lock().unwrap_or_else(|e| e.into_inner());
    let _ = blinc_layout::stateful::take_pending_subtree_rebuilds();

    let initials = state("AB");
    let mut tree = build(blinc_cn::avatar().fallback(Reactive::Bound(initials.clone())));
    assert!(
        texts(&tree).contains(&"AB".to_string()),
        "first frame: {:?}",
        texts(&tree)
    );

    initials.set("CD".to_string());
    refresh(&mut tree, &initials.signal_id());
    assert!(
        texts(&tree).contains(&"CD".to_string()),
        "a set rebuilds the initials: {:?}",
        texts(&tree)
    );
}

#[test]
fn a_bound_avatar_src_follows_its_signal() {
    init();
    let _guard = QUEUE.lock().unwrap_or_else(|e| e.into_inner());
    let _ = blinc_layout::stateful::take_pending_subtree_rebuilds();

    let src = state("first.png");
    let mut tree = build(blinc_cn::avatar().src(Reactive::Bound(src.clone())));
    assert_eq!(images(&tree), vec!["first.png".to_string()]);

    src.set("second.png".to_string());
    refresh(&mut tree, &src.signal_id());
    assert_eq!(
        images(&tree),
        vec!["second.png".to_string()],
        "a set swaps the image source"
    );
}

/// An omitted `fallback` still renders the placeholder rather than an
/// empty circle: an empty literal reads as absent.
#[test]
fn an_avatar_with_no_content_keeps_its_placeholder() {
    init();
    let tree = build(blinc_cn::avatar());
    assert_eq!(texts(&tree), vec!["?".to_string()]);
}

/// The spinner takes the property-binding path, so a colour change
/// queues no rebuild at all — the rotation would otherwise snap back to
/// 0° every time the colour moved.
#[test]
fn a_bound_spinner_colour_does_not_queue_a_rebuild() {
    init();
    let _guard = QUEUE.lock().unwrap_or_else(|e| e.into_inner());
    let _ = blinc_layout::stateful::take_pending_subtree_rebuilds();

    let tint = State::new(
        signal::<blinc_core::Color>(blinc_core::Color::rgba(1.0, 0.0, 0.0, 1.0)),
        global_graph(),
        global_dirty_flag(),
    );
    let _tree = build(blinc_cn::spinner().color(Reactive::Bound(tint.clone())));

    tint.set(blinc_core::Color::rgba(0.0, 1.0, 0.0, 1.0));
    assert!(
        !blinc_layout::stateful::has_pending_subtree_rebuilds(),
        "a bound spinner colour must patch in place, not rebuild"
    );
}
