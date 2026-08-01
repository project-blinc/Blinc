//! `cn.Separator` — divider line.

use std::cell::OnceCell;

use crate::bridge::is_set;

use blinc_dsl_core::{Reactive, extern_widget};
use blinc_layout::div::ElementBuilder;

/// `cn.Separator(orientation?, bg?, opacity?)` — divider line.
///
/// Props (DSL surface):
/// - `orientation: string` — `"horizontal"` (default) or `"vertical"`.
/// - `bg: string` — background colour override as a hex string
///   (`"#FF0000"` / `"#F00"` / `"FF0000"` / `"0xFF0000"`). Empty
///   means "use the theme's border-token default".
/// - `opacity: f64` — clamps to `[0, 1]`. Zero (the default) means
///   "no override" rather than "fully transparent" — the cn-side
///   default opacity applies.
///
/// `blinc_cn::Separator` also exposes layout-shape builders (`.w()`,
/// `.h()`, `.m{,x,y,t,b,l,r}()`) — those overlap with the DSL's
/// universal `Div(...)` styling surface and should ride through
/// that path rather than as per-widget props.
#[extern_widget(namespace = "cn", name = "Separator")]
pub struct CnSeparator {
    pub orientation: String,
    pub bg: String,
    pub opacity: Reactive<f64>,
    /// Lazy-constructed cn widget. Same caching rationale as
    /// `CnButton::built`.
    #[skip]
    built: OnceCell<blinc_cn::Separator>,
}

impl CnSeparator {
    fn get_or_build(&self) -> &blinc_cn::Separator {
        ::blinc_layout::build_once::build_once(&self.built, || self.to_cn_widget())
    }

    fn to_cn_widget(&self) -> blinc_cn::Separator {
        let mut s = match self.orientation.as_str() {
            "vertical" => blinc_cn::Separator::new().vertical(),
            "" | "horizontal" => blinc_cn::Separator::new(),
            other => {
                tracing::warn!(
                    orientation = %other,
                    "cn.Separator: unknown orientation — falling back to `horizontal`",
                );
                blinc_cn::Separator::new()
            }
        };
        if let Some(c) = crate::color::parse_color_prop("cn.Separator", "bg", &self.bg) {
            s = s.bg(c);
        }
        if is_set(&self.opacity) {
            // `0.0` is the macro-injected default for an unsupplied
            // literal prop — treated as "no override" rather than
            // "fully transparent", because an invisible-by-default
            // separator is the wrong ergonomic. A bound opacity is
            // always wired up: reading 0.0 on the first frame is a
            // live binding, not an absent one.
            //
            // `Div::opacity` clamps to [0, 1] on every write, including
            // binding updates, so no clamp is needed here.
            s = s.opacity(self.opacity.clone());
        }
        s
    }
}

impl ElementBuilder for CnSeparator {
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
