//! `cn.Textarea` — multi-line text field.

use std::cell::OnceCell;

use blinc_dsl_core::{Reactive, extern_widget};
use blinc_layout::div::ElementBuilder;

use crate::bridge::{CallSiteId, text_area_state_for_field, writable_signal};

/// `cn.Textarea(key, placeholder?, label?, description?, error?, rows?,
/// size?, disabled?, required?, max_length?)` — a multi-line field.
///
/// Props (DSL surface):
/// - `value: Reactive<String>` — bind a `signal` here and the field
///   shares it, exactly as `cn.Input`: the signal seeds the text and
///   every edit writes back. A bound field needs no `key`.
/// - `on_change: || => unit` — DSL closure fired after each edit, after
///   the write, so it can read the new text off the binding.
/// - `key: string` — identity for the typed text for an UNBOUND field.
///   Omitting it means the text does not survive a rebuild.
/// - `placeholder: string` — shown while empty.
/// - `label: string` / `description: string` / `error: string` — form
///   furniture; a non-empty `error` also switches to error styling.
/// - `rows: i64` — visible line count; `0` (default) leaves the
///   widget's own.
/// - `size: string` — `"small"` / `"sm"`, `"medium"` / `"md"`
///   (default), `"large"` / `"lg"`.
/// - `disabled: bool`, `required: bool`.
/// - `max_length: i64` — character cap; `0` (default) means unlimited.
///
/// Colour props are omitted: those belong in CSS via `.cn-textarea`.
#[extern_widget(namespace = "cn", name = "Textarea")]
pub struct CnTextarea {
    pub value: Reactive<String>,
    pub key: String,
    pub placeholder: String,
    pub label: String,
    pub description: String,
    pub error: String,
    pub rows: i64,
    pub size: String,
    pub disabled: bool,
    pub required: bool,
    pub max_length: i64,
    /// Zero when the user omitted `on_change`. Same zero-arg
    /// `extern "C" fn()` pointer convention as `cn.Button`'s `on_click`.
    pub on_change: i64,
    /// Call-site identity, captured while the FFI builds the struct.
    #[skip]
    call_site: CallSiteId,
    /// Lazy-constructed cn widget. Same caching rationale as
    /// `CnButton::built`.
    #[skip]
    built: OnceCell<blinc_cn::Textarea>,
}

impl CnTextarea {
    fn get_or_build(&self) -> &blinc_cn::Textarea {
        ::blinc_layout::build_once::build_once(&self.built, || self.to_cn_widget())
    }

    fn to_cn_widget(&self) -> blinc_cn::Textarea {
        let size = match self.size.as_str() {
            "small" | "sm" => blinc_cn::TextareaSize::Small,
            "large" | "lg" => blinc_cn::TextareaSize::Large,
            _ => blinc_cn::TextareaSize::Medium,
        };

        let state = text_area_state_for_field(&self.value, &self.key, self.call_site);
        let mut t = blinc_cn::textarea(&state).size(size);
        // Signal first, then the author's closure, so a zero-arg
        // closure reads the text that was just written.
        let bound = writable_signal(&self.value);
        let on_change_ptr = self.on_change;
        if bound.is_some() || on_change_ptr != 0 {
            t = t.on_change(move |new_value: &str| {
                if let Some(sig) = bound {
                    sig.set(new_value.to_string());
                }
                if on_change_ptr != 0 {
                    type ClosureFn = extern "C" fn();
                    let func: ClosureFn = unsafe { std::mem::transmute(on_change_ptr) };
                    func();
                }
            });
        }
        if !self.placeholder.is_empty() {
            t = t.placeholder(self.placeholder.clone());
        }
        if !self.label.is_empty() {
            t = t.label(self.label.clone());
        }
        if !self.description.is_empty() {
            t = t.description(self.description.clone());
        }
        if !self.error.is_empty() {
            t = t.error(self.error.clone());
        }
        if self.rows > 0 {
            t = t.rows(self.rows as usize);
        }
        if self.max_length > 0 {
            t = t.max_length(self.max_length as usize);
        }
        if self.disabled {
            t = t.disabled(true);
        }
        if self.required {
            t = t.required();
        }
        t.build_component()
    }
}

impl ElementBuilder for CnTextarea {
    fn build(&self, tree: &mut blinc_layout::LayoutTree) -> blinc_layout::LayoutNodeId {
        self.get_or_build().build(tree)
    }

    fn render_props(&self) -> blinc_layout::RenderProps {
        self.get_or_build().render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.get_or_build().children_builders()
    }

    // MUST forward — see `gotcha_element_builder_trait_forwarding`.
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

    // Intrinsic size lives here: a textarea's rows-derived height and an
    // input's height are set on the taffy style, so a wrapper that does
    // not forward it hides the size from every builder-tree reader.
    fn layout_style(&self) -> Option<&taffy::Style> {
        self.get_or_build().layout_style()
    }
}
