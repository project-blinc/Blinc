//! Property updates stay with their owning tree while stable identities survive rebuilds.
use blinc_core::reactive::{ReactiveGraph, State};
use blinc_layout::{RenderTree, div, take_pending_partial_prop_updates};
use std::sync::{Arc, Mutex, atomic::AtomicBool};

static TEST_LOCK: Mutex<()> = Mutex::new(());

fn serial() -> std::sync::MutexGuard<'static, ()> {
    TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner())
}

fn opacity(value: f32) -> State<f32> {
    let graph = Arc::new(Mutex::new(ReactiveGraph::new()));
    let signal = graph.lock().unwrap().create_signal(value);
    State::new(signal, graph, Arc::new(AtomicBool::new(false)))
}

#[test]
fn stable_identity_matches_across_builds_and_requires_a_tree_namespace() {
    let _serial = serial();
    let mut first = RenderTree::from_element(&div().id("card").opacity(0.2));
    let second = RenderTree::from_element(&div().id("card").opacity(0.8));
    assert_eq!(first.root(), second.root());
    let stable = first.stable_id(first.root().unwrap()).unwrap();
    assert_eq!(second.stable_id(second.root().unwrap()), Some(stable));
    assert!(first.update_if_changed(&div().id("card").opacity(0.4)));
    assert_eq!(first.stable_id(first.root().unwrap()), Some(stable));
    assert_eq!(first.layout_id(stable), first.root());
}

#[test]
fn primary_drain_does_not_apply_a_secondary_binding() {
    let _serial = serial();
    let mut first = RenderTree::from_element(&div().id("card").opacity(0.2));
    let value = opacity(0.8);
    let mut second = RenderTree::from_element(&div().id("card").opacity(&value));
    value.set(0.4);
    first.apply_partial_property_updates(take_pending_partial_prop_updates());
    assert_eq!(
        first
            .get_render_node(first.root().unwrap())
            .unwrap()
            .props
            .opacity,
        0.2
    );
    second.apply_partial_property_updates(take_pending_partial_prop_updates());
    assert_eq!(
        second
            .get_render_node(second.root().unwrap())
            .unwrap()
            .props
            .opacity,
        0.4
    );
}

#[test]
fn dropping_a_tree_removes_its_signal_subscriptions_and_queued_writes() {
    let _serial = serial();
    let value = opacity(0.8);
    let tree = RenderTree::from_element(&div().id("card").opacity(&value));
    value.set(0.4);
    drop(tree);
    assert!(
        take_pending_partial_prop_updates().is_empty(),
        "queued writes outlived the tree"
    );
    value.set(0.6);
    assert!(
        take_pending_partial_prop_updates().is_empty(),
        "a dropped tree is still subscribed"
    );
}

#[test]
fn rerouted_updates_keep_their_order_before_newer_writes() {
    let _serial = serial();
    let mut first = RenderTree::from_element(&div().id("card").opacity(0.2));
    let value = opacity(0.8);
    let mut second = RenderTree::from_element(&div().id("card").opacity(&value));
    value.set(0.4);
    let older = take_pending_partial_prop_updates();
    value.set(0.6);
    first.apply_partial_property_updates(older);
    second.apply_partial_property_updates(second.take_pending_property_updates());
    assert_eq!(
        second
            .get_render_node(second.root().unwrap())
            .unwrap()
            .props
            .opacity,
        0.6
    );
}

#[test]
fn dropping_an_old_tree_does_not_unregister_a_replacement_or_other_window() {
    let _serial = serial();
    let value = opacity(0.8);
    let old = RenderTree::from_element(&div().id("card").opacity(&value));
    let mut replacement = RenderTree::from_element(&div().id("card").opacity(&value));
    let mut other = RenderTree::from_element(&div().id("card").opacity(&value));
    assert_eq!(
        old.stable_id(old.root().unwrap()),
        replacement.stable_id(replacement.root().unwrap())
    );
    drop(old);
    value.set(0.6);
    replacement.apply_partial_property_updates(replacement.take_pending_property_updates());
    assert_eq!(
        replacement
            .get_render_node(replacement.root().unwrap())
            .unwrap()
            .props
            .opacity,
        0.6
    );
    assert_eq!(
        other
            .get_render_node(other.root().unwrap())
            .unwrap()
            .props
            .opacity,
        0.8
    );
    other.apply_partial_property_updates(other.take_pending_property_updates());
    assert_eq!(
        other
            .get_render_node(other.root().unwrap())
            .unwrap()
            .props
            .opacity,
        0.6
    );
}

