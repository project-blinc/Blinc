//! `cn.Button` — single-line action button.

use std::cell::OnceCell;

use blinc_dsl_core::{Reactive, extern_widget};
use blinc_layout::div::ElementBuilder;

/// `cn.Button(label, variant?, size?, disabled?, icon?, icon_position?, icon_size?, color?, on_click?)`
/// — a single-line action button.
///
/// Props (DSL surface):
/// - `label: Reactive<string>` — the button text. Positional or named.
///   Accepts three call-site shapes:
///     * `cn.Button(label = "Save")` — static literal.
///     * `cn.Button(label = my_signal)` — signal-bound. Value
///       snapshots at build time today; full live binding lands
///       once cn-side `IntoReactive<String>` arrives on
///       `ButtonBuilder::label`.
///     * `cn.Button(label = computed { … } : string)` — derived,
///       same snapshot-at-build semantics for now.
/// - `variant: string` — `"primary"` (default), `"secondary"`,
///   `"destructive"`, `"outline"`, `"ghost"`, or `"link"`. Unknown
///   values fall back to `"primary"`.
/// - `size: string` — `"small"`, `"medium"` (default), or `"large"`.
///   Unknown values fall back to `"medium"`.
/// - `disabled: Reactive<bool>` — false by default. Accepts the same
///   three call-site shapes as `label`: static literal, bare-signal
///   ref, or `computed { … } : bool`. Snapshot-only today; live
///   binding waits on cn-side `IntoReactive<bool>` for `ButtonBuilder::disabled`.
/// - `icon: string` — icon name / SVG-path. Empty string means no icon.
/// - `icon_position: string` — `"start"` (default) or `"end"`. Ignored
///   when `icon` is empty.
/// - `icon_size: f64` — pixel size override. Zero means "derive from
///   button size" (the cn-side default).
/// - `color: string` — text/icon colour override as a hex string
///   (`"#FF0000"` / `"#F00"` / `"FF0000"` / `"0xFF0000"`). Empty
///   string means "use the variant's default colour".
/// - `on_click: || => unit` — DSL closure invoked on click. Mirrors
///   the existing `Div(on_click = ||{ … })` shape: zero-arg, fires
///   for side effects (signal writes, FSM triggers). The cn-side
///   `EventContext` is discarded — the closure runs as if it had
///   been written `||{ … }` on a plain Div.
#[extern_widget(namespace = "cn", name = "Button")]
pub struct CnButton {
    pub label: Reactive<String>,
    pub variant: String,
    pub size: String,
    pub disabled: Reactive<bool>,
    pub icon: String,
    pub icon_position: String,
    pub icon_size: f64,
    pub color: String,
    /// Closure handle minted by Zyntax's `CreateClosure` → `func_addr`.
    /// Zero when the user omitted `on_click`. The cn-side builder
    /// constructed by `get_or_build` handles the transmute + dispatch
    /// when invoked.
    pub on_click: i64,
    /// Lazy-constructed cn-side builder. Cached so the `ElementBuilder`
    /// trait methods can delegate to a stable reference instead of
    /// rebuilding per call — and so `children_builders()` can hand
    /// back the cn widget's own slice rather than an empty stub. Not
    /// part of the FFI surface; the macro skips it and the thunk-side
    /// constructor fills the field via `Default::default()`.
    #[skip]
    built: OnceCell<blinc_cn::ButtonBuilder>,
}

impl CnButton {
    /// Lazy-build the cn-side widget once, then hand back a stable
    /// reference. Mirrors the OnceCell pattern cn's own
    /// `ButtonBuilder::get_or_build` uses.
    fn get_or_build(&self) -> &blinc_cn::ButtonBuilder {
        self.built.get_or_init(|| self.to_cn_builder())
    }

