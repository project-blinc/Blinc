use blinc_layout::{div::div, renderer::RenderTree, text::text};
use std::sync::{Arc, Mutex, Once, atomic::AtomicBool};

static INIT: Once = Once::new();
static LOCK: Mutex<()> = Mutex::new(());

fn init() -> std::sync::MutexGuard<'static, ()> {
    let lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    INIT.call_once(|| {
        blinc_theme::ThemeState::init_default();
        blinc_core::BlincContextState::init(
            blinc_core::reactive::global_graph(),
            Arc::new(Mutex::new(blinc_core::context_state::HookState::new())),
            Arc::new(AtomicBool::new(false)),
        );
    });
    lock
}

fn tree(content: blinc_layout::div::Div) -> RenderTree {
    let mut tree = RenderTree::from_element(&div().size(600.0, 500.0).child(content));
    tree.compute_layout(600.0, 500.0);
    tree
}

#[test]
fn a_dialog_exposes_its_role_and_keeps_its_controls() {
    let _lock = init();
    let tree = tree(
        div()
            .attr("role", "dialog")
            .attr("aria-label", "Edit profile")
            .size(300.0, 200.0)
            .child(
                div()
                    .attr("role", "button")
                    .size(80.0, 32.0)
                    .child(text("Save")),
            )
            .child(
                div()
                    .attr("role", "button")
                    .size(80.0, 32.0)
                    .child(text("Cancel")),
            ),
    );
    let nodes = tree.accessibility_nodes();
    assert_eq!(nodes.len(), 3);
    assert_eq!(format!("{:?}", nodes[0].role), "Dialog");
    assert_eq!(
        nodes.iter().map(|n| n.label.as_str()).collect::<Vec<_>>(),
        ["Edit profile", "Save", "Cancel"]
    );
}

#[test]
fn menu_checkbox_and_radio_roles_stay_distinct() {
    let _lock = init();
    let tree = tree(
        div()
            .size(300.0, 200.0)
            .child(
                div()
                    .attr("role", "menuitemcheckbox")
                    .attr("aria-checked", "true")
                    .size(160.0, 32.0)
                    .child(text("Show sidebar")),
            )
            .child(
                div()
                    .attr("role", "menuitemradio")
                    .attr("aria-checked", "false")
                    .size(160.0, 32.0)
                    .child(text("Compact")),
            ),
    );
    let nodes = tree.accessibility_nodes();
    assert_eq!(format!("{:?}", nodes[0].role), "MenuItemCheckbox");
    assert_eq!(format!("{:?}", nodes[1].role), "MenuItemRadio");
    assert_eq!(nodes[0].checked, Some(true));
    assert_eq!(nodes[1].checked, Some(false));
}

#[test]
fn status_and_alert_nodes_keep_their_announcable_text() {
    let _lock = init();
    for (role, expected) in [("status", "Status"), ("alert", "Alert")] {
        let tree = tree(
            div()
                .id("message")
                .attr("role", role)
                .size(300.0, 100.0)
                .child(text("File saved")),
        );
        let nodes = tree.accessibility_nodes();
        assert_eq!(format!("{:?}", nodes[0].role), expected);
        assert_eq!(nodes[0].identifier.as_deref(), Some("message"));
        assert_eq!(nodes[0].label, "File saved");
    }
}

#[test]
fn a_named_region_keeps_its_nested_tree_items() {
    let _lock = init();
    let tree = tree(
        div()
            .attr("role", "region")
            .attr("aria-label", "Files")
            .size(300.0, 200.0)
            .child(
                div().attr("role", "tree").size(250.0, 180.0).child(
                    div()
                        .attr("role", "treeitem")
                        .attr("aria-label", "src")
                        .attr("aria-expanded", "true")
                        .size(200.0, 32.0),
                ),
            ),
    );
    let nodes = tree.accessibility_nodes();
    assert_eq!(nodes.len(), 3);
    assert_eq!(format!("{:?}", nodes[0].role), "Region");
    assert_eq!(format!("{:?}", nodes[1].role), "Tree");
    assert_eq!(format!("{:?}", nodes[2].role), "TreeItem");
    assert_eq!(nodes[2].expanded, Some(true));
}