#[test]
fn subtree_rebuilds_wait_for_their_own_tree() {
    let _serial = serial();
    let mut first = RenderTree::from_element(&div().id("first"));
    let mut second = RenderTree::from_element(&div().id("second"));
    second.with_property_updates(|| {
        blinc_layout::queue_subtree_rebuild(
            second.root().unwrap(),
            div().child(div().id("new-child")),
        )
    });
    assert!(!first.process_pending_subtree_rebuilds());
    assert!(first.query_by_id("new-child").is_none());
    assert!(second.process_pending_subtree_rebuilds());
    assert!(
        !second
            .layout_tree
            .children(second.root().unwrap())
            .is_empty()
    );
}

#[test]
fn writes_taken_before_a_full_rebuild_cannot_target_its_replacement() {
    let _serial = serial();
    let value = opacity(0.8);
    let mut tree = RenderTree::from_element(&div().id("card").opacity(&value));
    let stable = tree.stable_id(tree.root().unwrap());
    let old_scope = tree.tree_scope_id();
    value.set(0.4);
    let in_flight = tree.take_pending_property_updates();
    assert!(tree.update_if_changed(&div().id("card").opacity(0.9)));
    assert_eq!(tree.stable_id(tree.root().unwrap()), stable);
    assert_ne!(tree.tree_scope_id(), old_scope);
    tree.apply_partial_property_updates(in_flight);
    assert_eq!(
        tree.get_render_node(tree.root().unwrap())
            .unwrap()
            .props
            .opacity,
        0.9
    );
    value.set(0.6);
    assert!(take_pending_partial_prop_updates().is_empty());
}

#[test]
fn handles_keep_their_tree_namespace_when_local_ids_match() {
    let _serial = serial();
    use blinc_layout::selector::ElementHandle;
    let first = RenderTree::from_element(&div().id("card").opacity(0.2));
    let mut second = RenderTree::from_element(&div().id("card").opacity(0.8));
    let handle = ElementHandle::<()>::new("card", second.element_registry().clone());
    let props = blinc_layout::element::RenderProps {
        opacity: 0.4,
        ..Default::default()
    };
    handle.mark_visual_dirty(props);
    assert!(first.take_pending_property_updates().is_empty());
    second.apply_partial_property_updates(second.take_pending_property_updates());
    assert_eq!(
        first
            .get_render_node(first.root().unwrap())
            .unwrap()
            .props
            .opacity,
        0.2
    );
    assert_eq!(
        second
            .get_render_node(second.root().unwrap())
            .unwrap()
            .props
            .opacity,
        0.4
    );
}

#[test]
fn text_and_computed_layout_updates_stay_in_their_tree() {
    let _serial = serial();
    use blinc_core::Computed;
    use blinc_layout::{binding::Reactive, text::Text};
    let graph = Arc::new(Mutex::new(ReactiveGraph::new()));
    let signal = graph.lock().unwrap().create_signal(20.0_f32);
    let derived = graph
        .lock()
        .unwrap()
        .create_derived(move |graph: &ReactiveGraph| {
            format!("{:.0}", graph.get(signal).unwrap_or_default())
        });
    let value = State::new(signal, graph.clone(), Arc::new(AtomicBool::new(false)));
    let label = Computed::new(derived, graph);
    let first =
        RenderTree::from_element(&div().id("card").w(10.0).child(blinc_layout::text("first")));
    let mut second = RenderTree::from_element(
        &div()
            .id("card")
            .w(&value)
            .child(Text::bound(Reactive::Computed(label))),
    );
    value.set(50.0);
    assert!(first.take_pending_property_updates().is_empty());
    let effects = second.apply_partial_property_updates(second.take_pending_property_updates());
    assert!(effects.needs_layout);
    assert_eq!(
        second
            .layout_tree
            .get_style(second.root().unwrap())
            .unwrap()
            .size
            .width,
        taffy::style_helpers::length::<_, taffy::Dimension>(50.0)
    );
    let child = second.layout_tree.children(second.root().unwrap())[0];
    let blinc_layout::renderer::ElementType::Text(label) =
        &second.get_render_node(child).unwrap().element_type
    else {
        panic!("text")
    };
    assert_eq!(label.content, "50");
    assert_eq!(
        first
            .layout_tree
            .get_style(first.root().unwrap())
            .unwrap()
            .size
            .width,
        taffy::style_helpers::length::<_, taffy::Dimension>(10.0)
    );
}

