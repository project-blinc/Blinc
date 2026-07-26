//! `cn.Input` — single-line text field.

use std::cell::OnceCell;

use blinc_dsl_core::extern_widget;
use blinc_layout::div::ElementBuilder;

use crate::bridge::text_input_data_keyed;

/// `cn.Input(key, placeholder?, label?, description?, error?, kind?,
/// size?, disabled?, required?, width?)` — a single-line text field.
///
/// Props (DSL surface):
/// - `key: string` — identity for the typed text. `cn::input` keeps its
///   contents in external state that must outlive a rebuild, and extern
///   widgets have no call-site identity to derive one from, so the key
///   comes from you. Two fields sharing a key share their contents.
///   Omitting it means the text does not survive a rebuild.
/// - `placeholder: string` — shown while empty.
/// - `label: string` / `description: string` / `error: string` — the
///   surrounding form furniture. A non-empty `error` also switches the
///   field to its error styling.
/// - `kind: string` — `"text"` (default), `"password"`, `"email"`,
///   `"number"`, `"integer"`, `"url"`. Unknown values fall back to text.
/// - `size: string` — `"small"` / `"sm"`, `"medium"` / `"md"`
///   (default), `"large"` / `"lg"`.
/// - `disabled: bool`, `required: bool`.
/// - `width: f64` — fixed width in px; `0` (the default) leaves the
///   field to its own sizing.
///
/// Colour props are omitted: those belong in CSS via `.cn-input`.
#[extern_widget(namespace = "cn", name = "Input")]
pub struct CnInput {
    pub key: String,
    pub placeholder: String,
    pub label: String,
    pub description: String,
    pub error: String,
    pub kind: String,
    pub size: String,
    pub disabled: bool,
    pub required: bool,
    pub width: f64,
    /// Lazy-constructed cn widget. Same caching rationale as
    /// `CnButton::built`.
    #[skip]
    built: OnceCell<blinc_cn::Input>,
}

impl CnInput {
    fn get_or_build(&self) -> &blinc_cn::Input {
        self.built.get_or_init(|| self.to_cn_widget())
    }

    fn to_cn_widget(&self) -> blinc_cn::Input {
        use blinc_layout::widgets::text_input::InputType;

        let size = match self.size.as_str() {
            "small" | "sm" => blinc_cn::InputSize::Small,
            "large" | "lg" => blinc_cn::InputSize::Large,
            _ => blinc_cn::InputSize::Medium,
        };
        let kind = match self.kind.as_str() {
            "password" => InputType::Password,
            "email" => InputType::Email,
            "number" => InputType::Number,
            "integer" => InputType::Integer,
            "url" => InputType::Url,
            _ => InputType::Text,
        };

        let data = text_input_data_keyed(&self.key);
        let mut i = blinc_cn::input(&data).size(size).input_type(kind);
        if !self.placeholder.is_empty() {
            i = i.placeholder(self.placeholder.clone());
        }
        if !self.label.is_empty() {
            i = i.label(self.label.clone());
        }
        if !self.description.is_empty() {
            i = i.description(self.description.clone());
        }
        if !self.error.is_empty() {
            i = i.error(self.error.clone());
        }
        if self.disabled {
            i = i.disabled(true);
        }
        if self.required {
            i = i.required();
        }
        if self.width > 0.0 {
            i = i.w(self.width as f32);
        }
        i.build_component()
    }
}

impl ElementBuilder for CnInput {
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
    // `event_handlers` carries the keyboard/focus surface; without it
    // the field cannot be typed into.
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
