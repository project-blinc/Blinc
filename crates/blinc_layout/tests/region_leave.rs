//! A row that asks to leave slowly: it stays mounted, in its place and with
//! what it owns, until it says it is done.

use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use blinc_core::reactive::{Signal, signal};
use blinc_layout::LayoutNodeId;
use blinc_layout::binding::with_registry;
use blinc_layout::div::{Div, div};
use blinc_layout::region::Row;
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

fn frame(tree: &mut RenderTree) {
    assert!(
        blinc_layout::stateful::take_pending_subtree_rebuilds().is_empty(),
        "a change queued a subtree rebuild"
    );
    let updates = blinc_layout::stateful::take_pending_partial_prop_updates();
    tree.apply_partial_property_updates(updates);
    tree.compute_layout(200.0, 400.0);
}

fn kids(tree: &RenderTree, parent: LayoutNodeId) -> Vec<LayoutNodeId> {
    tree.layout_tree.children(parent)
}

fn heights(tree: &RenderTree, root: LayoutNodeId) -> Vec<f32> {
    kids(tree, root)
        .iter()
        .map(|&n| tree.get_absolute_bounds(n).unwrap().height)
        .collect()
}

/// Which items have finished leaving, how often each was told to start, and
/// the signal each item's row created.
#[derive(Clone, Default)]
struct Log {
    finished: Arc<Mutex<HashSet<u32>>>,
    started: Arc<AtomicUsize>,
    made: Arc<Mutex<Vec<(u32, Signal<u32>)>>>,
}

impl Log {
    fn finish(&self, item: u32) {
        self.finished.lock().unwrap().insert(item);
    }

    fn alive(&self, item: u32) -> bool {
        self.made
            .lock()
            .unwrap()
            .iter()
            .filter(|(i, _)| *i == item)
            .any(|(_, s)| s.try_get().is_some())
    }
}

/// A column of rows `item * 10` tall that leave slowly, with a 5 high head
/// and a 7 high tail.
fn list(items: Signal<Vec<u32>>, log: &Log) -> Div {
    let log = log.clone();
    div()
        .w(200.0)
        .flex_col()
        .child(div().h(5.0))
        .for_each(
            items,
            |item| *item,
            move |item| {
                log.made.lock().unwrap().push((item, signal(item)));
                let started = Arc::clone(&log.started);
                let finished = Arc::clone(&log.finished);
                Row::new(div().h(item as f32 * 10.0)).on_leave(
                    move || {
                        started.fetch_add(1, Ordering::SeqCst);
                    },
                    move || finished.lock().unwrap().contains(&item),
                )
            },
        )
        .child(div().h(7.0))
}

fn mount(ui: &Div) -> (RenderTree, LayoutNodeId) {
    let mut tree = RenderTree::from_element(ui);
    tree.compute_layout(200.0, 400.0);
    let root = tree.root().unwrap();
    (tree, root)
}

#[test]
fn a_row_stays_until_it_is_done_and_then_goes() {
    let _g = serial();
    let items = signal(vec![1_u32, 2, 3]);
    let log = Log::default();
    let (mut tree, root) = mount(&list(items, &log));
    let before = kids(&tree, root);

    items.set(vec![1, 3]);
    frame(&mut tree);
    assert_eq!(kids(&tree, root), before, "a leaving row was torn down");
    assert_eq!(log.started.load(Ordering::SeqCst), 1);
    assert!(log.alive(2), "a leaving row lost what it owns");

    frame(&mut tree);
    frame(&mut tree);
    assert_eq!(kids(&tree, root), before);
    assert_eq!(log.started.load(Ordering::SeqCst), 1, "start ran again");

    log.finish(2);
    frame(&mut tree);
    assert_eq!(heights(&tree, root), vec![5.0, 10.0, 30.0, 7.0]);
    assert!(!log.alive(2), "a row that left kept what it owns");
    assert!(log.alive(1) && log.alive(3));
}

#[test]
fn a_leaving_row_keeps_its_place_among_the_others() {
    let _g = serial();
    let items = signal(vec![1_u32, 2, 3]);
    let log = Log::default();
    let (mut tree, root) = mount(&list(items, &log));

    // The middle one goes and a new one comes in at the end.
    items.set(vec![1, 3, 4]);
    frame(&mut tree);
    assert_eq!(heights(&tree, root), vec![5.0, 10.0, 20.0, 30.0, 40.0, 7.0]);

    // The first goes: it is left first, ahead of the rest.
    items.set(vec![3, 4]);
    frame(&mut tree);
    assert_eq!(heights(&tree, root), vec![5.0, 10.0, 20.0, 30.0, 40.0, 7.0]);

    // The last goes: it stays after the one before it.
    items.set(vec![3]);
    frame(&mut tree);
    assert_eq!(heights(&tree, root), vec![5.0, 10.0, 20.0, 30.0, 40.0, 7.0]);

    for item in [1, 2, 4] {
        log.finish(item);
    }
    frame(&mut tree);
    assert_eq!(heights(&tree, root), vec![5.0, 30.0, 7.0]);
}

