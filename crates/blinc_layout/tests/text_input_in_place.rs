//! A text field is built once: typing, focusing, hovering, scrolling and
//! selecting patch what it shows in place, with no subtree rebuilt.

use blinc_core::Color;
use blinc_core::events::event_types;
use blinc_layout::LayoutNodeId;
use blinc_layout::event_handler::EventContext;
use blinc_layout::renderer::{ElementType, RenderTree};
use blinc_layout::widgets::text_input::{
    SharedTextInputData, refresh_text_input, text_input, text_input_data,
};
use std::sync::Mutex;

/// The pending-write queue, focus statics and the reactive graph are
/// process-wide.
static LOCK: Mutex<()> = Mutex::new(());

const IDLE: Color = Color::rgba(0.1, 0.1, 0.1, 1.0);
const HOVER: Color = Color::rgba(0.2, 0.2, 0.2, 1.0);
const FOCUSED_BORDER: Color = Color::rgba(0.0, 0.5, 1.0, 1.0);

struct Scene {
    tree: RenderTree,
    input: blinc_layout::widgets::text_input::TextInput,
    data: SharedTextInputData,
}

fn scene(width: f32) -> Scene {
    blinc_theme::ThemeState::init_default();
    blinc_layout::widgets::text_input::blur_all_text_inputs();
    let _ = blinc_layout::stateful::take_pending_partial_prop_updates();
    let _ = blinc_layout::stateful::take_pending_subtree_rebuilds();
    let data = text_input_data();
    let input = text_input(&data)
        .w(width)
        .idle_bg_color(IDLE)
        .hover_bg_color(HOVER)
        .focused_border_color(FOCUSED_BORDER);
    let mut tree = RenderTree::from_element(&input);
    tree.compute_layout(400.0, 100.0);
    Scene { tree, input, data }
}

impl Scene {
    fn root(&self) -> LayoutNodeId {
        self.tree.root().unwrap()
    }

    fn dispatch(&mut self, ctx: EventContext) {
        blinc_layout::div::ElementBuilder::event_handlers(&self.input)
            .unwrap()
            .dispatch(&ctx);
        self.frame();
    }

    fn frame(&mut self) {
        assert!(
            blinc_layout::stateful::take_pending_subtree_rebuilds().is_empty(),
            "the field queued a subtree rebuild"
        );
        let updates = blinc_layout::stateful::take_pending_partial_prop_updates();
        self.tree.apply_partial_property_updates(updates);
        self.tree.compute_layout(400.0, 100.0);
    }

    fn click(&mut self) {
        let ctx = EventContext::new(event_types::POINTER_DOWN, self.root())
            .with_bounds(200.0, 36.0)
            .with_mouse_pos(10.0, 10.0);
        self.dispatch(ctx);
    }

    fn type_text(&mut self, text: &str) {
        for c in text.chars() {
            let ctx = EventContext::new(event_types::TEXT_INPUT, self.root()).with_key_char(c);
            self.dispatch(ctx);
        }
    }

    /// The text the field shows, run by run.
    fn runs(&self) -> Vec<String> {
        let mut runs = Vec::new();
        let mut stack = vec![self.root()];
        while let Some(node) = stack.pop() {
            if let Some(ElementType::Text(t)) =
                self.tree.get_render_node(node).map(|r| &r.element_type)
            {
                runs.push(t.content.clone());
            }
            let mut children = self.tree.layout_tree.children(node);
            children.reverse();
            stack.extend(children);
        }
        runs
    }

    fn shown(&self) -> String {
        self.runs().concat()
    }

    fn fill(&self) -> Option<Color> {
        match &self
            .tree
            .get_render_node(self.root())
            .unwrap()
            .props
            .background
        {
            Some(blinc_core::Brush::Solid(c)) => Some(*c),
            _ => None,
        }
    }

    fn border(&self) -> Option<Color> {
        self.tree
            .get_render_node(self.root())
            .unwrap()
            .props
            .border_color
    }

    /// How far the text has been scrolled left: the translation of the node
    /// holding the runs.
    fn scrolled(&self) -> f32 {
        let mut stack = vec![self.root()];
        while let Some(node) = stack.pop() {
            if let Some(blinc_core::Transform::Affine2D(a)) = self
                .tree
                .get_render_node(node)
                .and_then(|r| r.props.transform.clone())
            {
                let kids = self.tree.layout_tree.children(node);
                let holds_text = kids
                    .iter()
                    .any(|&k| !self.tree.layout_tree.children(k).is_empty());
                if holds_text {
                    return -a.elements[4];
                }
            }
            stack.extend(self.tree.layout_tree.children(node));
        }
        0.0
    }
}

#[test]
fn typing_patches_the_shown_text_in_place() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut s = scene(300.0);
    s.click();
    s.type_text("hello");
    assert_eq!(s.data.lock().unwrap().value, "hello");
    assert_eq!(s.shown(), "hello");
}

#[test]
fn focus_and_hover_restyle_the_box_in_place() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut s = scene(300.0);
    assert_eq!(s.fill(), Some(IDLE));
    let ctx = EventContext::new(event_types::POINTER_ENTER, s.root());
    s.dispatch(ctx);
    assert_eq!(s.fill(), Some(HOVER), "hovering did not change the fill");
    let ctx = EventContext::new(event_types::POINTER_LEAVE, s.root());
    s.dispatch(ctx);
    assert_eq!(s.fill(), Some(IDLE));

    s.click();
    assert_eq!(
        s.border(),
        Some(FOCUSED_BORDER),
        "focusing did not change the border"
    );
}

#[test]
fn a_long_value_scrolls_to_keep_the_caret_in_view() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut s = scene(80.0);
    s.click();
    assert_eq!(s.scrolled(), 0.0);
    s.type_text("a value far wider than the field");
    assert!(
        s.scrolled() > 20.0,
        "the text scrolled {} for a caret past the field's edge",
        s.scrolled()
    );
}

#[test]
fn a_selection_splits_the_text_into_runs() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut s = scene(300.0);
    s.click();
    s.type_text("hello world");
    {
        let mut d = s.data.lock().unwrap();
        d.selection_start = Some(0);
        d.cursor = 5;
    }
    refresh_text_input(&s.data);
    s.frame();
    let runs: Vec<String> = s.runs().into_iter().filter(|r| !r.is_empty()).collect();
    assert_eq!(
        runs,
        ["hello", " world"],
        "the selection did not split the text"
    );
    assert_eq!(s.shown(), "hello world");
}
