//! `cn.Progress` — value bar.

use std::cell::OnceCell;

use blinc_dsl_core::{Reactive, extern_widget};
use blinc_layout::div::ElementBuilder;

/// `cn.Progress(value, size?, width?, indicator_color?, track_color?, rounded?)`
/// — value bar.
///
/// Props (DSL surface):
/// - `value: Reactive<f64>` — fill ratio in `[0.0, 1.0]`. Accepts
///   any of:
///     * `cn.Progress(value = 0.5)` — static literal.
///     * `cn.Progress(value = my_signal)` — live signal binding.
///       Updates patch the property-binding registry without a
///       Stateful rebuild.
///     * `cn.Progress(value = computed { … } : f64)` — derived
///       reactive, same live-binding semantics.
///   Out-of-range values get clamped at render time on the cn side.
/// - `size: string` — `"small"`, `"medium"` (default), or `"large"`.
/// - `width: f64` — bar width in pixels. Zero (default) means "use
///   cn default of 200px".
/// - `indicator_color: string` / `track_color: string` — hex colour
///   overrides (`"#RRGGBB"` / `"RRGGBB"` / `"0xRRGGBB"` / `"#RGB"`).
/// - `rounded: f64` — corner radius override. Zero means "use cn
///   default" (the size-derived value).
///
/// `value` is the canary `Reactive<T>` prop — it exercises the
/// macro's two-slot FFI emission, the `lower_reactive_args` lowering
/// pass, and the cn-side `IntoReactive<f32>` bridge end-to-end. Other
/// numeric / colour props on this widget stay literal for now.
#[extern_widget(namespace = "cn", name = "Progress")]
pub struct CnProgress {
    pub value: Reactive<f64>,
    pub size: String,
    pub width: f64,
    pub indicator_color: String,
    pub track_color: String,
    pub rounded: f64,
    /// Lazy-constructed cn widget. Same caching rationale as
    /// `CnButton::built`. We cache the FINAL `Progress` (via
    /// `.build_component()`) rather than `ProgressBuilder` because
    /// the builder type isn't re-exported at `blinc_cn::` crate
    /// root; `Progress` is.
    #[skip]
    built: OnceCell<blinc_cn::Progress>,
}

impl CnProgress {
    fn get_or_build(&self) -> &blinc_cn::Progress {
        ::blinc_layout::build_once::build_once(&self.built, || {
            self.to_cn_builder().build_component()
        })
    }

    fn to_cn_builder(&self) -> blinc_cn::components::progress::ProgressBuilder {
        let size = match self.size.as_str() {
            "small" => blinc_cn::ProgressSize::Small,
            "large" => blinc_cn::ProgressSize::Large,
            "" | "medium" => blinc_cn::ProgressSize::Medium,
            other => {
                tracing::warn!(
                    size = %other,
                    "cn.Progress: unknown size — falling back to `medium`",
                );
                blinc_cn::ProgressSize::Medium
            }
        };
        // `Reactive<f64>` goes straight through: cn::progress is
        // generic over `IntoF32`, so the f64 -> f32 narrowing happens
        // once inside the binding layer instead of here.
        let mut b = blinc_cn::progress(self.value.clone());
        b = b.size(size);
        if self.width > 0.0 {
            b = b.w(self.width as f32);
        }
        if let Some(c) =
            crate::color::parse_color_prop("cn.Progress", "indicator_color", &self.indicator_color)
        {
            b = b.indicator_color(c);
        }
        if let Some(c) =
            crate::color::parse_color_prop("cn.Progress", "track_color", &self.track_color)
        {
            b = b.track_color(c);
        }
        if self.rounded > 0.0 {
            b = b.rounded(self.rounded as f32);
        }
        b
    }
}

impl ElementBuilder for CnProgress {
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
