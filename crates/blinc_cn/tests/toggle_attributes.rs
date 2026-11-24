//! A toggle is one element whose state is data: `data-state` on or off, the
//! `checked` attribute while on, `data-variant`, `data-size` and `disabled`.
//! A click turns it on and off, a disabled one does not, and a group sets the
//! variant and size for its items and keeps one item on.

use blinc_cn::{ToggleSize, ToggleVariant, toggle, toggle_group, toggle_item};
use blinc_core::reactive::{global_graph, signal};
use blinc_layout::LayoutNodeId;
use blinc_layout::css_parser::Stylesheet;
use blinc_layout::div::ElementBuilder;
use blinc_layout::renderer::RenderTree;
use std::sync::{Arc, Mutex};

static LOCK: Mutex<()> = Mutex::new(());

fn init() {
    static I: std::sync::Once = std::sync::Once::new();
    I.call_once(|| {
        blinc_theme::ThemeState::init(
            blinc_theme::HybridTheme::bundle(),
            blinc_theme::ColorScheme::Dark,
        );
        let s = blinc_animation::AnimationScheduler::new();
        blinc_animation::set_global_scheduler(s.handle());
        blinc_layout::render_state::set_global_scheduler(s.handle());
        Box::leak(Box::new(s));
        if !blinc_core::BlincContextState::is_initialized() {
            blinc_core::BlincContextState::init(
                global_graph(),
                Arc::new(Mutex::new(blinc_core::context_state::HookState::new())),
                Arc::new(std::sync::atomic::AtomicBool::new(false)),
            );
        }
    });
}

fn flag(initial: bool) -> blinc_core::reactive::Signal<bool> {
    signal(initial)
}

fn tree_of(ui: &impl ElementBuilder) -> RenderTree {
    let mut tree = RenderTree::from_element(ui);
    // A signal-driven attribute registers once the tree has a stylesheet.
    tree.set_stylesheet(Stylesheet::parse(".x { opacity: 1 }").expect("css"));
    tree.apply_all_stylesheet_styles();
    tree.compute_layout(300.0, 100.0);
    tree
}

fn nodes(tree: &RenderTree, class: &str) -> Vec<LayoutNodeId> {
    let mut found = tree
        .element_registry()
        .class_to_nodes_index()
        .remove(class)
        .unwrap_or_default();
    // Tree order.
    let mut order = Vec::new();
    let mut stack = vec![tree.root().unwrap()];
    while let Some(n) = stack.pop() {
        order.push(n);
        stack.extend(tree.layout_tree.children(n).into_iter().rev());
    }
    found.sort_by_key(|n| order.iter().position(|o| o == n));
    found
}

fn attr(tree: &RenderTree, node: LayoutNodeId, name: &str) -> Option<String> {
    tree.element_registry().attribute(node, name)
}

fn frame(tree: &mut RenderTree) {
    let updates = blinc_layout::take_pending_partial_prop_updates();
    tree.apply_partial_property_updates(updates);
}

#[test]
fn a_toggle_is_one_element_with_its_state_as_data() {
    init();
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let on = flag(false);
    let ui = toggle(on)
        .variant(ToggleVariant::Outline)
        .size(ToggleSize::Small)
        .label("B");
    let mut tree = tree_of(&ui);
    let all = nodes(&tree, "cn-toggle");
    assert_eq!(all.len(), 1, "one element, not a wrapper and a body");
    let t = all[0];
    assert!(tree.element_registry().has_class(t, "blinc-toggle"));
    assert_eq!(attr(&tree, t, "data-variant").as_deref(), Some("outline"));
    assert_eq!(attr(&tree, t, "data-size").as_deref(), Some("sm"));
    assert_eq!(attr(&tree, t, "data-state").as_deref(), Some("off"));
    assert_eq!(attr(&tree, t, "checked"), None);

    on.set(true);
    frame(&mut tree);
    assert_eq!(attr(&tree, t, "data-state").as_deref(), Some("on"));
    assert!(attr(&tree, t, "checked").is_some());

    on.set(false);
    frame(&mut tree);
    assert_eq!(attr(&tree, t, "data-state").as_deref(), Some("off"));
}

#[test]
fn a_click_toggles_and_a_disabled_toggle_ignores_it() {
    init();
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let on = flag(false);
    let live = toggle(on).label("B");
    let _ = tree_of(&live);
    click(&live);
    assert!(on.get());
    click(&live);
    assert!(!on.get());

    let blocked = toggle(on).label("B").disabled(true);
    let tree = tree_of(&blocked);
    click(&blocked);
    assert!(!on.get(), "a disabled toggle toggled");
    let t = nodes(&tree, "cn-toggle")[0];
    assert!(attr(&tree, t, "disabled").is_some());
}

#[test]
fn a_bound_disabled_follows_its_signal_in_place() {
    init();
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let on = flag(false);
    let locked = flag(true);
    let ui = toggle(on).label("B").disabled(locked);
    let mut tree = tree_of(&ui);
    let t = nodes(&tree, "cn-toggle")[0];
    assert!(attr(&tree, t, "disabled").is_some());
    click(&ui);
    assert!(!on.get(), "a disabled toggle toggled");

    locked.set(false);
    frame(&mut tree);
    assert_eq!(attr(&tree, t, "disabled"), None);
    click(&ui);
    assert!(on.get(), "re-enabled, the click did not toggle it");
}

fn click(el: &impl ElementBuilder) {
    use blinc_core::events::event_types::POINTER_UP;
    let handlers = el.event_handlers().expect("a click handler");
    handlers.dispatch(&blinc_layout::event_handler::EventContext::new(
        POINTER_UP,
        LayoutNodeId::default(),
    ));
}

#[test]
fn a_group_sets_variant_and_size_and_keeps_one_item_on() {
    init();
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let value = signal("a".to_string());
    let ui = toggle_group(value)
        .variant(ToggleVariant::Outline)
        .size(ToggleSize::Large)
        .item(toggle_item("a").label("L"))
        .item(toggle_item("b").label("C").disabled(true))
        .item(toggle_item("c").label("R"));
    let mut tree = tree_of(&ui);

    let group = nodes(&tree, "blinc-toggle-group")[0];
    assert_eq!(
        attr(&tree, group, "data-variant").as_deref(),
        Some("outline")
    );
    assert_eq!(attr(&tree, group, "data-size").as_deref(), Some("lg"));

    let items = nodes(&tree, "blinc-toggle");
    assert_eq!(items.len(), 3);
    let states = |tree: &RenderTree| -> Vec<Option<String>> {
        items.iter().map(|i| attr(tree, *i, "data-state")).collect()
    };
    assert_eq!(
        states(&tree),
        [Some("on".into()), Some("off".into()), Some("off".into())]
    );
    assert!(attr(&tree, items[1], "disabled").is_some());

    value.set("c".to_string());
    frame(&mut tree);
    assert_eq!(
        states(&tree),
        [Some("off".into()), Some("off".into()), Some("on".into())]
    );
}
