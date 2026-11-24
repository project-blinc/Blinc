//! Titles, descriptions and labels bound to a signal replace their text in
//! place: the text node stays, and nothing around it is rebuilt.

use blinc_core::reactive::{State, global_dirty_flag, global_graph, signal};
use blinc_layout::div::{ElementBuilder, div};
use blinc_layout::renderer::{ElementType, RenderTree};
use std::sync::Arc;

/// The pending-update queues are process-global, so these tests take turns.
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

/// Every text node, id and content, in tree order.
fn texts(tree: &RenderTree) -> Vec<(blinc_layout::LayoutNodeId, String)> {
    let mut out = Vec::new();
    let mut stack = vec![tree.root().unwrap()];
    while let Some(id) = stack.pop() {
        if let Some(node) = tree.get_render_node(id)
            && let ElementType::Text(t) = &node.element_type
        {
            out.push((id, t.content.clone()));
        }
        let mut children = tree.layout_tree.children(id);
        children.reverse();
        stack.extend(children);
    }
    out
}

/// Builds `el`, moves `text` from `from` to `to`, and checks the text node
/// that showed `from` shows `to` with no subtree rebuilt.
fn follows(el: impl ElementBuilder + 'static, text: &State<String>, from: &str, to: &str) {
    let _ = blinc_layout::stateful::take_pending_subtree_rebuilds();
    let _ = blinc_layout::take_pending_partial_prop_updates();
    let host = div().w(400.0).h(200.0).child(el);
    let mut tree = RenderTree::from_element(&host);
    tree.compute_layout(400.0, 200.0);
    let (node, _) = texts(&tree)
        .into_iter()
        .find(|(_, t)| t == from)
        .unwrap_or_else(|| panic!("no text {from:?} in {:?}", texts(&tree)));

    text.set(to.to_string());
    blinc_layout::stateful::check_stateful_deps(&[text.signal_id()]);
    assert!(
        blinc_layout::stateful::take_pending_subtree_rebuilds().is_empty(),
        "{from:?} queued a subtree rebuild"
    );
    let updates = blinc_layout::take_pending_partial_prop_updates();
    tree.apply_partial_property_updates(updates);
    let after = texts(&tree);
    assert!(
        after.contains(&(node, to.to_string())),
        "{from:?} did not become {to:?} on the same node: {after:?}"
    );
}

#[test]
fn an_alert_title_and_description_follow_their_signals() {
    init();
    let _guard = QUEUE.lock().unwrap_or_else(|e| e.into_inner());
    let (title, desc) = (state("Heads up"), state("Saved"));
    let alert = || blinc_cn::alert_box().title(&title).description(&desc);
    follows(alert(), &title, "Heads up", "Careful");
    follows(alert(), &desc, "Saved", "Not saved");
}

#[test]
fn a_card_title_and_description_follow_their_signals() {
    init();
    let _guard = QUEUE.lock().unwrap_or_else(|e| e.into_inner());
    let (title, desc) = (state("Plan"), state("Monthly"));
    let header = || blinc_cn::card_header().title(&title).description(&desc);
    follows(header(), &title, "Plan", "Team plan");
    follows(header(), &desc, "Monthly", "Yearly");
}

#[test]
fn breadcrumb_labels_follow_their_signals() {
    init();
    let _guard = QUEUE.lock().unwrap_or_else(|e| e.into_inner());
    let (home, page) = (state("Home"), state("Settings"));
    let crumbs = || blinc_cn::breadcrumb().item(&home, || {}).current(&page);
    follows(crumbs(), &page, "Settings", "Profile");
    follows(crumbs(), &home, "Home", "Start");
}

#[test]
fn a_navigation_link_description_follows_its_signal() {
    init();
    let _guard = QUEUE.lock().unwrap_or_else(|e| e.into_inner());
    let desc = state("Read the guide");
    follows(
        blinc_cn::navigation_link("Docs").description(&desc),
        &desc,
        "Read the guide",
        "Read the reference",
    );
}
