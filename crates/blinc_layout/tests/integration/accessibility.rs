use blinc_core::reactive::{global_graph, signal};
use blinc_layout::{
    accessibility::{AccessibilityAction, AccessibilityRole},
    div::div,
    renderer::RenderTree,
    text::text,
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

static LOCK: Mutex<()> = Mutex::new(());

fn init() -> std::sync::MutexGuard<'static, ()> {
    let guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        blinc_theme::ThemeState::init(
            blinc_theme::HybridTheme::bundle(),
            blinc_theme::ColorScheme::Dark,
        );
        blinc_core::BlincContextState::init(
            global_graph(),
            Arc::new(Mutex::new(blinc_core::context_state::HookState::new())),
            Arc::new(AtomicBool::new(false)),
        );
    });
    guard
}

#[test]
fn names_reading_order_and_atomic_controls() {
    let _guard = init();
    let mut tree = RenderTree::from_element(
        &div()
            .w(300.0)
            .h(400.0)
            .flex_col()
            .child(text("Title"))
            .child(
                div()
                    .id("save")
                    .attr("role", "button")
                    .w(100.0)
                    .h(40.0)
                    .child(text("Save"))
                    .child(
                        div()
                            .attr("aria-hidden", "true")
                            .child(text("Hidden decoration")),
                    ),
            )
            .child(
                div()
                    .attr("role", "button")
                    .attr("aria-label", "Delete document")
                    .w(100.0)
                    .h(40.0)
                    .child(text("×")),
            )
            .child(div().attr("aria-hidden", "true").child(text("Decorative"))),
    );
    tree.compute_layout(300.0, 400.0);
    let nodes = tree.accessibility_nodes();
    assert_eq!(
        nodes.iter().map(|n| n.label.as_str()).collect::<Vec<_>>(),
        ["Title", "Save", "Delete document"]
    );
    assert_eq!(nodes[1].role, AccessibilityRole::Button);
    assert_eq!(nodes[1].identifier.as_deref(), Some("save"));
    assert!(!tree.perform_accessibility_action(nodes[2].id, AccessibilityAction::Activate));
}

#[test]
fn actions_reject_disabled_hidden_and_removed_controls() {
    let _guard = init();
    let calls = Arc::new(AtomicUsize::new(0));
    let c = calls.clone();
    let mut tree = RenderTree::from_element(
        &div()
            .w(300.0)
            .h(400.0)
            .flex_col()
            .child(
                div()
                    .id("enabled")
                    .attr("role", "button")
                    .w(100.0)
                    .h(40.0)
                    .on_click(move |_| {
                        c.fetch_add(1, Ordering::Relaxed);
                    }),
            )
            .child(
                div()
                    .attr("disabled", "")
                    .child(div().id("disabled").attr("role", "button").w(100.0).h(40.0)),
            ),
    );
    tree.compute_layout(300.0, 400.0);
    let nodes = tree.accessibility_nodes();
    let enabled = nodes
        .iter()
        .find(|n| n.identifier.as_deref() == Some("enabled"))
        .unwrap()
        .id;
    let disabled = nodes
        .iter()
        .find(|n| n.identifier.as_deref() == Some("disabled"))
        .unwrap();
    assert!(disabled.disabled);
    assert!(!tree.perform_accessibility_action(disabled.id, AccessibilityAction::Activate));
    assert!(tree.perform_accessibility_action(enabled, AccessibilityAction::Activate));
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    tree.update_if_changed(&div().w(300.0).h(400.0));
    tree.compute_layout(300.0, 400.0);
    assert!(!tree.perform_accessibility_action(enabled, AccessibilityAction::Activate));
}

#[test]
fn live_input_values_labels_secure_text_and_edit_actions() {
    let _guard = init();
    use blinc_layout::widgets::text_input::{text_input, text_input_data};
    let data = text_input_data();
    let name = signal(String::from("Name"));
    let mut tree = RenderTree::from_element(
        &div()
            .w(300.0)
            .h(400.0)
            .child(text_input(&data).accessibility_label(name)),
    );
    tree.compute_layout(300.0, 400.0);
    let id = tree.accessibility_nodes()[0].id;
    assert_eq!(tree.accessibility_nodes()[0].label, "Name");
    assert!(tree.perform_accessibility_action(id, AccessibilityAction::SetValue("A🦊".into())));
    assert_eq!(tree.accessibility_nodes()[0].value, "A🦊");
    assert!(tree.perform_accessibility_action(
        id,
        AccessibilityAction::ReplaceText {
            start: 1,
            end: 3,
            text: "da".into()
        }
    ));
    assert_eq!(data.lock().unwrap().value, "Ada");
    name.set("Full name".into());
    assert_eq!(tree.accessibility_nodes()[0].label, "Full name");
    data.lock().unwrap().masked = true;
    assert_eq!(tree.accessibility_nodes()[0].value, "•••");
    assert!(tree.accessibility_nodes()[0].secure);
    assert!(tree.perform_accessibility_action(
        id,
        AccessibilityAction::ReplaceText {
            start: 3,
            end: 3,
            text: "!".into()
        }
    ));
    assert_eq!(data.lock().unwrap().value, "Ada!");
}

