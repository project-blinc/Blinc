//! A text area is built once: typing, new lines, focusing, hovering and
//! selecting patch what it shows in place, with no subtree rebuilt, and its
//! caret shows while it is focused and follows the cursor.

use blinc_core::Color;
use blinc_core::events::event_types;
use blinc_layout::LayoutNodeId;
use blinc_layout::event_handler::EventContext;
use blinc_layout::renderer::{ElementType, RenderTree};
use blinc_layout::widgets::text_area::{
    SharedTextAreaState, TextAreaConfig, TextPosition, refresh_text_area, text_area,
    text_area_state, text_area_state_with_placeholder,
};
use std::sync::Mutex;

/// The pending-write queue, focus statics and the reactive graph are
/// process-wide.
static LOCK: Mutex<()> = Mutex::new(());

struct Scene {
    tree: RenderTree,
    area: blinc_layout::widgets::text_area::TextArea,
    state: SharedTextAreaState,
}

fn scene(state: SharedTextAreaState) -> Scene {
    blinc_theme::ThemeState::init_default();
    blinc_layout::widgets::text_input::blur_all_text_inputs();
    let _ = blinc_layout::stateful::take_pending_partial_prop_updates();
    let _ = blinc_layout::stateful::take_pending_subtree_rebuilds();
    let area = text_area(&state).w(300.0).rows(4);
    let mut tree = RenderTree::from_element(&area);
    tree.compute_layout(400.0, 300.0);
    Scene { tree, area, state }
}

impl Scene {
    fn root(&self) -> LayoutNodeId {
        self.tree.root().unwrap()
    }

    fn dispatch(&mut self, ctx: EventContext) {
        blinc_layout::div::ElementBuilder::event_handlers(&self.area)
            .unwrap()
            .dispatch(&ctx);
        self.frame();
    }

    fn frame(&mut self) {
        assert!(
            blinc_layout::stateful::take_pending_subtree_rebuilds().is_empty(),
            "the text area queued a subtree rebuild"
        );
        let updates = blinc_layout::stateful::take_pending_partial_prop_updates();
        self.tree.apply_partial_property_updates(updates);
        self.tree.compute_layout(400.0, 300.0);
    }

    fn click(&mut self) {
        let ctx = EventContext::new(event_types::POINTER_DOWN, self.root())
            .with_bounds(300.0, 100.0)
            .with_mouse_pos(14.0, 14.0);
        self.dispatch(ctx);
    }

    fn type_text(&mut self, text: &str) {
        for c in text.chars() {
            let ctx = if c == '\n' {
                EventContext::new(event_types::KEY_DOWN, self.root()).with_key_code(13)
            } else {
                EventContext::new(event_types::TEXT_INPUT, self.root()).with_key_char(c)
            };
            self.dispatch(ctx);
        }
    }

    fn nodes(&self) -> Vec<LayoutNodeId> {
        let mut found = Vec::new();
        let mut stack = vec![self.root()];
        while let Some(node) = stack.pop() {
            found.push(node);
            let mut children = self.tree.layout_tree.children(node);
            children.reverse();
            stack.extend(children);
        }
        found
    }

    /// The text each row shows, in order.
    fn rows(&self) -> Vec<String> {
        self.nodes()
            .into_iter()
            .filter_map(
                |n| match self.tree.get_render_node(n).map(|r| &r.element_type) {
                    Some(ElementType::Text(t)) => Some(t.content.clone()),
                    _ => None,
                },
            )
            .collect()
    }

