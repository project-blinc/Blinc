//! `Div::show` and `Div::for_each`: children that come and go in place. What
//! stays keeps its node, what is new is built where it belongs, and what goes
//! takes the scope it was built under with it.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use blinc_core::reactive::{Signal, signal};
use blinc_layout::LayoutNodeId;
use blinc_layout::binding::with_registry;
use blinc_layout::div::{Div, div};
use blinc_layout::renderer::RenderTree;

/// The pending-write queue and the reactive graph are process-wide.
static LOCK: Mutex<()> = Mutex::new(());

fn serial() -> std::sync::MutexGuard<'static, ()> {
    let guard = LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let _ = blinc_layout::stateful::take_pending_partial_prop_updates();
    let _ = blinc_layout::stateful::take_pending_subtree_rebuilds();
    with_registry(|_| {});
    guard
}

/// Apply what the signals queued and lay out, as a runner does.
fn frame(tree: &mut RenderTree) {
    assert!(
        blinc_layout::stateful::take_pending_subtree_rebuilds().is_empty(),
        "a change queued a subtree rebuild"
    );
    let updates = blinc_layout::stateful::take_pending_partial_prop_updates();
    tree.apply_partial_property_updates(updates);
    tree.compute_layout(200.0, 400.0);
}

fn mount(ui: &Div) -> (RenderTree, LayoutNodeId) {
    let mut tree = RenderTree::from_element(ui);
    tree.compute_layout(200.0, 400.0);
    let root = tree.root().unwrap();
    (tree, root)
}

fn kids(tree: &RenderTree, parent: LayoutNodeId) -> Vec<LayoutNodeId> {
    tree.layout_tree.children(parent)
}

fn y(tree: &RenderTree, node: LayoutNodeId) -> f32 {
    tree.get_absolute_bounds(node).unwrap().y
}

/// A row `n` tall plus 1, so its height says which item it is.
fn row(item: &u32) -> Div {
    div().h(*item as f32 * 10.0)
}

#[test]
fn a_branch_is_built_only_while_it_is_shown() {
    let _g = serial();
    let shown = signal(false);
    let builds = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&builds);
    let ui = div()
        .w(200.0)
        .flex_col()
        .child(div().h(10.0))
        .show(shown, move || {
            counted.fetch_add(1, Ordering::SeqCst);
            div().h(20.0)
        })
        .child(div().h(30.0));
    let (mut tree, root) = mount(&ui);
    assert_eq!(builds.load(Ordering::SeqCst), 0, "built while hidden");
    assert_eq!(kids(&tree, root).len(), 2);

    shown.set(true);
    frame(&mut tree);
    let now = kids(&tree, root);
    assert_eq!(now.len(), 3);
    assert_eq!(builds.load(Ordering::SeqCst), 1);
    assert_eq!(y(&tree, now[2]), 30.0, "the tail is not after the branch");

    shown.set(false);
    frame(&mut tree);
    assert_eq!(kids(&tree, root).len(), 2);
    assert_eq!(y(&tree, kids(&tree, root)[1]), 10.0);

    shown.set(true);
    frame(&mut tree);
    assert_eq!(kids(&tree, root).len(), 3);
    assert_eq!(builds.load(Ordering::SeqCst), 2, "a branch is built again");
}

#[test]
fn a_branch_that_is_shown_at_the_start_is_there_at_the_start() {
    let _g = serial();
    let shown = signal(true);
    let ui = div()
        .w(200.0)
        .flex_col()
        .show(shown, || div().h(20.0))
        .child(div().h(30.0));
    let (mut tree, root) = mount(&ui);
    assert_eq!(kids(&tree, root).len(), 2);

    shown.set(false);
    frame(&mut tree);
    assert_eq!(kids(&tree, root).len(), 1);
}

#[test]
fn show_or_swaps_the_two_branches() {
    let _g = serial();
    let first = signal(true);
    let ui = div()
        .w(200.0)
        .flex_col()
        .show_or(first, || div().h(10.0), || div().h(25.0))
        .child(div().h(30.0));
    let (mut tree, root) = mount(&ui);
    let height = |tree: &RenderTree| {
        let k = kids(tree, root);
        assert_eq!(k.len(), 2);
        tree.get_absolute_bounds(k[0]).unwrap().height
    };
    assert_eq!(height(&tree), 10.0);

    first.set(false);
    frame(&mut tree);
    assert_eq!(height(&tree), 25.0);

    first.set(true);
    frame(&mut tree);
    assert_eq!(height(&tree), 10.0);
}

