//! `cn.AspectRatio` — a box that keeps a width-to-height ratio.

use std::cell::OnceCell;

use blinc_dsl_core::extern_widget;
use blinc_layout::div::ElementBuilder;

/// `cn.AspectRatio(ratio = 1.777) { … }` — hold a shape while the
/// content inside varies.
///
/// ```dsl,ignore
/// cn.AspectRatio(preset = "widescreen") {
///     cn.Skeleton(w = 320.0, h = 180.0)
/// }
/// ```
///
/// `ratio` is width / height, so 1.777 is 16:9. A `preset` name is the
/// same thing spelled for the common cases and wins when both are
/// given, since naming one is the more specific statement.
///
/// Layout shape beyond the ratio (`w`, `h`, padding, margins) rides the
/// universal `Div`-style overlay surface rather than per-widget props,
/// matching every other cn wrapper.
#[extern_widget(namespace = "cn", name = "AspectRatio")]
pub struct CnAspectRatio {
    /// Width divided by height. Ignored when `preset` names one.
    pub ratio: f64,
    /// `square` / `widescreen` / `traditional` / `ultrawide` / `photo`
    /// / `portrait`. Empty means use `ratio`.
    pub preset: String,
    #[children]
    pub children: Vec<Box<dyn ElementBuilder>>,
    /// Built once so `build()` and `render_props()` describe the same
    /// instance, and so the identity methods can borrow from it.
    #[skip]
    shell: OnceCell<blinc_cn::AspectRatio>,
}

impl CnAspectRatio {
    fn get_or_build(&self) -> &blinc_cn::AspectRatio {
        self.shell.get_or_init(|| {
            let builder = match self.preset.as_str() {
                "" => blinc_cn::aspect_ratio(self.ratio_or_default()),
                name => match preset_from_name(name) {
                    Some(preset) => blinc_cn::AspectRatioBuilder::from_preset(preset),
                    None => {
                        tracing::warn!(
                            preset = %name,
                            "cn.AspectRatio: unknown preset — falling back to `ratio`",
                        );
                        blinc_cn::aspect_ratio(self.ratio_or_default())
                    }
                },
            };
            builder.build_final()
        })
    }

    /// A ratio of zero is what an omitted prop reads as, and a zero
    /// ratio has no meaning, so it falls back to square rather than
    /// being clamped to a sliver by the builder's `max(0.01)`.
    fn ratio_or_default(&self) -> f32 {
        if self.ratio > 0.0 {
            self.ratio as f32
        } else {
            1.0
        }
    }
}

fn preset_from_name(name: &str) -> Option<blinc_cn::AspectRatioPreset> {
    use blinc_cn::AspectRatioPreset as P;
    Some(match name {
        "square" => P::Square,
        "widescreen" => P::Widescreen,
        "traditional" => P::Traditional,
        "ultrawide" => P::Ultrawide,
        "photo" => P::Photo,
        "portrait" => P::Portrait,
        _ => return None,
    })
}

impl ElementBuilder for CnAspectRatio {
    fn build(&self, tree: &mut blinc_layout::LayoutTree) -> blinc_layout::LayoutNodeId {
        // Same shape as `cn.Card`: build the shell, then parent the DSL
        // body to it. `AspectRatioBuilder::child` takes an owned value
        // and this wrapper holds shared refs, so the children go on
        // directly instead.
        let node = self.get_or_build().build(tree);
        for child in &self.children {
            let child_node = child.build(tree);
            tree.add_child(node, child_node);
        }
        node
    }

    fn render_props(&self) -> blinc_layout::RenderProps {
        self.get_or_build().render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        &self.children
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
}
