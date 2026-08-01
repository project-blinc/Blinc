//! `cn.Input` — single-line text field.

use std::cell::OnceCell;

use blinc_dsl_core::{Reactive, extern_widget};
use blinc_layout::div::ElementBuilder;

use crate::bridge::{CallSiteId, text_input_data_for_field, writable_signal};

/// `cn.Input(key, placeholder?, label?, description?, error?, kind?,
/// size?, disabled?, required?, width?)` — a single-line text field.
///
/// Props (DSL surface):
/// - `value: Reactive<String>` — bind a `signal` here and the field
///   shares it: what the user types is written back on every edit, and
///   the signal's value seeds the field. It also doubles as the
///   field's identity, so no `key` is needed for a bound field.
/// - `on_change: || => unit` — DSL closure fired after each edit.
///   Zero-arg, like `Div(on_click = …)`: read the new text from the
///   bound signal, which is written first.
/// - `key: string` — identity for the typed text, for an UNBOUND field. `cn::input` keeps its
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
    pub value: Reactive<String>,
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
    /// Zero when the user omitted `on_change`. Same zero-arg
    /// `extern "C" fn()` pointer convention as `cn.Button`'s `on_click`.
    pub on_change: i64,
    /// Handle from a `ref name: Input` declaration: `ref = email`.
    /// Zero when omitted.
    pub r#ref: i64,
    /// Call-site identity, captured while the FFI builds the struct --
    /// `current_call_id()` reads correctly only inside that bracket.
    #[skip]
    call_site: CallSiteId,
    /// Lazy-constructed cn widget. Same caching rationale as
    /// `CnButton::built`.
    #[skip]
    built: OnceCell<blinc_cn::Input>,
}

impl CnInput {
    fn get_or_build(&self) -> &blinc_cn::Input {
        ::blinc_layout::build_once::build_once(&self.built, || self.to_cn_widget())
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

        let data = text_input_data_for_field(&self.value, &self.key, self.call_site);
        let mut i = blinc_cn::input(&data).size(size);
        // Write-back and the author's callback share one handler: the
        // signal is written FIRST so a zero-arg closure can read the new
        // text straight off it.
        let bound = writable_signal(&self.value);
        let on_change_ptr = self.on_change;
        if bound.is_some() || on_change_ptr != 0 {
            i = i.on_change(move |new_value: &str| {
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
        // `password()` sets the masking flag AND the input type;
        // `input_type(Password)` alone types the field without masking
        // it, so the secret renders in clear text.
        if self.kind == "password" {
            i = i.password();
        } else {
            i = i.input_type(kind);
        }
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
        // Bound before building: the ref reads and writes the same
        // state the field is built from, so its value methods work
        // without waiting for a render.
        if self.r#ref != 0 {
            match blinc_dsl_core::refs::input_ref_by_id(self.r#ref) {
                Some(input_ref) => i = i.bind(&input_ref),
                None => tracing::warn!(
                    "cn.Input: `ref` is not an Input handle — declare it as `ref name: Input`",
                ),
            }
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

    // Intrinsic size lives here: a textarea's rows-derived height and an
    // input's height are set on the taffy style, so a wrapper that does
    // not forward it hides the size from every builder-tree reader.
    fn layout_style(&self) -> Option<&taffy::Style> {
        self.get_or_build().layout_style()
    }
}
