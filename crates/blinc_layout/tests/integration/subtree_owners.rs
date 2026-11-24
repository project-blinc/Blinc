//! What a subtree rebuild creates lives as long as the subtree it built: a later rebuild of the
//! same node, removing the node, or dropping the rebuild unapplied disposes it.
use blinc_core::owner::Owner;
use blinc_core::reactive::{effect, signal};
use blinc_layout::stateful::queue_subtree_rebuild_owned;
use blinc_layout::{RenderTree, div};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

static TEST_LOCK: Mutex<()> = Mutex::new(());

fn serial() -> std::sync::MutexGuard<'static, ()> {
    TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner())
}

/// An owner holding an effect that counts writes to `source` into `runs`.
fn counting(source: blinc_core::reactive::Signal<i32>, runs: &Arc<AtomicUsize>) -> Owner {
    let owner = Owner::default();
    let counted = Arc::clone(runs);
    owner.run(|| {
        effect(move |g| {
            let _ = g.get(source);
            counted.fetch_add(1, Ordering::SeqCst);
        })
    });
    owner
}

/// How many effects run on one write.
fn live(source: blinc_core::reactive::Signal<i32>, runs: &Arc<AtomicUsize>) -> usize {
    let before = runs.load(Ordering::SeqCst);
    source.update(|v| v + 1);
    runs.load(Ordering::SeqCst) - before
}

#[test]
fn a_rebuild_keeps_what_it_created_until_the_next_rebuild_or_removal() {
    let _serial = serial();
    let source = signal(0_i32);
    let runs = Arc::new(AtomicUsize::new(0));
    let mut tree = RenderTree::from_element(&div().child(div().child(div())));
    let outer = tree.layout().children(tree.root().unwrap())[0];
    let inner = tree.layout().children(outer)[0];

    queue_subtree_rebuild_owned(inner, div().child(div()), counting(source, &runs));
    assert!(tree.process_pending_subtree_rebuilds());
    assert_eq!(
        live(source, &runs),
        1,
        "the rebuild's effect is live while its subtree is"
    );

    queue_subtree_rebuild_owned(inner, div().child(div()), counting(source, &runs));
    assert!(tree.process_pending_subtree_rebuilds());
    assert_eq!(
        live(source, &runs),
        1,
        "the previous rebuild's effect outlived its subtree"
    );

    // Rebuilding the parent removes `inner`, and with it what built under it.
    blinc_layout::queue_subtree_rebuild(outer, div().child(div()));
    assert!(tree.process_pending_subtree_rebuilds());
    assert_eq!(
        live(source, &runs),
        0,
        "removing the node kept its rebuild's effect"
    );
}

#[test]
fn a_rebuild_dropped_unapplied_disposes_what_it_created() {
    let _serial = serial();
    let source = signal(0_i32);
    let runs = Arc::new(AtomicUsize::new(0));
    let tree = RenderTree::from_element(&div().child(div()));
    let node = tree.layout().children(tree.root().unwrap())[0];
    queue_subtree_rebuild_owned(node, div(), counting(source, &runs));
    drop(blinc_layout::stateful::take_pending_subtree_rebuilds());
    assert_eq!(
        live(source, &runs),
        0,
        "a discarded rebuild's effect is still live"
    );
}

#[test]
fn dropping_the_tree_disposes_what_its_rebuilds_created() {
    let _serial = serial();
    let source = signal(0_i32);
    let runs = Arc::new(AtomicUsize::new(0));
    let mut tree = RenderTree::from_element(&div().child(div()));
    let node = tree.layout().children(tree.root().unwrap())[0];
    queue_subtree_rebuild_owned(node, div().child(div()), counting(source, &runs));
    assert!(tree.process_pending_subtree_rebuilds());
    drop(tree);
    assert_eq!(live(source, &runs), 0, "the tree's rebuild outlived it");
}
