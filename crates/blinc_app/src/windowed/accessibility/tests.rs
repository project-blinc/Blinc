use super::*;
use blinc_layout::{div::div, renderer::RenderTree, text::text};

fn semantics(content: blinc_layout::Div) -> Vec<AccessibilityNode> {
    blinc_theme::ThemeState::init_default();
    let mut tree = RenderTree::from_element(&div().size(400.0, 300.0).child(content));
    tree.compute_layout(400.0, 300.0);
    tree.accessibility_nodes()
}

#[test]
fn container_hierarchy_and_scaled_bounds_reach_accesskit() {
    let nodes = semantics(
        div()
            .id("dialog")
            .attr("role", "dialog")
            .attr("aria-label", "Edit profile")
            .attr("aria-modal", "true")
            .size(200.0, 150.0)
            .child(
                div()
                    .id("save")
                    .attr("role", "button")
                    .size(80.0, 32.0)
                    .child(text("Save")),
            ),
    );
    let mut cache = AccessibilityCache::default();
    let update = cache.update(&nodes, 1, "Blinc", 2.0, (800, 600)).unwrap();
    let mapped: HashMap<_, _> = update.nodes.into_iter().collect();
    let dialog = NodeId(nodes[0].id.to_raw());
    let save = NodeId(nodes[1].id.to_raw());
    assert_eq!(mapped[&ROOT].children(), &[dialog]);
    assert_eq!(mapped[&dialog].role(), Role::Dialog);
    assert!(mapped[&dialog].is_modal());
    assert_eq!(mapped[&dialog].children(), &[save]);
    assert_eq!(mapped[&save].bounds().unwrap().width(), 160.0);
}

#[test]
fn unchanged_live_content_emits_no_update_and_changes_keep_identity() {
    let mut nodes = semantics(
        div()
            .id("toast")
            .attr("role", "status")
            .size(200.0, 80.0)
            .child(text("Saved")),
    );
    let mut cache = AccessibilityCache::default();
    let first = cache.update(&nodes, 1, "Blinc", 1.0, (400, 300)).unwrap();
    let id = NodeId(nodes[0].id.to_raw());
    let toast = first.nodes.iter().find(|(n, _)| *n == id).unwrap();
    assert_eq!(toast.1.live(), Some(ak::Live::Polite));
    assert!(toast.1.is_live_atomic());
    assert!(cache.update(&nodes, 1, "Blinc", 1.0, (400, 300)).is_none());
    nodes[0].label = "Upload complete".into();
    let changed = cache.update(&nodes, 1, "Blinc", 1.0, (400, 300)).unwrap();
    assert!(changed.tree.is_none());
    assert_eq!(changed.nodes.len(), 1);
    assert_eq!(changed.nodes[0].0, id);
    assert!(cache.update(&nodes, 1, "Blinc", 1.0, (400, 300)).is_none());
}

#[test]
fn reactivation_sends_a_full_tree_and_deactivation_releases_cached_nodes() {
    let nodes = semantics(
        div()
            .attr("role", "alert")
            .size(200.0, 80.0)
            .child(text("Connection lost")),
    );
    let mut cache = AccessibilityCache::default();
    cache.update(&nodes, 1, "Blinc", 1.0, (400, 300)).unwrap();
    let update = cache.update(&nodes, 2, "Blinc", 1.0, (400, 300)).unwrap();
    assert!(update.tree.is_some());
    assert_eq!(update.nodes.len(), nodes.len() + 1);
    cache.reset();
    assert_eq!(cache.nodes.capacity(), 0);
    assert!(cache.focus.is_none());
}

#[test]
fn removal_updates_parent_children_without_reemitting_removed_nodes() {
    let nodes = semantics(
        div().attr("role", "dialog").size(200.0, 150.0).child(
            div()
                .attr("role", "button")
                .size(80.0, 32.0)
                .child(text("Save")),
        ),
    );
    let mut cache = AccessibilityCache::default();
    cache.update(&nodes, 1, "Blinc", 1.0, (400, 300)).unwrap();
    let update = cache.update(&[], 1, "Blinc", 1.0, (400, 300)).unwrap();
    assert_eq!(update.nodes.len(), 1);
    assert_eq!(update.nodes[0].0, ROOT);
    assert!(update.nodes[0].1.children().is_empty());
}

#[test]
fn focus_only_changes_are_published_without_resending_nodes() {
    let mut nodes = semantics(
        div()
            .attr("role", "button")
            .size(80.0, 32.0)
            .child(text("Save")),
    );
    let mut cache = AccessibilityCache::default();
    cache.update(&nodes, 1, "Blinc", 1.0, (400, 300)).unwrap();
    nodes[0].focused = true;
    let update = cache.update(&nodes, 1, "Blinc", 1.0, (400, 300)).unwrap();
    assert_eq!(update.focus, NodeId(nodes[0].id.to_raw()));
    assert!(update.nodes.is_empty());
}