    fn to_cn_builder(&self) -> blinc_cn::ButtonBuilder {
        let variant = match self.variant.as_str() {
            "secondary" => blinc_cn::ButtonVariant::Secondary,
            "destructive" => blinc_cn::ButtonVariant::Destructive,
            "outline" => blinc_cn::ButtonVariant::Outline,
            "ghost" => blinc_cn::ButtonVariant::Ghost,
            "link" => blinc_cn::ButtonVariant::Link,
            // Empty string ("variant not supplied") OR explicit "primary"
            // OR an unknown value all resolve to Primary. Unknown values
            // get a tracing::warn so misspelled enum strings surface in
            // logs without breaking the build.
            "" | "primary" => blinc_cn::ButtonVariant::Primary,
            other => {
                tracing::warn!(
                    variant = %other,
                    "cn.Button: unknown variant — falling back to `primary`",
                );
                blinc_cn::ButtonVariant::Primary
            }
        };
        let size = match self.size.as_str() {
            "small" => blinc_cn::ButtonSize::Small,
            "large" => blinc_cn::ButtonSize::Large,
            "" | "medium" => blinc_cn::ButtonSize::Medium,
            other => {
                tracing::warn!(
                    size = %other,
                    "cn.Button: unknown size — falling back to `medium`",
                );
                blinc_cn::ButtonSize::Medium
            }
        };
        // `disabled` crosses live: the DSL `Reactive<bool>` converts
        // to the layout channel via `IntoReactive`, so a signal-bound
        // value registers a `deps()` subscription on the cn side and
        // the button restyles on set.
        //
        // `label` still snapshots — `ButtonBuilder::label` takes a
        // plain `String`. Swap it the same way once that setter grows
        // an `IntoReactive<String>` surface.
        let label = self.label.get_or_else(String::new());
        let mut b = blinc_cn::button(label)
            .variant(variant)
            .size(size)
            .disabled(self.disabled.clone());
        if !self.icon.is_empty() {
            b = b.icon(self.icon.clone());
            // `icon_position` only meaningful when an icon is present.
            let pos = match self.icon_position.as_str() {
                "end" => blinc_cn::IconPosition::End,
                "" | "start" => blinc_cn::IconPosition::Start,
                other => {
                    tracing::warn!(
                        icon_position = %other,
                        "cn.Button: unknown icon_position — falling back to `start`",
                    );
                    blinc_cn::IconPosition::Start
                }
            };
            b = b.icon_position(pos);
            if self.icon_size > 0.0 {
                b = b.icon_size(self.icon_size as f32);
            }
        }
        if let Some(c) = crate::color::parse_color_prop("cn.Button", "color", &self.color) {
            b = b.color(c);
        }
        if self.on_click != 0 {
            // Mirrors `blinc_div_view`: Zyntax mints a zero-arg
            // `extern "C" fn()` for the DSL closure and hands it across
            // as `i64`. cn::button's on_click handler takes
            // `&EventContext`; the substrate-level DSL closure shape
            // doesn't carry that context yet, so we discard it and
            // fire the closure with no args. Signal writes inside the
            // closure route through the existing `__signal_set_*`
            // host externs as usual.
            let click_ptr = self.on_click;
            b = b.on_click(move |_ctx| {
                type ClosureFn = extern "C" fn();
                let func: ClosureFn = unsafe { std::mem::transmute(click_ptr) };
                func();
            });
        }
        b
    }
}

impl ElementBuilder for CnButton {
    fn build(&self, tree: &mut blinc_layout::LayoutTree) -> blinc_layout::LayoutNodeId {
        self.get_or_build().build(tree)
    }

    fn render_props(&self) -> blinc_layout::RenderProps {
        self.get_or_build().render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.get_or_build().children_builders()
    }

    // MUST forward — without these, the renderer queries `CnButton`
    // (the outer wrapper) for identity / interaction surface and gets
    // the trait defaults (None / &[]), even though every layer below
    // (cn::ButtonBuilder → cn::Button → layout_button → Stateful → Div)
    // carries the real values. Symptom: clicks silently drop because
    // `event_handlers()` returns None; CSS class selectors don't match
    // because `element_classes()` returns `&[]`. Same gotcha
    // `gotcha_element_builder_trait_forwarding` calls out — every
    // wrapping `impl ElementBuilder for Foo` needs the full set.
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

    // layout_style forwarding deliberately omitted: blinc_cn_dsl
    // doesn't depend on `taffy` directly, and the layout side reads
    // the style from the node_id `build()` populated rather than via
    // the wrapper. If a downstream consumer needs `.layout_style()`
    // on `CnButton`, lift `taffy` into the workspace dep list and
    // forward to `self.get_or_build().layout_style()`.
}
