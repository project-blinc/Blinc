//! `cn.Textarea` — multi-line text field.

use std::cell::OnceCell;

use blinc_dsl_core::extern_widget;
use blinc_layout::div::ElementBuilder;

use crate::bridge::text_area_state_keyed;

/// `cn.Textarea(key, placeholder?, label?, description?, error?, rows?,
/// size?, disabled?, required?, max_length?)` — a multi-line field.
///
/// Props (DSL surface):
/// - `key: string` — identity for the typed text, exactly as
///   `cn.Input`. Omitting it means the text does not survive a rebuild.
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
    /// Lazy-constructed cn widget. Same caching rationale as
    /// `CnButton::built`.
    #[skip]
    built: OnceCell<blinc_cn::Textarea>,
}

impl CnTextarea {
    fn get_or_build(&self) -> &blinc_cn::Textarea {
        self.built.get_or_init(|| self.to_cn_widget())
    }

    fn to_cn_widget(&self) -> blinc_cn::Textarea {
        let size = match self.size.as_str() {
            "small" | "sm" => blinc_cn::TextareaSize::Small,
            "large" | "lg" => blinc_cn::TextareaSize::Large,
            _ => blinc_cn::TextareaSize::Medium,
        };

        let state = text_area_state_keyed(&self.key);
        let mut t = blinc_cn::textarea(&state).size(size);
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
}