    /// The width of each selection highlight, row by row.
    fn highlights(&self) -> Vec<f32> {
        let selection = TextAreaConfig::default().selection_color;
        self.nodes()
            .into_iter()
            .filter(|&n| {
                matches!(
                    &self.tree.get_render_node(n).unwrap().props.background,
                    Some(blinc_core::Brush::Solid(c)) if *c == selection
                )
            })
            .map(|n| self.tree.get_absolute_bounds(n).unwrap().width)
            .collect()
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

    fn caret(&self) -> LayoutNodeId {
        self.nodes()
            .into_iter()
            .find(|&n| {
                matches!(
                    self.tree.get_render_node(n).map(|r| &r.element_type),
                    Some(ElementType::Canvas(_))
                )
            })
            .expect("the text area has no caret")
    }

    /// The rectangles the caret paints this frame.
    fn caret_rects(&self) -> usize {
        let caret = self.caret();
        let Some(ElementType::Canvas(c)) =
            self.tree.get_render_node(caret).map(|r| &r.element_type)
        else {
            unreachable!()
        };
        let paint = c.render_fn.clone().expect("the caret has no paint");
        let b = self.tree.get_absolute_bounds(caret).unwrap();
        let mut ctx = blinc_core::RecordingContext::new(blinc_core::Size::new(400.0, 300.0));
        paint(
            &mut ctx,
            blinc_layout::CanvasBounds {
                x: 0.0,
                y: 0.0,
                width: b.width,
                height: b.height,
            },
        );
        ctx.commands()
            .iter()
            .filter(|c| matches!(c, blinc_core::DrawCommand::FillRect { .. }))
            .count()
    }

    /// How far down the caret has been moved.
    fn caret_y(&self) -> f32 {
        let holder = self
            .nodes()
            .into_iter()
            .find(|&n| self.tree.layout_tree.children(n).contains(&self.caret()))
            .unwrap();
        match self
            .tree
            .get_render_node(holder)
            .unwrap()
            .props
            .transform
            .clone()
        {
            Some(blinc_core::Transform::Affine2D(a)) => a.elements[5],
            _ => 0.0,
        }
    }
}

#[test]
fn typing_and_new_lines_patch_the_rows_in_place() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut s = scene(text_area_state());
    s.click();
    s.type_text("hello");
    assert_eq!(s.rows(), ["hello"]);
    s.type_text("\nworld");
    assert_eq!(s.state.lock().unwrap().value(), "hello\nworld");
    assert_eq!(s.rows(), ["hello", "world"], "the new line got no row");
}

#[test]
fn an_empty_field_shows_its_placeholder() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut s = scene(text_area_state_with_placeholder("Say something"));
    assert_eq!(s.rows(), ["Say something"]);
    s.click();
    s.type_text("hi");
    assert_eq!(s.rows(), ["hi"]);
}

#[test]
fn focus_and_hover_restyle_the_box_in_place() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut s = scene(text_area_state());
    let (rest_fill, rest_border) = (s.fill(), s.border());
    let ctx = EventContext::new(event_types::POINTER_ENTER, s.root());
    s.dispatch(ctx);
    assert_ne!(s.fill(), rest_fill, "hovering did not change the fill");
    let ctx = EventContext::new(event_types::POINTER_LEAVE, s.root());
    s.dispatch(ctx);
    assert_eq!(s.fill(), rest_fill);

    s.click();
    assert_ne!(
        s.border(),
        rest_border,
        "focusing did not change the border"
    );
}

#[test]
fn a_selection_highlights_each_line_it_covers() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut s = scene(text_area_state());
    s.click();
    s.type_text("hello\nworld");
    assert!(s.highlights().iter().all(|w| *w == 0.0));
    {
        let mut d = s.state.lock().unwrap();
        d.selection_start = Some(TextPosition::new(0, 2));
        d.cursor = TextPosition::new(1, 3);
    }
    refresh_text_area(&s.state);
    s.frame();
    let widths = s.highlights();
    assert_eq!(widths.len(), 2);
    assert!(
        widths.iter().all(|w| *w > 0.0),
        "a line in the selection is not highlighted: {widths:?}"
    );
}

#[test]
fn the_caret_shows_while_focused_and_follows_the_cursor() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut s = scene(text_area_state());
    assert_eq!(s.caret_rects(), 0, "an unfocused field draws a caret");
    s.click();
    assert_eq!(s.caret_rects(), 1, "the focused field draws no caret");
    let first_line = s.caret_y();
    s.type_text("one\ntwo");
    assert!(
        s.caret_y() > first_line + 5.0,
        "the caret stayed at {} on the second line",
        s.caret_y()
    );
}
