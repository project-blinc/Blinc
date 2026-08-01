//! `cn.Skeleton` — placeholder block while content loads.

use std::cell::OnceCell;

use crate::bridge::is_set;
use blinc_dsl_core::{Reactive, extern_widget};
use blinc_layout::div::ElementBuilder;

/// `cn.Skeleton(w?, h?, rounded?, circle_size?)` — placeholder
/// block while content loads.
///
/// Props (DSL surface):
/// - `w: f64` — width in pixels. Zero means "use cn default
///   (160px)". Set together with `h` for a custom rectangle.
/// - `h: f64` — height in pixels. Same zero-as-default semantics.
/// - `rounded: f64` — corner radius. Zero means "use cn default
///   (4px)".
/// - `circle_size: f64` — when non-zero, builds a circular
///   skeleton (`cn::skeleton_circle(size)`) of that diameter and
///   ignores `w` / `h` / `rounded`.
///
/// Shimmer animation isn't part of this prop surface — `cn::Skeleton`
/// promotes to `AnimatedSkeleton` via `.shimmer(timeline)`, which
/// takes a `SharedAnimatedTimeline` that the DSL doesn't expose
/// today. Static skeletons cover the loading-state case for now.
#[extern_widget(namespace = "cn", name = "Skeleton")]
pub struct CnSkeleton {
    pub w: Reactive<f64>,
    pub h: Reactive<f64>,
    pub rounded: Reactive<f64>,
    pub circle_size: f64,
    /// Lazy-constructed cn widget. Same caching rationale as
    /// `CnButton::built`.
    #[skip]
    built: OnceCell<blinc_cn::Skeleton>,
}

impl CnSkeleton {
    fn get_or_build(&self) -> &blinc_cn::Skeleton {
        ::blinc_layout::build_once::build_once(&self.built, || self.to_cn_widget())
    }

    fn to_cn_widget(&self) -> blinc_cn::Skeleton {
        // `circle_size` stays eager: it selects a different
        // constructor, so a change is structural rather than a value
        // patch and there is no binding to hang it on.
        if self.circle_size > 0.0 {
            // Circle takes priority — its own dimensions; `w`/`h`/
            // `rounded` are silently ignored to keep the call shape
            // small. The doc above flags this.
            return blinc_cn::Skeleton::circle(self.circle_size as f32);
        }
        // Width / height / radius are all bindable properties, so a
        // signal patches `RenderProps` in place — no rebuild.
        let mut s = blinc_cn::Skeleton::new();
        if is_set(&self.w) {
            s = s.w(self.w.clone());
        }
        if is_set(&self.h) {
            s = s.h(self.h.clone());
        }
        if is_set(&self.rounded) {
            s = s.rounded(self.rounded.clone());
        }
        s
    }
}

impl ElementBuilder for CnSkeleton {
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