#[test]
fn live_priority_atomicity_and_busy_state_are_inherited() {
    use blinc_layout::accessibility::AccessibilityLive;
    let _lock = init();
    let tree = tree(
        div()
            .attr("role", "status")
            .attr("aria-busy", "true")
            .size(300.0, 150.0)
            .child(text("Uploading"))
            .child(
                div()
                    .attr("aria-live", "off")
                    .size(200.0, 40.0)
                    .child(text("Decorative count")),
            ),
    );
    let nodes = tree.accessibility_nodes();
    assert_eq!(nodes[0].live.priority, AccessibilityLive::Polite);
    assert!(nodes[0].live.atomic && nodes[0].live.busy);
    assert_eq!(nodes[1].parent, Some(nodes[0].id));
    assert_eq!(nodes[1].live.root, Some(nodes[0].id));
    assert_eq!(nodes[1].live.priority, AccessibilityLive::Polite);
    assert_eq!(nodes[2].live.priority, AccessibilityLive::Off);
    assert_eq!(nodes[3].live.priority, AccessibilityLive::Off);
}

#[test]
fn an_assistive_focus_request_publishes_noneditable_focus() {
    use blinc_layout::{accessibility::AccessibilityAction, event_router::EventRouter};
    let _lock = init();
    let mut tree = tree(
        div()
            .id("save")
            .attr("role", "button")
            .attr("tabindex", "0")
            .size(80.0, 32.0)
            .child(text("Save")),
    );
    let id = tree.accessibility_nodes()[0].id;
    let mut router = EventRouter::new();
    assert!(tree.perform_accessibility_action_with_router(
        &mut router,
        id,
        AccessibilityAction::Focus
    ));
    assert_eq!(router.focused(), tree.layout_id(id));
    assert!(tree.accessibility_nodes()[0].focused);
    assert!(!tree.accessibility_nodes()[0].disabled);
}

#[test]
fn described_by_resolves_description_text() {
    let _lock = init();
    let tree = tree(
        div()
            .size(300.0, 150.0)
            .child(text("This action deletes the draft").id("description"))
            .child(
                div()
                    .attr("role", "dialog")
                    .attr("aria-label", "Review deletion")
                    .attr("aria-describedby", "description")
                    .size(200.0, 80.0),
            ),
    );
    let nodes = tree.accessibility_nodes();
    assert_eq!(nodes[1].hint, "This action deletes the draft");
}

#[test]
fn a_custom_component_registers_its_role_and_actions_without_a_role_lookup() {
    use blinc_layout::accessibility::{
        AccessibilityAction, AccessibilityActions as A, AccessibilityBinding, AccessibilityRole,
        AccessibilitySemantics, AccessibilityValue,
    };
    let _lock = init();
    let value = blinc_core::reactive::signal(3_i32);
    let mut tree = tree(
        div()
            .id("tasks")
            .size(200.0, 40.0)
            .accessibility(
                AccessibilitySemantics::new(AccessibilityRole::Custom("task-counter"))
                    .children(false)
                    .actions(A::INCREMENT)
                    .binding(AccessibilityBinding {
                        read: Arc::new(move || AccessibilityValue {
                            label: Some("Tasks".into()),
                            value: value.get().to_string(),
                            ..Default::default()
                        }),
                        action: Arc::new(move |action| {
                            if *action == AccessibilityAction::Increment {
                                value.set(value.get() + 1);
                                true
                            } else {
                                false
                            }
                        }),
                    }),
            )
            .child(text("Tasks")),
    );
    let nodes = tree.accessibility_nodes();
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0].role, AccessibilityRole::Custom("task-counter"));
    assert_eq!(nodes[0].label, "Tasks");
    assert_eq!(nodes[0].value, "3");
    assert!(tree.perform_accessibility_action(nodes[0].id, AccessibilityAction::Increment));
    assert_eq!(tree.accessibility_nodes()[0].value, "4");
}

