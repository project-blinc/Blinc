//! A combobox is used in place: typing filters the open list's rows without
//! rebuilding the search field, a search that matches nothing offers its own
//! text as the value, and choosing closes the list and shows the choice.

mod common;

use blinc_core::context_state::BlincContextState;
use blinc_core::reactive::{State, global_dirty_flag, global_graph, signal};
use blinc_layout::LayoutNodeId;
use blinc_layout::div::div;
use blinc_layout::event_router::{EventRouter, MouseButton};
use blinc_layout::renderer::{ElementType, RenderTree};
use common::*;

struct Scene {
    tree: RenderTree,
    router: EventRouter,
    value: State<String>,
    query: State<String>,
}

fn scene(key: &str) -> Scene {
    init();
    let value = State::new(signal(String::new()), global_graph(), global_dirty_flag());
    let combobox = blinc_cn::ComboboxBuilder::with_key(key, &value)
        .placeholder("Pick a fruit")
        .option("apple", "Apple")
        .option("pear", "Pear")
        .option("plum", "Plum")
        .allow_custom(true)
        .w(220.0);
    let host = div().w(400.0).h(400.0).child(combobox);
    let mut tree = RenderTree::from_element(&host);
    tree.compute_layout(400.0, 400.0);
    // What the search field writes as it is typed in.
    let query =
        BlincContextState::get().use_state_keyed(&format!("{key}_search_query"), String::new);
    Scene {
        tree,
        router: EventRouter::new(),
        value,
        query,
    }
}

impl Scene {
    fn find_class(&self, class: &str) -> Vec<LayoutNodeId> {
        let registry = self.tree.element_registry();
        let mut found = Vec::new();
        let mut stack = vec![self.tree.root().unwrap()];
        while let Some(node) = stack.pop() {
            if registry.has_class(node, class) {
                found.push(node);
            }
            let mut children = self.tree.layout_tree.children(node);
            children.reverse();
            stack.extend(children);
        }
        found
    }

    fn texts_under(&self, from: LayoutNodeId) -> Vec<String> {
        let mut out = Vec::new();
        let mut stack = vec![from];
        while let Some(node) = stack.pop() {
            if let Some(ElementType::Text(t)) =
                self.tree.get_render_node(node).map(|r| &r.element_type)
            {
                out.push(t.content.clone());
            }
            let mut children = self.tree.layout_tree.children(node);
            children.reverse();
            stack.extend(children);
        }
        out
    }

    fn trigger(&self) -> LayoutNodeId {
        self.find_class("cn-combobox-trigger")[0]
    }

    fn list(&self) -> Option<LayoutNodeId> {
        self.find_class("cn-combobox-content").first().copied()
    }

    /// The search field: the first child of the list's search row.
    fn search_field(&self) -> LayoutNodeId {
        let list = self.list().expect("the list is not open");
        let search_row = self.tree.layout_tree.children(list)[0];
        self.tree.layout_tree.children(search_row)[0]
    }

    /// What the runner does after an event.
    fn frame(&mut self) {
        assert!(
            !blinc_layout::stateful::has_pending_subtree_rebuilds(),
            "the combobox queued a subtree rebuild"
        );
        let updates = blinc_layout::take_pending_partial_prop_updates();
        self.tree.apply_partial_property_updates(updates);
        self.tree.compute_layout(400.0, 400.0);
    }

    fn click(&mut self, node: LayoutNodeId) {
        let b = self.tree.get_absolute_bounds(node).unwrap();
        let (x, y) = (b.x + b.width / 2.0, b.y + b.height / 2.0);
        let mut events = self.router.on_mouse_move(&self.tree, x, y);
        events.extend(
            self.router
                .on_mouse_down(&self.tree, x, y, MouseButton::Left),
        );
        events.extend(self.router.on_mouse_up(&self.tree, x, y, MouseButton::Left));
        for (node, event) in events {
            self.tree.dispatch_event(node, event, x, y);
        }
        self.frame();
    }

    fn type_search(&mut self, text: &str) {
        self.query.set(text.to_string());
        self.frame();
    }
}

#[test]
fn typing_filters_the_rows_in_place() {
    guarded(|| {
        let mut s = scene("cb_filter");
        s.click(s.trigger());
        assert_eq!(s.find_class("cn-combobox-item").len(), 3);
        let field = s.search_field();

        s.type_search("plu");
        let rows = s.find_class("cn-combobox-item");
        assert_eq!(rows.len(), 1, "the rows did not follow the search");
        assert_eq!(s.texts_under(rows[0]), vec!["Plum".to_string()]);
        assert_eq!(s.search_field(), field, "typing rebuilt the search field");

        s.type_search("");
        assert_eq!(s.find_class("cn-combobox-item").len(), 3);
    })
}

#[test]
fn a_search_that_matches_nothing_can_be_used_as_the_value() {
    guarded(|| {
        let mut s = scene("cb_custom");
        s.click(s.trigger());
        s.type_search("kiwi");
        let list = s.list().unwrap();
        let texts = s.texts_under(list);
        assert!(texts.contains(&"No results found".to_string()), "{texts:?}");
        let custom = s.find_class("cn-combobox-item");
        assert_eq!(s.texts_under(custom[0]), vec!["Use \"kiwi\"".to_string()]);

        s.click(custom[0]);
        assert_eq!(s.value.get(), "kiwi");
        assert!(s.list().is_none(), "choosing did not close the list");
        assert_eq!(s.texts_under(s.trigger()), vec!["kiwi".to_string()]);
    })
}

#[test]
fn choosing_an_option_shows_it_in_the_trigger() {
    guarded(|| {
        let mut s = scene("cb_choose");
        assert_eq!(s.texts_under(s.trigger()), vec!["Pick a fruit".to_string()]);
        s.click(s.trigger());
        let rows = s.find_class("cn-combobox-item");
        s.click(rows[1]);
        assert_eq!(s.value.get(), "pear");
        assert!(s.list().is_none());
        assert_eq!(s.texts_under(s.trigger()), vec!["Pear".to_string()]);
    })
}
