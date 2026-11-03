//! A slider follows the number it is bound to, whoever writes it.
//!
//! The value can be kept in either precision, so a number input holding
//! an `f64` and a slider can share one number rather than two that are
//! kept in step. Writing it moves the thumb in place: the fill and the rest
//! of the track are bound to it, and the value label is subscribed on its
//! own.
use blinc_core::reactive::{global_graph, signal};
use blinc_layout::div::div;
use blinc_layout::renderer::RenderTree;
use std::sync::Arc;

const WIDTH: f32 = 300.0;
/// Medium thumb, and what the travel is `width - thumb`.
const THUMB: f32 = 16.0;
const TRAVEL: f32 = WIDTH - THUMB;

/// Sets the process up and holds the lock for the test: the queue of bound writes is shared, so
/// tests that drain it cannot run side by side.
fn init() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
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
    guard
}

fn build(value: impl Into<blinc_cn::NumberValue>) -> RenderTree {
    let host = div().w(400.0).h(80.0).child(
        blinc_cn::slider(value)
            .min(0.0)
            .max(100.0)
            .w(WIDTH)
            .label("Volume"),
    );
    let mut tree = RenderTree::from_element(&host);
    tree.compute_layout(400.0, 80.0);
    tree
}

/// How far along its travel the thumb is laid out, after the bound writes
/// queued so far have landed.
fn thumb_along(tree: &mut RenderTree) -> f32 {
    let updates = blinc_layout::take_pending_partial_prop_updates();
    tree.apply_partial_property_updates(updates);
    tree.compute_layout(400.0, 80.0);
    let registry = tree.element_registry();
    let mut stack = vec![tree.root().unwrap()];
    let (mut slider, mut thumb) = (None, None);
    while let Some(node) = stack.pop() {
        if registry.has_class(node, "cn-slider") {
            slider = Some(node);
        }
        if registry.has_class(node, "cn-slider-thumb") {
            thumb = Some(node);
        }
        stack.extend(tree.layout_tree.children(node));
    }
    let slider = tree
        .get_absolute_bounds(slider.expect("no slider"))
        .unwrap();
    let thumb = tree.get_absolute_bounds(thumb.expect("no thumb")).unwrap();
    thumb.x - slider.x
}

/// The case that did not work: a field bound to the same number is
/// typed into, and the slider has to follow it.
#[test]
fn a_write_from_elsewhere_moves_the_thumb() {
    let _guard = init();
    let value = signal::<f64>(0.0);
    let mut tree = build(value);
    assert_eq!(thumb_along(&mut tree), 0.0, "starts at the floor");

    // Nothing rebuilds here. This is the write a number input makes.
    value.set(75.0);
    let actual = thumb_along(&mut tree);
    let expected = 0.75 * TRAVEL;
    assert!(
        (actual - expected).abs() < 1.0,
        "the thumb is three quarters along: {actual} vs {expected}"
    );
}

/// An `f32` state works the same, which is what Rust callers pass.
#[test]
fn either_precision_drives_the_thumb() {
    let _guard = init();
    let value = signal::<f32>(0.0);
    let mut tree = build(value);

    value.set(50.0);
    let actual = thumb_along(&mut tree);
    let expected = 0.5 * TRAVEL;
    assert!(
        (actual - expected).abs() < 1.0,
        "half the travel: {actual} vs {expected}"
    );
}

/// A slider built again over the same number places the thumb from it and
/// follows it after.
#[test]
fn a_rebuilt_slider_follows_the_number() {
    let _guard = init();
    let value = signal::<f64>(20.0);
    let _ = build(value);
    let mut tree = build(value);
    let expected = 0.2 * TRAVEL;
    let actual = thumb_along(&mut tree);
    assert!((actual - expected).abs() < 1.0, "{actual} vs {expected}");

    value.set(60.0);
    let expected = 0.6 * TRAVEL;
    let actual = thumb_along(&mut tree);
    assert!(
        (actual - expected).abs() < 1.0,
        "and it still follows writes after a rebuild: {actual} vs {expected}"
    );
}

/// Every text node's content, in tree order.
fn texts(tree: &RenderTree) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![tree.root().unwrap()];
    while let Some(id) = stack.pop() {
        if let Some(node) = tree.get_render_node(id)
            && let blinc_layout::renderer::ElementType::Text(t) = &node.element_type
        {
            out.push(t.content.clone());
        }
        stack.extend(tree.layout_tree.children(id));
    }
    out
}

/// The number the slider shows follows its state. The text is bound, so a
/// write changes it in place and queues no rebuild of anything.
#[test]
fn the_shown_value_follows_the_state_in_place() {
    let _guard = init();
    let _ = blinc_layout::stateful::take_pending_subtree_rebuilds();
    let _ = blinc_layout::take_pending_partial_prop_updates();

    let value = signal::<f32>(35.0);
    let host = div().w(400.0).h(80.0).child(
        blinc_cn::slider(value)
            .min(0.0)
            .max(100.0)
            .w(WIDTH)
            .label("Volume")
            .show_value(),
    );
    let mut tree = RenderTree::from_element(&host);
    tree.compute_layout(400.0, 80.0);
    assert!(
        texts(&tree).contains(&"35.00".to_string()),
        "{:?}",
        texts(&tree)
    );

    value.set(80.0);
    blinc_layout::stateful::check_stateful_deps(&[value.id()]);
    assert!(
        blinc_layout::stateful::take_pending_subtree_rebuilds().is_empty(),
        "writing the value queued a subtree rebuild"
    );
    let updates = blinc_layout::take_pending_partial_prop_updates();
    tree.apply_partial_property_updates(updates);
    tree.compute_layout(400.0, 80.0);

    assert!(
        texts(&tree).contains(&"80.00".to_string()),
        "{:?}",
        texts(&tree)
    );
    assert!(
        !texts(&tree).contains(&"35.00".to_string()),
        "the old value is still shown"
    );
}