#[test]
fn keyed_identity_survives_reorder_and_clipped_bounds_follow_scroll() {
    let _guard = init();
    let row = |id: &str| {
        div()
            .id(id)
            .attr("role", "button")
            .w(100.0)
            .h(30.0)
            .flex_shrink_0()
    };
    let mut tree = RenderTree::from_element(
        &div()
            .w(200.0)
            .h(40.0)
            .overflow_clip()
            .flex_col()
            .child(row("first"))
            .child(row("second")),
    );
    tree.compute_layout(200.0, 40.0);
    let before = tree.accessibility_nodes();
    let first = before
        .iter()
        .find(|n| n.identifier.as_deref() == Some("first"))
        .unwrap()
        .id;
    assert_eq!(before[1].bounds.height, 10.0);
    tree.update_if_changed(
        &div()
            .w(200.0)
            .h(40.0)
            .overflow_clip()
            .flex_col()
            .child(row("second"))
            .child(row("first")),
    );
    tree.compute_layout(200.0, 40.0);
    let after = tree.accessibility_nodes();
    assert_eq!(after[1].id, first);
    assert_eq!(after[1].bounds.height, 10.0);
}

#[test]
fn active_overlay_excludes_background_and_closed_overlay_is_removed() {
    let _guard = init();
    use blinc_layout::widgets::overlay::OVERLAY_LAYER_ID;
    let view = |active: &str| {
        div()
            .w(300.0)
            .h(400.0)
            .flex_col()
            .child(text("Background"))
            .child(
                div().id(OVERLAY_LAYER_ID).child(
                    div()
                        .attr("data-focus-scope", "a11y-test")
                        .attr("data-focus-active", active)
                        .w(200.0)
                        .h(100.0)
                        .child(text("Dialog")),
                ),
            )
    };
    let mut tree = RenderTree::from_element(&view("true"));
    tree.compute_layout(300.0, 400.0);
    assert_eq!(
        tree.accessibility_nodes()
            .iter()
            .map(|n| n.label.as_str())
            .collect::<Vec<_>>(),
        ["Dialog"]
    );
    tree.update_if_changed(&view("false"));
    tree.compute_layout(300.0, 400.0);
    assert!(
        tree.accessibility_nodes()
            .iter()
            .any(|n| n.label == "Background")
    );
    assert!(
        !tree
            .accessibility_nodes()
            .iter()
            .any(|n| n.label == "Dialog")
    );
}

#[test]
fn checkbox_is_one_named_control_with_live_checked_state() {
    let _guard = init();
    let checked = signal(false);
    let mut tree = RenderTree::from_element(
        &div()
            .w(300.0)
            .h(400.0)
            .child(blinc_layout::widgets::checkbox(checked).label("Notifications")),
    );
    tree.compute_layout(300.0, 400.0);
    let nodes = tree.accessibility_nodes();
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0].role, AccessibilityRole::Checkbox);
    assert_eq!(nodes[0].label, "Notifications");
    assert_eq!(nodes[0].checked, Some(false));
    assert!(tree.perform_accessibility_action(nodes[0].id, AccessibilityAction::Activate));
    let updates = tree.take_pending_property_updates();
    tree.apply_partial_property_updates(updates);
    assert!(checked.get());
    assert_eq!(tree.accessibility_nodes()[0].checked, Some(true));
}

#[test]
fn computed_accessible_name_wakes_the_host_and_unsubscribes_on_removal() {
    let _guard = init();
    use blinc_layout::widgets::text_input::{text_input, text_input_data};
    let name = signal(String::from("Name"));
    let label = blinc_core::reactive::computed(move |g| format!("{} field", g.get(name).unwrap()));
    let data = text_input_data();
    let widget = text_input(&data).accessibility_label(label);
    let mut tree = RenderTree::from_element(&div().w(300.0).h(400.0).child(widget));
    tree.compute_layout(300.0, 400.0);
    blinc_layout::take_needs_redraw();
    name.set("Full name".into());
    assert!(blinc_layout::peek_needs_redraw());
    assert_eq!(tree.accessibility_nodes()[0].label, "Full name field");
    drop(tree);
    blinc_layout::take_needs_redraw();
    name.set("Removed".into());
    assert!(!blinc_layout::peek_needs_redraw());
}

#[test]
fn explicit_label_references_can_name_a_control_from_hidden_text() {
    let _guard = init();
    let mut tree = RenderTree::from_element(
        &div()
            .w(300.0)
            .h(400.0)
            .flex_col()
            .child(
                div()
                    .attr("role", "button")
                    .attr("aria-labelledby", "name")
                    .w(100.0)
                    .h(40.0)
                    .child(text("×")),
            )
            .child(
                div()
                    .id("name")
                    .attr("aria-hidden", "true")
                    .child(text("Archive item")),
            ),
    );
    tree.compute_layout(300.0, 400.0);
    let nodes = tree.accessibility_nodes();
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0].label, "Archive item");
}