#[test]
fn native_actions_reject_unknown_trees_and_invalid_payloads() {
    let mut request = ak::ActionRequest {
        action: Action::SetValue,
        target_tree: TreeId::ROOT,
        target_node: NodeId(7),
        data: None,
    };
    assert!(action(&request).is_none());
    request.data = Some(ActionData::Value("River".into()));
    assert_eq!(
        action(&request),
        Some((
            StableNodeId::from_raw(7),
            AccessibilityAction::SetValue("River".into())
        ))
    );
    request.target_node = ROOT;
    assert!(action(&request).is_none());
}

#[test]
fn disabled_menu_variants_keep_their_checked_state_without_actions() {
    let nodes = semantics(
        div()
            .attr("role", "menuitemcheckbox")
            .attr("aria-checked", "true")
            .attr("aria-disabled", "true")
            .size(200.0, 40.0)
            .child(text("Show sidebar")),
    );
    let native = native_node(&nodes[0], 1.0);
    assert_eq!(native.role(), Role::MenuItemCheckBox);
    assert_eq!(native.toggled(), Some(ak::Toggled::True));
    assert!(native.is_disabled());
    assert!(!native.supports_action(Action::Click));
    assert!(!native.supports_action(Action::Focus));
}

#[test]
fn text_is_exposed_as_a_native_label_value() {
    let nodes = semantics(div().size(200.0, 40.0).child(text("Visible text")));
    let native = native_node(&nodes[0], 1.0);
    assert_eq!(native.role(), Role::Label);
    assert_eq!(native.value(), Some("Visible text"));
}

#[test]
fn atomic_toasts_announce_once_and_busy_regions_wait() {
    let nodes = semantics(
        div()
            .attr("role", "status")
            .size(200.0, 80.0)
            .child(text("Saved")),
    );
    let mut cache = AccessibilityCache::default();
    let update = cache.update(&nodes, 1, "Blinc", 1.0, (400, 300)).unwrap();
    let native: HashMap<_, _> = update.nodes.into_iter().collect();
    assert_eq!(
        native[&NodeId(nodes[0].id.to_raw())].live(),
        Some(ak::Live::Polite)
    );
    assert_eq!(
        native[&NodeId(nodes[1].id.to_raw())].live(),
        Some(ak::Live::Off)
    );
    let busy = semantics(
        div()
            .attr("role", "status")
            .attr("aria-busy", "true")
            .size(200.0, 80.0)
            .child(text("Uploading")),
    );
    assert_eq!(native_node(&busy[0], 1.0).live(), Some(ak::Live::Off));
}

#[test]
fn registered_custom_roles_keep_their_description_and_advertised_actions() {
    use blinc_layout::accessibility::{AccessibilityActions as A, AccessibilitySemantics};
    let nodes = semantics(
        div()
            .size(200.0, 40.0)
            .attr("aria-label", "Run task")
            .accessibility(
                AccessibilitySemantics::new(AccessibilityRole::Custom("task-action"))
                    .children(false)
                    .actions(A::FOCUS | A::ACTIVATE),
            ),
    );
    let native = native_node(&nodes[0], 1.0);
    assert_eq!(native.role(), Role::Section);
    assert_eq!(native.role_description(), Some("task-action"));
    assert!(native.supports_action(Action::Focus));
    assert!(native.supports_action(Action::Click));
}

#[test]
fn numeric_slider_values_reach_the_native_numeric_property() {
    let nodes = semantics(
        div()
            .attr("role", "slider")
            .attr("aria-valuenow", "42")
            .size(200.0, 32.0),
    );
    assert_eq!(native_node(&nodes[0], 1.0).numeric_value(), Some(42.0));
}

#[test]
fn nonatomic_status_changes_announce_children_instead_of_repeating_the_parent() {
    let nodes = semantics(
        div()
            .attr("role", "status")
            .attr("aria-atomic", "false")
            .size(200.0, 80.0)
            .child(text("Saved")),
    );
    let update = AccessibilityCache::default()
        .update(&nodes, 1, "Blinc", 1.0, (400, 300))
        .unwrap();
    let native: HashMap<_, _> = update.nodes.into_iter().collect();
    assert_eq!(
        native[&NodeId(nodes[0].id.to_raw())].live(),
        Some(ak::Live::Off)
    );
    assert_eq!(
        native[&NodeId(nodes[1].id.to_raw())].live(),
        Some(ak::Live::Polite)
    );
}
