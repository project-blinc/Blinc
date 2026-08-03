//! Core widgets declared through `#[extern_widget]`.
//!
//! The older core widgets are three coupled declarations — a
//! `ComponentDefinition` in `runtime_bridge`, a `BuiltinDescriptor` in
//! `abi`, and a `blinc_*_view` in `widget_ffi` — which must agree on
//! arity and order. Adding a prop means editing all three, and getting
//! two of them right silently breaks every call site of that widget.
//!
//! `#[extern_widget]` derives all of it from one struct, the same way
//! the `cn.*` widgets are declared, and gives reactive props for free.
//! Widgets converted here register in `BlincDsl::new` so they stay
//! available with no `register_*` call, exactly as before.

use crate::{Reactive, extern_widget};
use blinc_layout::div::ElementBuilder;
use std::cell::OnceCell;

/// `RichText("plain <b>bold</b> and <a href=\"…\">a link</a>")` — one
/// text node with inline formatting.
///
/// ```dsl,ignore
/// signal note: string = "status: <b>ready</b>"
///
/// RichText("Press <b>Enter</b> or <i>Escape</i>")
/// RichText(note, size = 18.0, align = "center")
/// ```
///
/// Tags: `<b>`/`<strong>`, `<i>`/`<em>`, `<u>`, `<s>`/`<strike>`/`<del>`,
/// `<a href="…">`, `<span color="…">`. Entities decode, and a link is
/// clickable.
///
/// One node, not a row of them: formatting mid-sentence survives a line
/// break, which separate elements on a baseline cannot manage.
#[extern_widget(name = "RichText", styled)]
pub struct RichTextWidget {
    /// The markup. Bind a signal and the text follows it.
    pub markup: Reactive<String>,
    /// Font size in pixels. Omitted keeps the layout default.
    pub size: f64,
    /// Line height multiplier. Omitted keeps the layout default.
    pub line_height: f64,
    /// `left` (default) / `center` / `right`.
    pub align: String,
    /// Built once. Boxed because bound markup is a subscribed wrapper
    /// while a literal is a bare rich-text node.
    #[skip]
    built: OnceCell<Box<dyn ElementBuilder>>,
}

impl RichTextWidget {
    fn get_or_build(&self) -> &Box<dyn ElementBuilder> {
        blinc_layout::build_once::build_once(&self.built, || {
            let size = self.size;
            let line_height = self.line_height;
            let align = self.align();
            reactive_string_node(&self.markup, move |s| {
                let mut t = blinc_layout::rich_text(s);
                if size > 0.0 {
                    t = t.size(size as f32);
                }
                if line_height > 0.0 {
                    t = t.line_height(line_height as f32);
                }
                if let Some(align) = align {
                    t = t.align(align);
                }
                Box::new(t)
            })
        })
    }

    fn align(&self) -> Option<blinc_layout::div::TextAlign> {
        use blinc_layout::div::TextAlign as A;
        match self.align.as_str() {
            "" => None,
            "left" => Some(A::Left),
            "center" => Some(A::Center),
            "right" => Some(A::Right),
            other => {
                tracing::warn!(align = %other, "RichText: unknown align");
                None
            }
        }
    }
}

/// `Markdown(source)` — a block of markdown, rendered to elements.
///
/// ```dsl,ignore
/// Markdown("## Heading\n\nSome **bold** text:\n- one\n- two")
/// ```
///
/// Unlike [`RichTextWidget`], which is one inline node, this produces a
/// column: headings, paragraphs, lists and code blocks each become
/// their own element.
#[extern_widget(name = "Markdown", styled)]
pub struct MarkdownWidget {
    /// The markdown source. Bind a signal and the block follows it.
    pub source: Reactive<String>,
    /// Built once, same rationale as [`RichTextWidget`].
    #[skip]
    built: OnceCell<Box<dyn ElementBuilder>>,
}

impl MarkdownWidget {
    fn get_or_build(&self) -> &Box<dyn ElementBuilder> {
        blinc_layout::build_once::build_once(&self.built, || {
            reactive_string_node(&self.source, |s| {
                Box::new(blinc_layout::markdown::markdown(&s))
            })
        })
    }
}

/// An element that follows a reactive string, or is built once when the
/// source is a literal.
///
/// The same shape `blinc_cn`'s `reactive_props::reactive_node` has, but
/// this crate cannot depend on `blinc_cn` — it sits below it.
fn reactive_string_node<F>(src: &Reactive<String>, build: F) -> Box<dyn ElementBuilder>
where
    F: Fn(String) -> Box<dyn ElementBuilder> + Send + Sync + 'static,
{
    // `NoState`, not `ButtonState`: this subscribes to a signal, it
    // does not react to the pointer. A `ButtonState` FSM transitions on
    // hover and press, and every transition would rebuild the text for
    // no reason — the same needless rebuild storm `cn::radio` had.
    use blinc_layout::stateful::{NoState, stateful};

    let current = |r: &Reactive<String>| -> String {
        match r {
            Reactive::Literal(v) => v.clone(),
            Reactive::Signal(s) => s.try_get().unwrap_or_default(),
            Reactive::Computed(c) => c.try_get().unwrap_or_default(),
        }
    };

    let Reactive::Signal(sig) = src else {
        return build(current(src));
    };
    let sig = *sig;
    Box::new(
        stateful::<NoState>()
            .deps([sig.id()])
            .on_state(move |_ctx| {
                blinc_layout::div::div()
                    .w_full()
                    .h_fit()
                    .child_box(build(sig.try_get().unwrap_or_default()))
            })
            .w_full()
            .h_fit(),
    )
}

macro_rules! forward_text_element {
    ($ty:ident) => {
        impl ElementBuilder for $ty {
            fn build(&self, tree: &mut blinc_layout::LayoutTree) -> blinc_layout::LayoutNodeId {
                self.get_or_build().build(tree)
            }

            fn render_props(&self) -> blinc_layout::RenderProps {
                self.get_or_build().render_props()
            }

            fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
                self.get_or_build().children_builders()
            }

            // `styled_text_render_info` is how rich text reaches the
            // renderer at all — a wrapper leaving it at the `None`
            // default lays out a correctly-sized node that draws
            // nothing. `event_handlers` carries the link hit regions.
            fn text_render_info(&self) -> Option<blinc_layout::div::TextRenderInfo> {
                self.get_or_build().text_render_info()
            }

            fn styled_text_render_info(&self) -> Option<blinc_layout::div::StyledTextRenderInfo> {
                self.get_or_build().styled_text_render_info()
            }

            fn event_handlers(&self) -> Option<&blinc_layout::event_handler::EventHandlers> {
                self.get_or_build().event_handlers()
            }

            fn element_classes(&self) -> &[std::sync::Arc<str>] {
                self.get_or_build().element_classes()
            }

            fn element_id(&self) -> Option<&str> {
                self.get_or_build().element_id()
            }

            fn element_type_id(&self) -> blinc_layout::div::ElementTypeId {
                self.get_or_build().element_type_id()
            }
        }
    };
}

forward_text_element!(RichTextWidget);
forward_text_element!(MarkdownWidget);