#[test]
fn a_constant_branch_is_decided_now() {
    let _g = serial();
    let (tree, root) = mount(&div().w(200.0).show(true, || div().h(5.0)).show(false, div));
    assert_eq!(kids(&tree, root).len(), 1);
}

fn list(items: Signal<Vec<u32>>) -> Div {
    div()
        .w(200.0)
        .flex_col()
        .child(div().h(5.0))
        .for_each(items, |item| *item, |item| row(&item))
        .child(div().h(7.0))
}

fn heights(tree: &RenderTree, root: LayoutNodeId) -> Vec<f32> {
    kids(tree, root)
        .iter()
        .map(|&n| tree.get_absolute_bounds(n).unwrap().height)
        .collect()
}

#[test]
fn a_list_has_a_child_per_item_between_its_neighbours() {
    let _g = serial();
    let items = signal(vec![1_u32, 2, 3]);
    let (tree, root) = mount(&list(items));
    assert_eq!(heights(&tree, root), vec![5.0, 10.0, 20.0, 30.0, 7.0]);
}

#[test]
fn an_item_that_stays_keeps_its_node() {
    let _g = serial();
    let items = signal(vec![1_u32, 2, 3]);
    let (mut tree, root) = mount(&list(items));
    let before = kids(&tree, root);

    items.set(vec![4, 1, 2, 3]);
    frame(&mut tree);
    let after = kids(&tree, root);
    assert_eq!(heights(&tree, root), vec![5.0, 40.0, 10.0, 20.0, 30.0, 7.0]);
    assert_eq!(&after[2..5], &before[1..4], "a row was rebuilt");
    assert_eq!(after[5], before[4], "the tail was rebuilt");
    assert_eq!(after[0], before[0], "the head was rebuilt");

    items.set(vec![1, 3]);
    frame(&mut tree);
    let last = kids(&tree, root);
    assert_eq!(heights(&tree, root), vec![5.0, 10.0, 30.0, 7.0]);
    assert_eq!(last[1], after[2]);
    assert_eq!(last[2], after[4]);
}

#[test]
fn reordering_moves_the_nodes_and_keeps_them() {
    let _g = serial();
    let items = signal(vec![1_u32, 2, 3]);
    let (mut tree, root) = mount(&list(items));
    let before = kids(&tree, root);

    items.set(vec![3, 1, 2]);
    frame(&mut tree);
    let after = kids(&tree, root);
    assert_eq!(heights(&tree, root), vec![5.0, 30.0, 10.0, 20.0, 7.0]);
    assert_eq!(
        after,
        vec![before[0], before[3], before[1], before[2], before[4]]
    );
}

#[test]
fn a_list_can_be_emptied_and_filled_again() {
    let _g = serial();
    let items = signal(vec![1_u32, 2]);
    let (mut tree, root) = mount(&list(items));

    items.set(Vec::new());
    frame(&mut tree);
    assert_eq!(heights(&tree, root), vec![5.0, 7.0]);

    items.set(vec![2, 3]);
    frame(&mut tree);
    assert_eq!(heights(&tree, root), vec![5.0, 20.0, 30.0, 7.0]);
}

#[test]
fn an_item_is_built_once_while_it_stays() {
    let _g = serial();
    let items = signal(vec![1_u32, 2]);
    let builds = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&builds);
    let ui = div().w(200.0).for_each(
        items,
        |item| *item,
        move |item| {
            counted.fetch_add(1, Ordering::SeqCst);
            row(&item)
        },
    );
    let (mut tree, _) = mount(&ui);
    assert_eq!(builds.load(Ordering::SeqCst), 2);

    items.set(vec![1, 2, 3]);
    frame(&mut tree);
    assert_eq!(builds.load(Ordering::SeqCst), 3);

    items.set(vec![3, 2, 1]);
    frame(&mut tree);
    assert_eq!(builds.load(Ordering::SeqCst), 3, "a reorder built a row");
}

