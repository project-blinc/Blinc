//! The rich text editor is built once: an edit rebuilds only the block it
//! changed, and the selection, the cursor and the floating toolbar follow
//! in place, with no subtree rebuilt.

use std::sync::Arc;

use blinc_core::Color;
use blinc_core::events::event_types;
use blinc_layout::LayoutNodeId;
use blinc_layout::div::{Div, ElementBuilder};
use blinc_layout::event_handler::EventContext;
use blinc_layout::renderer::{ElementType, RenderTree};
use blinc_layout::widgets::rich_text_editor::editor::rich_text_editor;
use blinc_layout::widgets::rich_text_editor::{
    DocPosition, RichDocument, RichTextState, RichTextTheme, Selection, rich_text_state,
};
use std::sync::Mutex;

/// Focus statics, the pending-update queue and the reactive graph are
/// process-wide.
static LOCK: Mutex<()> = Mutex::new(());

fn init() {
    static I: std::sync::Once = std::sync::Once::new();
    I.call_once(|| {
        blinc_theme::ThemeState::init_default();
        if !blinc_core::BlincContextState::is_initialized() {
            blinc_core::BlincContextState::init(
                blinc_core::reactive::global_graph(),
                Arc::new(Mutex::new(blinc_core::context_state::HookState::new())),
                Arc::new(std::sync::atomic::AtomicBool::new(false)),
            );
        }
    });
}

struct Scene {
    tree: RenderTree,
    editor: Div,
    state: RichTextState,
}

fn scene() -> Scene {
    init();
    blinc_layout::widgets::text_input::blur_all_text_inputs();
    let _ = blinc_layout::stateful::take_pending_partial_prop_updates();
    let _ = blinc_layout::stateful::take_pending_subtree_rebuilds();
    let state = rich_text_state(RichDocument::from_markdown(
        "first paragraph\n\nsecond paragraph\n\nthird paragraph",
        Color::WHITE,
    ));
    let editor = rich_text_editor(&state, RichTextTheme::dark(), 400.0);
    let mut tree = RenderTree::from_element(&editor);
    tree.compute_layout(500.0, 400.0);
    Scene {
        tree,
        editor,
        state,
    }
}

