//! `cn.Badge` — inline status / count chip.

use std::cell::OnceCell;

use blinc_dsl_core::{Reactive, extern_widget};
use blinc_layout::div::ElementBuilder;

/// `cn.Badge(label, variant?, style?)` — inline status / count chip.
///
/// Props (DSL surface):
/// - `label: Reactive<String>` — chip text. Bind a `signal` and the
///   chip follows it; text has no property writer, so a bound label
///   rebuilds the chip through `deps()`.
/// - `variant: string` — `"default"`, `"secondary"`, `"success"`,
///   `"warning"`, or `"destructive"`. Unknown values fall back to
///   `"default"`. (`blinc_cn::BadgeVariant` doesn't currently expose
///   an `Outline` variant; if it grows one, add a match arm here.)
/// - `style: string` — `"soft"` (default, tinted bg + same-hue text)
///   or `"solid"` (filled bg + inverse text). Unknown → `"soft"`.
///
/// `cn::Badge::icon()` takes a Rust `ElementBuilder`, not a string —
/// piping that through the FFI needs a children-/slot-style design,
/// not a scalar prop. Deferred to the same follow-up that wires
/// children blocks generally.
#[extern_widget(namespace = "cn", name = "Badge")]
pub struct CnBadge {
    pub label: Reactive<String>,
    pub variant: String,
    pub style: String,
    /// Lazy-constructed cn widget. Same caching rationale as
    /// `CnButton::built`.
    #[skip]
    built: OnceCell<blinc_cn::Badge>,
}

impl CnBadge {
    fn get_or_build(&self) -> &blinc_cn::Badge {
        ::blinc_layout::build_once::build_once(&self.built, || self.to_cn_widget())
    }

    fn to_cn_widget(&self) -> blinc_cn::Badge {
        let variant = match self.variant.as_str() {
            "secondary" => blinc_cn::BadgeVariant::Secondary,
            "success" => blinc_cn::BadgeVariant::Success,
            "warning" => blinc_cn::BadgeVariant::Warning,
            "destructive" => blinc_cn::BadgeVariant::Destructive,
            "" | "default" => blinc_cn::BadgeVariant::Default,
            other => {
                tracing::warn!(
                    variant = %other,
                    "cn.Badge: unknown variant — falling back to `default`",
                );
                blinc_cn::BadgeVariant::Default
            }
        };
        let style = match self.style.as_str() {
            "solid" => blinc_cn::BadgeStyle::Solid,
            "" | "soft" => blinc_cn::BadgeStyle::Soft,
            other => {
                tracing::warn!(
                    style = %other,
                    "cn.Badge: unknown style — falling back to `soft`",
                );
                blinc_cn::BadgeStyle::Soft
            }
        };
        // Variant and style first: `reactive_label` snapshots them into
        // the rebuild closure, so anything set after it would be lost on
        // the next refresh.
        blinc_cn::badge("")
            .variant(variant)
            .style(style)
            .reactive_label(blinc_layout::binding::IntoReactive::into_reactive(
                self.label.clone(),
            ))
    }
}

impl ElementBuilder for CnBadge {
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