#[test]
fn registration_controls_children_and_live_behavior() {
    use blinc_layout::accessibility::{
        AccessibilityLive, AccessibilityRole, AccessibilitySemantics,
    };
    let _lock = init();
    let make = |children| {
        div()
            .id("log")
            .size(250.0, 120.0)
            .accessibility(
                AccessibilitySemantics::new(AccessibilityRole::Custom("event-log"))
                    .children(children)
                    .live(AccessibilityLive::Polite, false),
            )
            .child(text("Build complete"))
    };
    let mut tree = tree(make(false));
    assert_eq!(tree.accessibility_nodes().len(), 1);
    tree.update_if_changed(&div().size(600.0, 500.0).child(make(true)));
    tree.compute_layout(600.0, 500.0);
    let nodes = tree.accessibility_nodes();
    assert_eq!(nodes.len(), 2, "metadata changed without a visual change");
    assert_eq!(nodes[1].parent, Some(nodes[0].id));
    assert_eq!(nodes[0].live.priority, AccessibilityLive::Polite);
    assert!(!nodes[0].live.atomic);
}

#[test]
fn a_registered_focus_action_uses_the_shared_focus_router() {
    use blinc_layout::{
        accessibility::{
            AccessibilityAction, AccessibilityActions as A, AccessibilityRole,
            AccessibilitySemantics,
        },
        event_router::EventRouter,
    };
    let _lock = init();
    let mut tree = tree(div().id("custom").size(80.0, 32.0).accessibility(
        AccessibilitySemantics::new(AccessibilityRole::Custom("custom-control")).actions(A::FOCUS),
    ));
    let id = tree.accessibility_nodes()[0].id;
    let mut router = EventRouter::new();
    assert!(tree.perform_accessibility_action_with_router(
        &mut router,
        id,
        AccessibilityAction::Focus
    ));
    assert!(tree.accessibility_nodes()[0].focused);
}

#[test]
fn a_boxed_custom_element_supplies_semantics_through_the_trait() {
    use blinc_layout::{
        accessibility::{AccessibilityRole, AccessibilitySemantics},
        div::{Div, ElementBuilder},
        element::RenderProps,
        tree::{LayoutNodeId, LayoutTree},
    };
    struct Custom(Div, Arc<AccessibilitySemantics>);
    impl ElementBuilder for Custom {
        fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
            self.0.build(tree)
        }
        fn render_props(&self) -> RenderProps {
            self.0.render_props()
        }
        fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
            self.0.children_builders()
        }
        fn accessibility_semantics(&self) -> Option<Arc<AccessibilitySemantics>> {
            Some(self.1.clone())
        }
    }
    let _lock = init();
    let custom: Box<dyn ElementBuilder> = Box::new(Custom(
        div().size(200.0, 40.0).child(text("Task ready")),
        Arc::new(
            AccessibilitySemantics::new(AccessibilityRole::Custom("task-card")).children(false),
        ),
    ));
    let tree = tree(div().size(300.0, 100.0).child(custom));
    let nodes = tree.accessibility_nodes();
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0].role, AccessibilityRole::Custom("task-card"));
    assert_eq!(nodes[0].label, "Task ready");
}

#[test]
fn component_live_defaults_override_standard_role_defaults() {
    use blinc_layout::accessibility::{
        AccessibilityLive, AccessibilityRole, AccessibilitySemantics,
    };
    let _lock = init();
    let tree = tree(
        div()
            .size(250.0, 80.0)
            .accessibility(
                AccessibilitySemantics::new(AccessibilityRole::Status)
                    .live(AccessibilityLive::Assertive, false),
            )
            .child(text("Ready")),
    );
    let node = &tree.accessibility_nodes()[0];
    assert_eq!(node.live.priority, AccessibilityLive::Assertive);
    assert!(!node.live.atomic);
}