#[test]
fn class_and_region_bindings_use_the_owning_stable_id_namespace() {
    let _serial = serial();
    let first_flag = blinc_core::reactive::signal(false);
    let second_flag = blinc_core::reactive::signal(false);
    let mut first =
        RenderTree::from_element(&div().id("card").show(first_flag, || div().id("first-row")));
    let mut second = RenderTree::from_element(
        &div()
            .id("card")
            .show(second_flag, || div().id("second-row")),
    );
    assert_eq!(
        first.stable_id(first.root().unwrap()),
        second.stable_id(second.root().unwrap())
    );
    second_flag.set(true);
    assert!(first.take_pending_property_updates().is_empty());
    second.apply_partial_property_updates(second.take_pending_property_updates());
    assert!(first.query_by_id("first-row").is_none());
    assert!(second.query_by_id("second-row").is_some());
    first_flag.set(true);
    first.apply_partial_property_updates(first.take_pending_property_updates());
    assert!(first.query_by_id("first-row").is_some());
    assert!(second.query_by_id("second-row").is_some());

    let graph = Arc::new(Mutex::new(ReactiveGraph::new()));
    let signal = graph.lock().unwrap().create_signal(false);
    let flag = State::new(signal, graph, Arc::new(AtomicBool::new(false)));
    let mut plain = RenderTree::from_element(&div().id("card"));
    let mut styled = RenderTree::from_element(&div().id("card").class_when("on", &flag));
    plain.set_stylesheet(
        blinc_layout::css_parser::Stylesheet::parse(".on { opacity: 0.5 }").unwrap(),
    );
    styled.set_stylesheet(
        blinc_layout::css_parser::Stylesheet::parse(".on { opacity: 0.5 }").unwrap(),
    );
    plain.apply_stylesheet_base_styles();
    styled.apply_stylesheet_base_styles();
    flag.set(true);
    assert!(plain.take_pending_property_updates().is_empty());
    styled.apply_partial_property_updates(styled.take_pending_property_updates());
    assert!(
        !plain
            .element_registry()
            .has_class(plain.root().unwrap(), "on")
    );
    assert!(
        styled
            .element_registry()
            .has_class(styled.root().unwrap(), "on")
    );
}

#[test]
fn a_tree_can_drop_while_a_binding_registry_is_borrowed() {
    let _serial = serial();
    let value = opacity(0.8);
    let tree = RenderTree::from_element(&div().id("card").opacity(&value));
    blinc_layout::binding::with_registry(|_| drop(tree));
    value.set(0.4);
    assert!(take_pending_partial_prop_updates().is_empty());
}

#[test]
fn legacy_stateful_refresh_uses_its_bound_tree_and_stops_after_drop() {
    use blinc_layout::stateful::{ButtonState, Stateful};
    let _serial = serial();
    let widget = Stateful::new(ButtonState::Idle).on_state(|state, element| {
        *element = element.swap().opacity(if *state == ButtonState::Hovered {
            0.4
        } else {
            0.8
        });
    });
    let first = RenderTree::from_element(&div().id("card").opacity(0.2));
    let mut second = RenderTree::from_element(&widget);
    assert_eq!(first.root(), second.root());
    assert!(widget.dispatch_state(ButtonState::Hovered));
    assert!(first.take_pending_property_updates().is_empty());
    let updates = second.take_pending_property_updates();
    assert!(!updates.is_empty());
    assert!(
        updates
            .iter()
            .all(|update| update.tree_scope == second.tree_scope_id())
    );
    second.apply_partial_property_updates(updates);
    assert_eq!(
        second
            .get_render_node(second.root().unwrap())
            .unwrap()
            .props
            .opacity,
        0.4
    );
    drop(second);
    assert!(widget.dispatch_state(ButtonState::Pressed));
    assert!(take_pending_partial_prop_updates().is_empty());
}
