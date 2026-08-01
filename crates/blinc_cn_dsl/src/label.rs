//! `cn.Label` — form-field caption.

use std::cell::OnceCell;

use blinc_dsl_core::{Reactive, extern_widget};
use blinc_layout::div::ElementBuilder;

/// `cn.Label(text, size?, required?, disabled?)` — form-field caption.
///
/// Props (DSL surface):
/// - `text: string` — the label text.
/// - `size: string` — `"small"`, `"medium"` (default), or `"large"`.
/// - `required: bool` — appends a `*` glyph when true.
/// - `disabled: bool` — dimmed appearance when true.
#[extern_widget(namespace = "cn", name = "Label")]
pub struct CnLabel {
    pub text: Reactive<String>,
    pub size: String,
    pub required: bool,
    pub disabled: bool,
    /// Lazy-constructed cn builder. Same caching rationale as
    /// `CnButton::built`.
    #[skip]
    built: OnceCell<blinc_cn::LabelBuilder>,
}

impl CnLabel {
    fn get_or_build(&self) -> &blinc_cn::LabelBuilder {
        ::blinc_layout::build_once::build_once(&self.built, || self.to_cn_builder())
    }

    fn to_cn_builder(&self) -> blinc_cn::LabelBuilder {
        let size = match self.size.as_str() {
            "small" => blinc_cn::LabelSize::Small,
            "large" => blinc_cn::LabelSize::Large,
            "" | "medium" => blinc_cn::LabelSize::Medium,
            other => {
                tracing::warn!(
                    size = %other,
                    "cn.Label: unknown size — falling back to `medium`",
                );
                blinc_cn::LabelSize::Medium
            }
        };
        let mut b = blinc_cn::label(self.text.clone()).size(size);
        if self.required {
            b = b.required();
        }
        b.disabled(self.disabled)
    }
}

impl ElementBuilder for CnLabel {
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
    // Without these the renderer queries the wrapper for identity and
    // interaction surface and gets the trait defaults (None / &[]),
    // even though every layer below carries the real values. For a
    // CSS-class-driven widget that means selectors never match, so it
    // renders with no chrome and inherits whatever text colour is
    // around it.
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