#[test]
fn a_row_takes_what_it_created_when_it_goes() {
    let _g = serial();
    let items = signal(vec![1_u32, 2]);
    let made: Arc<Mutex<Vec<(u32, Signal<u32>)>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&made);
    let ui = div().w(200.0).for_each(
        items,
        |item| *item,
        move |item| {
            sink.lock().unwrap().push((item, signal(item)));
            row(&item)
        },
    );
    let (mut tree, _) = mount(&ui);
    let alive = |item: u32| {
        made.lock()
            .unwrap()
            .iter()
            .find(|(i, _)| *i == item)
            .map(|(_, s)| s.try_get().is_some())
            .unwrap()
    };
    assert!(alive(1) && alive(2));

    items.set(vec![2]);
    frame(&mut tree);
    assert!(!alive(1), "a removed row's signal outlived it");
    assert!(alive(2), "a row that stayed lost its signal");
}

#[test]
fn a_handler_on_a_later_node_survives_an_insert_before_it() {
    let _g = serial();
    let items = signal(vec![1_u32]);
    let ui = div()
        .w(200.0)
        .flex_col()
        .for_each(items, |item| *item, |item| row(&item).on_click(|_| {}))
        .child(div().h(7.0).on_click(|_| {}));
    let (mut tree, root) = mount(&ui);
    let before = kids(&tree, root);
    let has = |tree: &RenderTree, node| {
        let stable = tree.stable_id(node).expect("stable id");
        tree.handler_registry().get(stable).is_some()
    };
    assert!(has(&tree, before[0]) && has(&tree, before[1]));
    let (row_stable, tail_stable) = (
        tree.stable_id(before[0]).unwrap(),
        tree.stable_id(before[1]).unwrap(),
    );

    items.set(vec![2, 1]);
    frame(&mut tree);
    let after = kids(&tree, root);
    assert_eq!(after.len(), 3);
    assert_eq!(tree.stable_id(after[1]), Some(row_stable), "row id moved");
    assert_eq!(tree.stable_id(after[2]), Some(tail_stable), "tail id moved");
    assert!(after.iter().all(|&n| has(&tree, n)), "a handler was lost");
}

#[test]
fn a_new_row_gets_the_stylesheet() {
    use blinc_core::Brush;
    let _g = serial();
    let items = signal(vec![1_u32]);
    let ui = div()
        .w(200.0)
        .flex_col()
        .for_each(items, |item| *item, |item| row(&item).class("r"));
    let mut tree = RenderTree::from_element(&ui);
    tree.set_stylesheet(
        blinc_layout::css_parser::Stylesheet::parse(".r { background: #ff0000; }").expect("css"),
    );
    tree.apply_stylesheet_base_styles();
    tree.compute_layout(200.0, 400.0);
    let root = tree.root().unwrap();

    items.set(vec![1, 2]);
    frame(&mut tree);
    let fill = |node| match &tree.get_render_node(node).unwrap().props.background {
        Some(Brush::Solid(c)) => (c.r, c.g, c.b),
        _ => (-1.0, -1.0, -1.0),
    };
    for node in kids(&tree, root) {
        assert_eq!(fill(node), (1.0, 0.0, 0.0));
    }
}

#[test]
fn a_list_can_come_from_a_state_a_computed_or_a_plain_vec() {
    use blinc_core::reactive::{State, computed, global_dirty_flag, global_graph};
    let _g = serial();

    let source = signal(vec![1_u32, 2]);
    let state = State::new(source, global_graph(), global_dirty_flag());
    let (mut tree, root) = mount(&div().w(200.0).flex_col().for_each(
        state.clone(),
        |item| *item,
        |item| row(&item),
    ));
    assert_eq!(heights(&tree, root), vec![10.0, 20.0]);
    state.set(vec![1, 2, 3]);
    frame(&mut tree);
    assert_eq!(heights(&tree, root), vec![10.0, 20.0, 30.0]);

    // A list derived from something else is a computed.
    let evens = computed(move |g| {
        g.get(source)
            .unwrap_or_default()
            .into_iter()
            .filter(|n| n % 2 == 0)
            .collect::<Vec<u32>>()
    });
    let (mut tree, root) = mount(&div().w(200.0).flex_col().for_each(
        &evens,
        |item| *item,
        |item| row(&item),
    ));
    source.set(vec![2, 3, 4]);
    frame(&mut tree);
    assert_eq!(heights(&tree, root), vec![20.0, 40.0]);

    // A plain list is shown as it is.
    let (tree, root) = mount(&div().w(200.0).flex_col().for_each(
        vec![5_u32, 6],
        |item| *item,
        |item| row(&item),
    ));
    assert_eq!(heights(&tree, root), vec![50.0, 60.0]);
}

