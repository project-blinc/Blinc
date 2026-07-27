//! `cn.Alert` — inline notification banner.

use std::cell::OnceCell;

use blinc_dsl_core::{Reactive, extern_widget};
use blinc_layout::div::ElementBuilder;

/// `cn.Alert(message, variant?)` — inline notification banner.
///
/// Props (DSL surface):
/// - `message: string` — the alert body.
/// - `variant: string` — `"default"` (info), `"success"`, `"warning"`,
///   `"destructive"`. Unknown values fall back to `"default"`.
#[extern_widget(namespace = "cn", name = "Alert")]
pub struct CnAlert {
    pub message: Reactive<String>,
    pub variant: String,
    /// Lazy-constructed cn widget. Same caching rationale as
    /// `CnButton::built`.
    #[skip]
    built: OnceCell<blinc_cn::Alert>,
}

impl CnAlert {
    fn get_or_build(&self) -> &blinc_cn::Alert {
        self.built.get_or_init(|| self.to_cn_widget())
    }

    fn to_cn_widget(&self) -> blinc_cn::Alert {
        let variant = match self.variant.as_str() {
            "success" => blinc_cn::AlertVariant::Success,
            "warning" => blinc_cn::AlertVariant::Warning,
            "destructive" => blinc_cn::AlertVariant::Destructive,
            "" | "default" => blinc_cn::AlertVariant::Default,
            other => {
                tracing::warn!(
                    variant = %other,
                    "cn.Alert: unknown variant — falling back to `default`",
                );
                blinc_cn::AlertVariant::Default
            }
        };
        blinc_cn::alert(self.message.clone()).variant(variant)
    }
}

impl ElementBuilder for CnAlert {
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
