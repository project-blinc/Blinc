//! The code editor with a floating find and replace bar.
//!
//! [`code_editor`] is `blinc_layout`'s editor over the same [`SharedCodeEditorState`], with cn's
//! own find bar floating at its top right in place of the built-in one.
//!
//! ```ignore
//! let state = code_editor_state("fn main() {}");
//! cn::code_editor(&state)
//!     .syntax(SyntaxConfig::new(RustHighlighter::new()))
//!     .line_numbers(true)
//!     .h(360.0)
//! ```
//!
//! # Keys
//!
//! | Keys | What they do |
//! |---|---|
//! | Cmd or Ctrl+F | Open the bar, starting from a one-line selection |
//! | Cmd+Option+F or Ctrl+H | Open it with the replace row |
//! | Enter, Shift+Enter | Next and previous match, from the find field |
//! | Cmd or Ctrl+G, F3 | Next match, previous with Shift |
//! | Enter, Cmd or Ctrl+Enter | Replace one, replace all, from the replace field |
//! | Alt+C, Alt+W, Alt+R | Match case, whole word, regular expression |
//! | Esc | Close the bar and return to the editor |

mod find;

use crate::components::names::{Names, name_methods};
use std::cell::{OnceCell, RefCell};
use std::sync::Arc;

use blinc_layout::div::{Div, ElementBuilder, ElementTypeId, div};
use blinc_layout::event_handler::EventContext;
use blinc_layout::syntax::SyntaxConfig;
use blinc_layout::widgets::code::{CodeEditor, SharedCodeEditorState};

use find::FindUi;

/// The space between the find bar and the editor's edges.
const INSET: f32 = 8.0;

/// A cn code editor, built from [`code_editor`].
pub struct CodeEditorBuilder {
    state: SharedCodeEditorState,
    editor: RefCell<Option<CodeEditor>>,
    find: bool,
    width: Option<f32>,
    height: Option<f32>,
    full_width: bool,
    built: OnceCell<Div>,
    names: Names,
}

/// A code editor over `state`, with a floating find and replace bar.
#[track_caller]
pub fn code_editor(state: &SharedCodeEditorState) -> CodeEditorBuilder {
    CodeEditorBuilder {
        state: Arc::clone(state),
        editor: RefCell::new(Some(blinc_layout::widgets::code::code_editor(state))),
        find: true,
        width: None,
        height: None,
        full_width: false,
        built: OnceCell::new(),
        names: Names::default(),
    }
}

impl CodeEditorBuilder {
    /// Change the layout editor this wraps.
    fn editor(self, change: impl FnOnce(CodeEditor) -> CodeEditor) -> Self {
        let editor = self.editor.borrow_mut().take();
        *self.editor.borrow_mut() = editor.map(change);
        self
    }

    /// Whether there is a find and replace bar, which there is unless set.
    pub fn find(mut self, find: bool) -> Self {
        self.find = find;
        self
    }

    pub fn line_numbers(self, shown: bool) -> Self {
        self.editor(|e| e.line_numbers(shown))
    }

    pub fn syntax(self, config: SyntaxConfig) -> Self {
        self.editor(|e| e.syntax(config))
    }

    pub fn font_size(self, size: f32) -> Self {
        self.editor(|e| e.font_size(size))
    }

    pub fn line_height(self, multiplier: f32) -> Self {
        self.editor(|e| e.line_height(multiplier))
    }

    pub fn padding(self, padding: f32) -> Self {
        self.editor(|e| e.padding(padding))
    }

    pub fn gutter_width(self, width: f32) -> Self {
        self.editor(|e| e.gutter_width(width))
    }

    pub fn minimap(self, shown: bool) -> Self {
        self.editor(|e| e.minimap(shown))
    }

    pub fn indent_guides(self, shown: bool) -> Self {
        self.editor(|e| e.indent_guides(shown))
    }

    pub fn code_folding(self, enabled: bool) -> Self {
        self.editor(|e| e.code_folding(enabled))
    }

    /// Let the text be read, selected and copied but not changed.
    pub fn read_only(self, read_only: bool) -> Self {
        self.editor(|e| e.read_only(read_only))
    }

    /// How many spaces Tab and Shift+Tab indent and dedent by.
    pub fn tab_size(self, columns: usize) -> Self {
        self.editor(|e| e.tab_size(columns))
    }

    /// Text shown, dimmed, while there is none.
    pub fn placeholder(self, text: impl Into<String>) -> Self {
        let text = text.into();
        self.editor(|e| e.placeholder(text))
    }