impl Scene {
    fn root(&self) -> LayoutNodeId {
        self.tree.root().unwrap()
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

    fn frame(&mut self) {
        assert!(
            !blinc_layout::stateful::has_pending_subtree_rebuilds(),
            "the editor queued a subtree rebuild"
        );
        let updates = blinc_layout::stateful::take_pending_partial_prop_updates();
        self.tree.apply_partial_property_updates(updates);
        self.tree.compute_layout(500.0, 400.0);
    }

    fn dispatch(&mut self, ctx: EventContext) {
        ElementBuilder::event_handlers(&self.editor)
            .unwrap()
            .dispatch(&ctx);
        self.frame();
    }

    /// Press at `(x, y)` in the editor's own coordinates.
    fn click(&mut self, x: f32, y: f32) {
        let ctx = EventContext::new(event_types::POINTER_DOWN, self.root())
            .with_bounds(400.0, 300.0)
            .with_mouse_pos(x, y)
            .with_local_pos(x, y);
        self.dispatch(ctx);
    }

    fn type_text(&mut self, text: &str) {
        for c in text.chars() {
            let ctx = EventContext::new(event_types::TEXT_INPUT, self.root()).with_key_char(c);
            self.dispatch(ctx);
        }
    }

    /// The text on the page, in tree order. Prose is a styled text.
    fn texts(&self) -> String {
        self.nodes()
            .into_iter()
            .filter_map(
                |n| match self.tree.get_render_node(n).map(|r| &r.element_type) {
                    Some(ElementType::Text(t)) => Some(t.content.clone()),
                    Some(ElementType::StyledText(t)) => Some(t.content.clone()),
                    _ => None,
                },
            )
            .collect::<Vec<_>>()
            .join("|")
    }

    /// The nodes that make up the document's blocks: the root's first
    /// child's children.
    fn blocks(&self) -> Vec<LayoutNodeId> {
        let content = self.tree.layout_tree.children(self.root())[0];
        let document = self.tree.layout_tree.children(content)[0];
        self.tree.layout_tree.children(document)
    }

    /// The y of line `n`, where a click lands on it.
    fn line_y(&self, n: usize) -> f32 {
        let d = self.state.lock().unwrap();
        d.line_index[n].y + d.line_index[n].height / 2.0
    }

    fn bump(&self) {
        let key = format!("rich_text_editor:{:p}", Arc::as_ptr(&self.state));
        let version = blinc_core::BlincContextState::get().use_state_keyed(&key, || 0u32);
        version.set(version.get().wrapping_add(1));
    }

    /// Rectangles filled with the selection's highlight colour.
    fn highlights(&self) -> usize {
        let theme = RichTextTheme::dark();
        let highlight = Color::rgba(theme.text.r, theme.text.g, theme.text.b, 0.18);
        self.nodes()
            .into_iter()
            .filter(|&n| {
                matches!(
                    &self.tree.get_render_node(n).unwrap().props.background,
                    Some(blinc_core::Brush::Solid(c)) if *c == highlight
                )
            })
            .count()
    }

    /// The rectangles the cursor paints this frame.
    fn cursor_rects(&self) -> usize {
        let canvas = self
            .nodes()
            .into_iter()
            .find(|&n| {
                matches!(
                    self.tree.get_render_node(n).map(|r| &r.element_type),
                    Some(ElementType::Canvas(_))
                )
            })
            .expect("the editor has no cursor");
        let Some(ElementType::Canvas(c)) =
            self.tree.get_render_node(canvas).map(|r| &r.element_type)
        else {
            unreachable!()
        };
        let paint = c.render_fn.clone().expect("the cursor has no paint");
        let b = self.tree.get_absolute_bounds(canvas).unwrap();
        let mut ctx = blinc_core::RecordingContext::new(blinc_core::Size::new(500.0, 400.0));
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
}

#[test]
fn an_edit_rebuilds_only_the_block_it_changed() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut s = scene();
    assert_eq!(s.blocks().len(), 3);
    let before = s.blocks();

    // A click at the end of the second paragraph, then a typed character.
    let y = s.line_y(1);
    s.click(1000.0, y);
    s.type_text("!");
    assert!(
        s.texts().contains("second paragraph!"),
        "the edit is not shown: {}",
        s.texts()
    );
    let after = s.blocks();
    assert_eq!(after.len(), 3);
    assert_eq!(after[0], before[0], "the first block was rebuilt");
    assert_ne!(after[1], before[1], "the edited block was not rebuilt");
    assert_eq!(after[2], before[2], "the last block was rebuilt");
}

#[test]
fn a_new_paragraph_adds_a_block_in_place() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut s = scene();
    let y = s.line_y(0);
    s.click(1000.0, y);
    let enter = EventContext::new(event_types::KEY_DOWN, s.root()).with_key_code(13);
    s.dispatch(enter);
    assert_eq!(s.blocks().len(), 4, "Enter did not add a block");
}

#[test]
fn the_selection_and_toolbar_follow_in_place() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut s = scene();
    let y = s.line_y(0);
    s.click(5.0, y);
    assert_eq!(s.highlights(), 0);
    assert!(!s.texts().contains("B|"), "a toolbar with no selection");

    {
        let mut d = s.state.lock().unwrap();
        let anchor = DocPosition::new(0, 0, 0);
        let head = DocPosition::new(0, 0, 5);
        d.cursor = head;
        d.selection = Some(Selection { anchor, head });
    }
    s.bump();
    s.frame();
    assert!(s.highlights() > 0, "the selection is not highlighted");
    assert!(
        s.texts().contains('B'),
        "the toolbar did not appear: {}",
        s.texts()
    );

    s.state.lock().unwrap().selection = None;
    s.bump();
    s.frame();
    assert_eq!(s.highlights(), 0, "the highlight stayed");
}

#[test]
fn the_cursor_shows_while_focused_and_not_after() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut s = scene();
    assert_eq!(s.cursor_rects(), 0, "an unfocused editor draws a cursor");
    let y = s.line_y(0);
    s.click(5.0, y);
    assert_eq!(s.cursor_rects(), 1, "the focused editor draws no cursor");
    blinc_layout::widgets::text_input::blur_all_text_inputs();
    s.frame();
    assert_eq!(s.cursor_rects(), 0, "the cursor stayed after blur");
}