#[test]
fn incremental_registration_replaces_an_unchanged_looking_provider() {
    use blinc_layout::accessibility::{
        AccessibilityBinding, AccessibilityRole, AccessibilitySemantics, AccessibilityValue,
    };
    let _lock = init();
    let make = |name: &'static str| {
        div().size(600.0, 500.0).child(
            div().id("provider").size(200.0, 40.0).accessibility(
                AccessibilitySemantics::new(AccessibilityRole::Custom("provider"))
                    .children(false)
                    .binding(AccessibilityBinding {
                        read: Arc::new(move || AccessibilityValue {
                            label: Some(name.into()),
                            ..Default::default()
                        }),
                        action: Arc::new(|_| false),
                    }),
            ),
        )
    };
    let mut tree = RenderTree::from_element(&make("First provider"));
    tree.compute_layout(600.0, 500.0);
    let id = tree.accessibility_nodes()[0].id;
    tree.incremental_update(&make("Second provider"));
    tree.compute_layout(600.0, 500.0);
    let nodes = tree.accessibility_nodes();
    assert_eq!(nodes[0].id, id);
    assert_eq!(nodes[0].label, "Second provider");
}

#[test]
fn visible_overflow_children_survive_a_zero_sized_wrapper() {
    let _lock = init();
    let tree = tree(
        div().size(0.0, 0.0).child(
            div()
                .absolute()
                .left(20.0)
                .top(20.0)
                .attr("role", "button")
                .size(80.0, 32.0)
                .child(text("Save")),
        ),
    );
    assert_eq!(tree.accessibility_nodes()[0].label, "Save");
}

#[test]
fn an_auto_sized_root_does_not_clip_a_visible_overlay_to_its_content_height() {
    let _lock = init();
    let mut tree = RenderTree::from_element(
        &div().w(200.0).h(0.0).child(
            div()
                .absolute()
                .top(60.0)
                .attr("role", "tooltip")
                .size(150.0, 32.0)
                .child(text("Share workspace")),
        ),
    );
    tree.compute_layout(200.0, 200.0);
    assert_eq!(tree.accessibility_nodes()[0].label, "Share workspace");
}

#[test]
fn invisible_registered_elements_are_not_exposed() {
    use blinc_layout::accessibility::{AccessibilityRole, AccessibilitySemantics};
    let _lock = init();
    let tree = tree(
        div()
            .size(200.0, 100.0)
            .visible(false)
            .accessibility(AccessibilitySemantics::new(AccessibilityRole::Dialog))
            .child(
                div()
                    .attr("role", "button")
                    .size(80.0, 32.0)
                    .child(text("Hidden action")),
            ),
    );
    assert!(tree.accessibility_nodes().is_empty());
}

#[test]
fn tree_item_names_do_not_include_the_names_of_nested_items() {
    let _lock = init();
    let tree = tree(
        div().attr("role", "tree").size(300.0, 200.0).child(
            div()
                .attr("role", "treeitem")
                .size(250.0, 120.0)
                .child(text("src"))
                .child(
                    div()
                        .attr("role", "treeitem")
                        .size(200.0, 32.0)
                        .child(text("main.rs")),
                ),
        ),
    );
    let items: Vec<_> = tree
        .accessibility_nodes()
        .into_iter()
        .filter(|n| n.role == blinc_layout::accessibility::AccessibilityRole::TreeItem)
        .collect();
    assert_eq!(
        items.iter().map(|n| n.label.as_str()).collect::<Vec<_>>(),
        ["src", "main.rs"]
    );
    assert_eq!(items[1].parent, Some(items[0].id));
}