    /// Call `callback` with the text after each change the editor or the find bar makes.
    pub fn on_change(self, callback: impl Fn(&str) + Send + Sync + 'static) -> Self {
        self.editor(|e| e.on_change(callback))
    }

    /// The editor's width.
    pub fn w(mut self, width: f32) -> Self {
        self.width = Some(width);
        self.editor(|e| e.w(width))
    }

    /// Fill the parent's width.
    pub fn w_full(mut self) -> Self {
        self.full_width = true;
        self.editor(|e| e.w_full())
    }

    /// The editor's height, which it scrolls within.
    pub fn h(mut self, height: f32) -> Self {
        self.height = Some(height);
        self.editor(|e| e.h(height))
    }

    /// Build the editor and its bar.
    pub fn build_component(self) -> Div {
        self.get_or_build();
        self.built.into_inner().unwrap_or_else(div)
    }

    fn get_or_build(&self) -> &Div {
        ::blinc_layout::build_once::build_once(&self.built, || {
            let mut root = self
                .names
                .apply(div().class("cn-code-editor").relative().flex_col());
            if let Some(width) = self.width {
                root = root.w(width);
            }
            if self.full_width {
                root = root.w_full();
            }
            if let Some(height) = self.height {
                root = root.h(height);
            }
            if let Some(editor) = self.editor.borrow_mut().take() {
                root = root.child(editor);
            }
            if !self.find {
                return root;
            }
            // The minimap keeps the editor's right edge; the bar stays left of it.
            let minimap = self.state.lock().map_or(0.0, |d| {
                if d.config.minimap {
                    d.config.minimap_width
                } else {
                    0.0
                }
            });
            let ui = FindUi::new(&self.state);
            let state = Arc::clone(&self.state);
            let anchor = div()
                .class("cn-code-editor-find-anchor")
                .absolute()
                .top(INSET)
                .left(INSET)
                .right(INSET + minimap)
                .flex_row()
                .justify_end()
                // On the foreground layer, so the bar covers the code under it.
                .foreground()
                // Only the bar takes presses; the text beside it stays reachable.
                .pointer_events_none()
                .show(ui.open, move || find::bar(&state, ui));
            let state = Arc::clone(&self.state);
            root.child(anchor)
                .on_key_down(move |ctx| on_key(&state, ui, ctx))
        })
    }

    name_methods!(names);
}

/// The find bar's keys, while the editor or one of the bar's fields has the keyboard.
fn on_key(state: &SharedCodeEditorState, ui: FindUi, ctx: &EventContext) {
    let (editor, query, replacement) = {
        let Ok(d) = state.lock() else {
            return;
        };
        let focused = |field: &blinc_layout::widgets::text_input::SharedTextInputData| {
            field.lock().is_ok_and(|f| f.visual.is_focused())
        };
        (
            d.focused,
            focused(&d.search_input_state),
            focused(&d.replace_input_state),
        )
    };
    if !(editor || query || replacement) {
        return;
    }
    let command = ctx.command_modifier();
    let open = ui.open.get();
    match ctx.key_code {
        // F, with Option for the replace row, and H
        70 if command => ui.open(state, ctx.alt),
        72 if command => ui.open(state, true),
        // G and F3
        71 if command => ui.step(state, !ctx.shift),
        114 => ui.step(state, !ctx.shift),
        27 if open => ui.close(state),
        13 if open && query => ui.step(state, !ctx.shift),
        13 if open && replacement && command => ui.replace_all(state),
        13 if open && replacement => ui.replace_one(state),
        // Alt+C, W and R, whose characters are not typed
        67 | 87 | 82 if open && ctx.alt && !command => {
            ctx.prevent_default();
            ui.flip(state, ctx.key_code);
        }
        _ => {}
    }
}

impl ElementBuilder for CodeEditorBuilder {
    fn build(&self, tree: &mut blinc_layout::tree::LayoutTree) -> blinc_layout::tree::LayoutNodeId {
        self.get_or_build().build(tree)
    }

    fn render_props(&self) -> blinc_layout::element::RenderProps {
        self.get_or_build().render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.get_or_build().children_builders()
    }

    fn element_type_id(&self) -> ElementTypeId {
        self.get_or_build().element_type_id()
    }

    fn semantic_type_name(&self) -> Option<&'static str> {
        Some("code_editor")
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        self.get_or_build().layout_style()
    }

    fn event_handlers(&self) -> Option<&blinc_layout::event_handler::EventHandlers> {
        ElementBuilder::event_handlers(self.get_or_build())
    }

    fn element_id(&self) -> Option<&str> {
        self.get_or_build().element_id()
    }

    fn element_classes(&self) -> &[Arc<str>] {
        self.get_or_build().element_classes()
    }
}
