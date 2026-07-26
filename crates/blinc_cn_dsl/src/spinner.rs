//! `cn.Spinner` — loading indicator.

use std::cell::OnceCell;

use blinc_dsl_core::extern_widget;
use blinc_layout::div::ElementBuilder;

/// `cn.Spinner(size?, duration_ms?, color?, track_color?)` —
/// loading indicator.
///
/// Props (DSL surface):
/// - `size: string` — `"small"`, `"medium"` (default), or `"large"`.
/// - `duration_ms: i32` — full-rotation duration in milliseconds.
///   Zero or negative means "use cn default".
/// - `color: string` — foreground arc colour as a hex string
///   (`"#FF0000"` / `"#F00"` / `"FF0000"` / `"0xFF0000"`). Empty
///   string means "use cn default".
/// - `track_color: string` — background track colour, same hex shape.
#[extern_widget(namespace = "cn", name = "Spinner")]
pub struct CnSpinner {
    pub size: String,
    pub duration_ms: i32,
    pub color: String,
    pub track_color: String,
    /// Lazy-constructed cn builder. Same caching rationale as
    /// `CnButton::built`.
    #[skip]
    built: OnceCell<blinc_cn::SpinnerBuilder>,
}

impl CnSpinner {
    fn get_or_build(&self) -> &blinc_cn::SpinnerBuilder {
        self.built.get_or_init(|| self.to_cn_builder())
    }

    fn to_cn_builder(&self) -> blinc_cn::SpinnerBuilder {
        let size = match self.size.as_str() {
            "small" => blinc_cn::SpinnerSize::Small,
            "large" => blinc_cn::SpinnerSize::Large,
            "" | "medium" => blinc_cn::SpinnerSize::Medium,
            other => {
                tracing::warn!(
                    size = %other,
                    "cn.Spinner: unknown size — falling back to `medium`",
                );
                blinc_cn::SpinnerSize::Medium
            }
        };
        let mut b = blinc_cn::spinner().size(size);
        if self.duration_ms > 0 {
            b = b.duration_ms(self.duration_ms as u32);
        }
        if let Some(c) = crate::color::parse_color_prop("cn.Spinner", "color", &self.color) {
            b = b.color(c);
        }
        if let Some(c) =
            crate::color::parse_color_prop("cn.Spinner", "track_color", &self.track_color)
        {
            b = b.track_color(c);
        }
        b
    }
}

impl ElementBuilder for CnSpinner {
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
