//! `cn.Avatar` — circular / square profile image with fallback initials.

use std::cell::OnceCell;

use blinc_dsl_core::{Reactive, extern_widget};
use blinc_layout::div::ElementBuilder;

/// `cn.Avatar(src?, fallback?, size?, shape?, fallback_bg?, fallback_color?)`
/// — profile image with fallback initials.
///
/// Props (DSL surface):
/// - `src: string` — image URL/path. Empty falls back to text.
/// - `fallback: string` — initials shown when no image (or image
///   fails to load). Typically `"JD"` etc.
/// - `size: string` — `"extra_small"`, `"small"`, `"medium"`
///   (default), `"large"`, `"extra_large"`.
/// - `shape: string` — `"circle"` (default) or `"square"`.
/// - `fallback_bg: string` / `fallback_color: string` — hex colour
///   overrides for the fallback-initials background and text.
///
/// `blinc_cn::AvatarBuilder` doesn't expose a OnceCell-style cache
/// API today — its `.src() / .fallback() / .size()` methods consume
/// `self` and return `Self`. Each render rebuilds the builder; cost
/// is dominated by image-loading anyway, so the cache cost would be
/// negligible. `children_builders()` returns `&[]` for now (no
/// children-bearing API on cn::Avatar).
#[extern_widget(namespace = "cn", name = "Avatar")]
pub struct CnAvatar {
    pub src: Reactive<String>,
    pub fallback: Reactive<String>,
    pub size: String,
    pub shape: String,
    pub fallback_bg: String,
    pub fallback_color: String,
    /// Cached cn builder. Needed so the `ElementBuilder` identity
    /// methods can return references -- they borrow from the built
    /// widget, which a fresh `to_cn_builder()` temporary cannot
    /// outlive. Same caching rationale as `CnButton::built`.
    #[skip]
    built: OnceCell<blinc_cn::AvatarBuilder>,
}

/// Whether a reactive string prop was actually supplied.
///
/// An omitted prop arrives as an empty literal, and `src` and
/// `fallback` pick different content, so an empty one has to read as
/// absent. A bound prop counts as supplied whatever it holds right now:
/// it can become non-empty later, and the rebuild that follows needs
/// the branch to already be there.
fn supplied(r: &Reactive<String>) -> bool {
    match r {
        Reactive::Literal(s) => !s.is_empty(),
        _ => true,
    }
}

impl CnAvatar {
    fn get_or_build(&self) -> &blinc_cn::AvatarBuilder {
        self.built.get_or_init(|| self.to_cn_builder())
    }

    fn to_cn_builder(&self) -> blinc_cn::AvatarBuilder {
        let size = match self.size.as_str() {
            "extra_small" | "xs" => blinc_cn::AvatarSize::ExtraSmall,
            "small" | "sm" => blinc_cn::AvatarSize::Small,
            "large" | "lg" => blinc_cn::AvatarSize::Large,
            "extra_large" | "xl" => blinc_cn::AvatarSize::ExtraLarge,
            "" | "medium" | "md" => blinc_cn::AvatarSize::Medium,
            other => {
                tracing::warn!(
                    size = %other,
                    "cn.Avatar: unknown size — falling back to `medium`",
                );
                blinc_cn::AvatarSize::Medium
            }
        };
        let shape = match self.shape.as_str() {
            "square" => blinc_cn::AvatarShape::Square,
            "" | "circle" => blinc_cn::AvatarShape::Circle,
            other => {
                tracing::warn!(
                    shape = %other,
                    "cn.Avatar: unknown shape — falling back to `circle`",
                );
                blinc_cn::AvatarShape::Circle
            }
        };
        let mut b = blinc_cn::avatar().size(size).shape(shape);
        if supplied(&self.src) {
            b = b.src(blinc_layout::binding::IntoReactive::into_reactive(
                self.src.clone(),
            ));
        }
        if supplied(&self.fallback) {
            b = b.fallback(blinc_layout::binding::IntoReactive::into_reactive(
                self.fallback.clone(),
            ));
        }
        if let Some(c) =
            crate::color::parse_color_prop("cn.Avatar", "fallback_bg", &self.fallback_bg)
        {
            b = b.fallback_bg(c);
        }
        if let Some(c) =
            crate::color::parse_color_prop("cn.Avatar", "fallback_color", &self.fallback_color)
        {
            b = b.fallback_color(c);
        }
        b
    }
}

impl ElementBuilder for CnAvatar {
    fn build(&self, tree: &mut blinc_layout::LayoutTree) -> blinc_layout::LayoutNodeId {
        self.get_or_build().build(tree)
    }
    fn render_props(&self) -> blinc_layout::RenderProps {
        self.get_or_build().render_props()
    }
    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        // Forwarded, not `&[]`: the avatar builds a fallback-initials
        // text child, and reporting none left the renderer unable to
        // walk into it -- "N layout children but 0 builder children" --
        // so the initials never picked up their render props.
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
