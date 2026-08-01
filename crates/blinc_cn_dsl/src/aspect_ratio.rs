//! `cn.AspectRatio` — a box that keeps a width-to-height ratio.

use std::cell::{OnceCell, RefCell};

use blinc_dsl_core::extern_widget;
use blinc_layout::div::{ElementBuilder, div};

/// `cn.AspectRatio(preset = "widescreen", w = 240.0) { … }` — hold a
/// shape while the content inside varies.
///
/// ```dsl,ignore
/// cn.AspectRatio(preset = "widescreen", w = 240.0) {
///     cn.Skeleton(w = 240.0, h = 135.0, rounded = 6.0)
/// }
/// ```
///
/// `ratio` is width / height, so 1.777 is 16:9. A `preset` name is the
/// same thing spelled for the common cases and wins when both are
/// given, since naming one is the more specific statement.
///
/// `w` / `h` are real props rather than the usual `Div` overlay: the
/// builder derives the other dimension from the ratio, so a size that
/// went to the wrapper instead would leave the box sizing itself from
/// its content and the ratio would not hold. Give one, not both.
///
/// The body is handed to the cn builder rather than parented
/// afterwards, and `children_builders` forwards to the shell — see
/// `CnScrollArea` for why that ordering is what makes a container
/// widget work at all.
#[extern_widget(namespace = "cn", name = "AspectRatio")]
pub struct CnAspectRatio {
    /// Width divided by height. Ignored when `preset` names one.
    pub ratio: f64,
    /// `square` / `widescreen` / `traditional` / `ultrawide` / `photo`
    /// / `portrait`. Empty means use `ratio`.
    pub preset: String,
    /// Box width; the height follows from the ratio. Omitted is zero.
    pub w: f64,
    /// Box height; the width follows from the ratio. Omitted is zero.
    pub h: f64,
    /// Fill colour as a hex string. Empty leaves the box transparent,
    /// which is the useful default for holding a shape around an image
    /// but shows nothing on its own.
    pub bg: String,
    /// Corner radius. Omitted is zero.
    pub rounded: f64,
    #[children]
    pub children: RefCell<Vec<Box<dyn ElementBuilder>>>,
    /// Built once, consuming `children`.
    #[skip]
    shell: OnceCell<blinc_cn::AspectRatio>,
}

impl CnAspectRatio {
    fn get_or_build(&self) -> &blinc_cn::AspectRatio {
        ::blinc_layout::build_once::build_once(&self.shell, || {
            let mut b = match self.preset.as_str() {
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
            if self.w > 0.0 {
                b = b.w(self.w as f32);
            }
            if self.h > 0.0 {
                b = b.h(self.h as f32);
            }
            if let Some(color) = crate::color::parse_color_prop("cn.AspectRatio", "bg", &self.bg) {
                b = b.bg(color);
            }
            if self.rounded > 0.0 {
                b = b.rounded(self.rounded as f32);
            }
            // One content child holding the whole body: the builder
            // takes a single element, and it is that wrapper which
            // carries the fill sizing the ratio depends on.
            let mut content = div().flex_col();
            for child in self.children.borrow_mut().drain(..) {
                content = content.child_box(child);
            }
            b.child(content).build_final()
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
        self.get_or_build().build(tree)
    }

    fn render_props(&self) -> blinc_layout::RenderProps {
        self.get_or_build().render_props()
    }

    /// The shell's children, which now hold the body.
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
}
