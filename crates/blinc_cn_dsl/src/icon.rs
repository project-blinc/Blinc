//! Icons crossing from `.blinc` source: `cn.Icon`, and the name lookup
//! every widget with an `icon` prop shares.
//!
//! A DSL source names an icon rather than pasting one, so the value
//! arrives as data and has to be looked up. That is what the `registry`
//! feature on `blinc_icons` is for, and why this crate turns it on: a
//! `.blinc` call site cannot name a constant.

use std::cell::OnceCell;

use blinc_dsl_core::extern_widget;
use blinc_layout::div::ElementBuilder;

/// Lucide path data for a name, as `cn::icon` wants it.
///
/// `None` for an unknown name, with a warning: a typo should show up in
/// the log rather than only as a blank square on screen. Empty is `None`
/// too, being what an omitted prop reads as.
pub(crate) fn path_data(widget: &str, name: &str) -> Option<&'static str> {
    if name.is_empty() {
        return None;
    }
    match blinc_icons::icons::by_name(name) {
        Some(data) => Some(data),
        None => {
            tracing::warn!(
                widget = %widget,
                icon = %name,
                "unknown icon name — see the Lucide set for what is available",
            );
            None
        }
    }
}

/// Resolve an `icon = "…"` prop to a complete SVG string.
///
/// For widgets that draw the icon themselves and want markup rather than
/// path data, `cn.SidebarItem` being the one today. A value already
/// starting with `<` passes through untouched, which is the escape hatch
/// for an icon Lucide does not ship.
pub(crate) fn resolve(widget: &str, icon: &str, size: f32) -> Option<String> {
    if icon.starts_with('<') {
        return Some(icon.to_string());
    }
    path_data(widget, icon).map(|data| blinc_icons::to_svg(data, size))
}

/// `cn.Icon(name = "house", size = "large", color = "#8AB4F8")` — one
/// Lucide glyph.
///
/// ```dsl,ignore
/// Div(class = "row") {
///     cn.Icon(name = "house")
///     cn.Icon(name = "bell", size = "small", color = "#8AB4F8")
///     cn.Icon(name = "settings", size_px = 40.0, stroke = 1.0)
/// }
/// ```
///
/// An unknown name renders nothing and warns, rather than drawing a
/// blank box that reads as a layout bug.
#[extern_widget(namespace = "cn", name = "Icon")]
pub struct CnIcon {
    /// Lucide name, kebab-case: `house`, `square-pen`, `chevron-right`.
    pub name: String,
    /// `xs` / `small` / `medium` (default) / `large` / `xl`.
    pub size: String,
    /// Exact pixel size, which wins over `size` when both are given
    /// since it is the more specific statement. Omitted is zero.
    pub size_px: f64,
    /// Stroke width as a hex string colour. Empty takes the theme's
    /// current text colour.
    pub color: String,
    /// Line thickness. Omitted keeps Lucide's 2.0.
    pub stroke: f64,
    /// Built once, so `build` and `render_props` describe the same
    /// instance and the identity methods can return references.
    #[skip]
    shell: OnceCell<blinc_cn::IconBuilder>,
}

impl CnIcon {
    fn get_or_build(&self) -> &blinc_cn::IconBuilder {
        ::blinc_layout::build_once::build_once(&self.shell, || {
            // An unknown name draws nothing: empty path data is a valid
            // SVG that paints no pixels, which beats a placeholder box.
            let data = path_data("cn.Icon", &self.name).unwrap_or("");
            let mut b = blinc_cn::icon(data);
            if let Some(size) = self.size() {
                b = b.size(size);
            }
            if self.size_px > 0.0 {
                b = b.size_px(self.size_px as f32);
            }
            if let Some(color) = crate::color::parse_color_prop("cn.Icon", "color", &self.color) {
                b = b.color_value(color);
            }
            if self.stroke > 0.0 {
                b = b.stroke_width(self.stroke as f32);
            }
            b
        })
    }

    fn size(&self) -> Option<blinc_cn::IconSize> {
        use blinc_cn::IconSize as S;
        match self.size.as_str() {
            "" => None,
            "xs" => Some(S::ExtraSmall),
            "small" => Some(S::Small),
            "medium" => Some(S::Medium),
            "large" => Some(S::Large),
            "xl" => Some(S::ExtraLarge),
            other => {
                tracing::warn!(size = %other, "cn.Icon: unknown size");
                None
            }
        }
    }
}

impl ElementBuilder for CnIcon {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lucide_name_resolves_to_an_svg() {
        let svg = resolve("test", "house", 18.0).expect("house is a Lucide icon");
        assert!(svg.starts_with("<svg"), "wrapped in an svg tag: {svg}");
        assert!(svg.contains("width=\"18\""), "sized: {svg}");
    }

    /// The escape hatch: an icon Lucide does not ship.
    #[test]
    fn a_raw_svg_passes_through() {
        let raw = "<svg><path d=\"M0 0\"/></svg>";
        assert_eq!(resolve("test", raw, 18.0).as_deref(), Some(raw));
    }

    #[test]
    fn an_unknown_name_is_none_rather_than_a_blank_icon() {
        assert_eq!(resolve("test", "not-an-icon", 18.0), None);
        assert_eq!(resolve("test", "", 18.0), None);
    }
}