#[test]
fn a_row_that_goes_runs_its_unmount_handler_and_the_ones_that_stay_do_not() {
    let _g = serial();
    let unmounted = Arc::new(Mutex::new(Vec::<u32>::new()));
    let log = Arc::clone(&unmounted);
    let items = signal(vec![1_u32, 2, 3]);
    let ui = div().w(200.0).flex_col().for_each(
        items,
        |item| *item,
        move |item| {
            let log = Arc::clone(&log);
            let id = item;
            row(&item).on_unmount(move |_| log.lock().unwrap().push(id))
        },
    );
    let (mut tree, _root) = mount(&ui);
    assert!(unmounted.lock().unwrap().is_empty());

    items.set(vec![1, 3]);
    frame(&mut tree);
    assert_eq!(*unmounted.lock().unwrap(), vec![2], "row 2 went");

    items.set(vec![3, 1, 4]);
    frame(&mut tree);
    assert_eq!(
        *unmounted.lock().unwrap(),
        vec![2],
        "reordering and adding unmount nothing"
    );

    items.set(vec![]);
    frame(&mut tree);
    let mut gone = unmounted.lock().unwrap().clone();
    gone.sort();
    assert_eq!(gone, vec![1, 2, 3, 4]);
}

/// A write queued before a region reconciles, and made again by the start of a row that leaves in
/// that reconcile, lands in the order the values were produced: the start's is the newer.
#[test]
fn a_departing_rows_start_write_wins_over_an_older_queued_value() {
    use blinc_layout::region::Row;
    let _g = serial();
    let items = signal(vec![1_u32, 2]);
    let opacity = signal(1.0_f32);
    let ui = div()
        .w(200.0)
        .flex_col()
        .child(div().id("target").h(10.0).opacity(opacity))
        .for_each(
            items,
            |item| *item,
            move |item| Row::new(row(&item)).on_leave(move || opacity.set(0.25), || true),
        );
    let (mut tree, _root) = mount(&ui);
    let target = tree.query_by_id("target").expect("target");

    // The region is queued first; then a plain write, which the row's start overtakes.
    items.set(vec![1]);
    opacity.set(0.75);
    frame(&mut tree);

    assert_eq!(opacity.get(), 0.25, "the start ran");
    assert_eq!(
        tree.get_render_node(target).unwrap().props.opacity,
        0.25,
        "the older write landed after the newer one"
    );
}

/// A computed made while a row is built, over a signal another row's start writes in the same
/// reconcile, keeps following its sources.
#[test]
fn a_computed_built_beside_a_departing_start_keeps_following_its_sources() {
    use blinc_core::reactive::computed;
    use blinc_layout::region::Row;
    let _g = serial();
    let items = signal(vec![1_u32]);
    let gone = signal(false);
    let pull = signal(0.5_f32);
    let ui = div().w(200.0).flex_col().for_each(
        items,
        |item| *item,
        move |item| {
            let id = item;
            let shown = computed(move |g: &blinc_core::reactive::ReactiveGraph| {
                if g.get(gone).unwrap_or(false) {
                    0.1
                } else {
                    g.get(pull).unwrap_or(0.0)
                }
            });
            Row::new(div().id(format!("row{id}")).h(10.0).opacity(shown))
                .on_leave(move || gone.set(true), || true)
        },
    );
    let (mut tree, _root) = mount(&ui);

    // Row 1 leaves (its start writes `gone`) while row 2 is built in the same reconcile.
    items.set(vec![2]);
    frame(&mut tree);
    // The re-read the reconcile queued lands in the next drain.
    frame(&mut tree);
    let two = tree.query_by_id("row2").expect("row 2");
    let opacity = |tree: &RenderTree| tree.get_render_node(two).unwrap().props.opacity;
    assert_eq!(opacity(&tree), 0.1, "built after the start wrote");

    gone.set(false);
    pull.set(0.9);
    frame(&mut tree);
    assert_eq!(
        opacity(&tree),
        0.9,
        "the computed stopped following its sources"
    );
}