#[test]
fn a_leaving_row_keeps_its_id_while_rows_come_in_around_it() {
    let _g = serial();
    let items = signal(vec![1_u32, 2]);
    let log = Log::default();
    let (mut tree, root) = mount(&list(items, &log));
    let leaving = kids(&tree, root)[2];
    let id = tree.stable_id(leaving).expect("stable id");

    items.set(vec![1]);
    frame(&mut tree);
    items.set(vec![0, 1]);
    frame(&mut tree);

    assert!(kids(&tree, root).contains(&leaving));
    assert_eq!(tree.stable_id(leaving), Some(id), "its id moved");
}

#[test]
fn it_asks_again_only_while_a_row_is_leaving() {
    let _g = serial();
    let items = signal(vec![1_u32, 2]);
    let log = Log::default();
    let (mut tree, _) = mount(&list(items, &log));
    assert!(!blinc_layout::stateful::has_pending_partial_prop_updates());

    items.set(vec![1]);
    frame(&mut tree);
    assert!(
        blinc_layout::stateful::has_pending_partial_prop_updates(),
        "nothing was queued to ask again"
    );
    frame(&mut tree);
    assert!(blinc_layout::stateful::has_pending_partial_prop_updates());

    log.finish(2);
    frame(&mut tree);
    assert!(
        !blinc_layout::stateful::has_pending_partial_prop_updates(),
        "it keeps asking after the last row is gone"
    );
}

#[test]
fn a_key_that_comes_back_is_a_new_row_and_the_old_one_still_leaves() {
    let _g = serial();
    let items = signal(vec![1_u32, 2]);
    let log = Log::default();
    let (mut tree, root) = mount(&list(items, &log));
    let first = kids(&tree, root)[2];

    items.set(vec![1]);
    frame(&mut tree);
    items.set(vec![1, 2]);
    frame(&mut tree);
    let now = kids(&tree, root);
    assert_eq!(now.len(), 5, "the old row and the new one are both there");
    assert!(now.contains(&first));
    assert_eq!(
        log.made
            .lock()
            .unwrap()
            .iter()
            .filter(|(i, _)| *i == 2)
            .count(),
        2,
        "the item was not built again"
    );

    log.finish(2);
    frame(&mut tree);
    let last = kids(&tree, root);
    assert_eq!(last.len(), 4);
    assert!(!last.contains(&first), "the old row stayed");
}

#[test]
fn a_branch_can_linger_too() {
    let _g = serial();
    let shown = signal(true);
    let log = Log::default();
    let finished = Arc::clone(&log.finished);
    let ui = div().w(200.0).flex_col().show(shown, move || {
        let finished = Arc::clone(&finished);
        Row::new(div().h(10.0)).on_leave(|| {}, move || finished.lock().unwrap().contains(&0))
    });
    let (mut tree, root) = mount(&ui);
    assert_eq!(kids(&tree, root).len(), 1);

    shown.set(false);
    frame(&mut tree);
    frame(&mut tree);
    assert_eq!(kids(&tree, root).len(), 1, "the branch went at once");

    log.finish(0);
    frame(&mut tree);
    assert!(kids(&tree, root).is_empty());
}

#[test]
fn a_region_queued_twice_is_asked_again_once() {
    let _g = serial();
    let items = signal(vec![1_u32, 2, 3]);
    let log = Log::default();
    let (mut tree, _) = mount(&list(items, &log));

    items.set(vec![1, 3]);
    items.set(vec![1]);
    frame(&mut tree);

    let queued = blinc_layout::stateful::take_pending_partial_prop_updates();
    assert_eq!(queued.len(), 1, "duplicates carry on from frame to frame");
}

#[test]
fn what_a_row_queues_when_it_starts_to_leave_lands_in_the_same_frame() {
    let _g = serial();
    let items = signal(vec![1_u32, 2]);
    let flags: Arc<Mutex<Vec<(u32, Signal<bool>)>>> = Arc::new(Mutex::new(Vec::new()));
    let made = Arc::clone(&flags);
    let ui = div().w(200.0).flex_col().for_each(
        items,
        |item| *item,
        move |item| {
            let leaving = signal(false);
            made.lock().unwrap().push((item, leaving));
            Row::new(div().h(10.0).class_when("leaving", leaving))
                .on_leave(move || leaving.set(true), || false)
        },
    );
    let mut tree = RenderTree::from_element(&ui);
    tree.set_stylesheet(
        blinc_layout::css_parser::Stylesheet::parse(".leaving { opacity: 0.25; }").expect("css"),
    );
    tree.apply_stylesheet_base_styles();
    tree.compute_layout(200.0, 400.0);
    let root = tree.root().unwrap();
    let second = kids(&tree, root)[1];
    let opacity = |tree: &RenderTree| tree.get_render_node(second).unwrap().props.opacity;
    assert_eq!(opacity(&tree), 1.0);

    items.set(vec![1]);
    frame(&mut tree);

    assert_eq!(kids(&tree, root).len(), 2, "the row went at once");
    assert_eq!(
        opacity(&tree),
        0.25,
        "the write its start queued waited for the next frame"
    );
}
