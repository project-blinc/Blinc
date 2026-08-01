//! `cn.Kbd` — keyboard-key chip.

use std::cell::OnceCell;

use blinc_dsl_core::{Reactive, extern_widget};
use blinc_layout::div::ElementBuilder;

/// `cn.Kbd(text, size?)` — a keyboard key rendered as a chip, for
/// documenting shortcuts inline.
///
/// Props (DSL surface):
/// - `text: Reactive<String>` — the key or chord, e.g. `"Ctrl"`,
///   `"⌘K"`. Bind a `signal` and the chip follows it; text has no
///   property writer, so a bound key rebuilds through `deps()`.
/// - `size: string` — `"small"` / `"sm"`, `"medium"` / `"md"`
///   (default), `"large"` / `"lg"`. Unknown values fall back to
///   medium.
#[extern_widget(namespace = "cn", name = "Kbd")]
pub struct CnKbd {
    pub text: Reactive<String>,
    pub size: String,
    /// Lazy-constructed cn widget. Same caching rationale as
    /// `CnButton::built`.
    #[skip]
    built: OnceCell<blinc_cn::KbdBuilder>,
}

impl CnKbd {
    fn get_or_build(&self) -> &blinc_cn::KbdBuilder {
        ::blinc_layout::build_once::build_once(&self.built, || self.to_cn_widget())
    }

    fn to_cn_widget(&self) -> blinc_cn::KbdBuilder {
        let size = match self.size.as_str() {
            "small" | "sm" => blinc_cn::KbdSize::Small,
            "large" | "lg" => blinc_cn::KbdSize::Large,
            _ => blinc_cn::KbdSize::Medium,
        };
        blinc_cn::kbd(blinc_layout::binding::IntoReactive::into_reactive(
            self.text.clone(),
        ))
        .size(size)
    }
}

impl ElementBuilder for CnKbd {
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
