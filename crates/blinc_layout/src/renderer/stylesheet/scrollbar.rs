//! Scrollbar rules applied to native and CSS-created scroll containers.

use super::super::RenderTree;
use crate::element_style::ScrollbarWidth;
use crate::scroll::{ScrollbarSize, ScrollbarVisibility};
use std::collections::HashSet;

impl RenderTree {
    pub(super) fn apply_base_scrollbar_styles(&mut self) {
        let Some(sheet) = self.stylesheet.clone() else {
            return;
        };
        let mut rules: Vec<_> = sheet
            .complex_rules()
            .iter()
            .filter(|(selector, style)| !selector.has_state() && style.scrollbar_width.is_some())
            .collect();
        rules.sort_by_key(|(selector, style)| Self::rule_order(selector, style));
        let empty = HashSet::new();
        for (&node, physics) in &self.scroll_physics {
            let mut width = None;
            for (selector, style) in &rules {
                if self.complex_selector_matches(selector, node, &empty, &empty, None) {
                    width = style.scrollbar_width;
                }
            }
            if let Some(style) = self
                .element_registry
                .get_id(node)
                .and_then(|id| sheet.get(&id))
            {
                width = style.scrollbar_width.or(width);
            }
            if let Some(width) = width {
                let mut physics = physics.lock().unwrap();
                let config = &mut physics.config.scrollbar;
                config.visibility = if width == ScrollbarWidth::None {
                    ScrollbarVisibility::Never
                } else {
                    ScrollbarVisibility::Auto
                };
                config.size = if width == ScrollbarWidth::Thin {
                    ScrollbarSize::Thin
                } else {
                    ScrollbarSize::Normal
                };
                config.custom_width = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::css_parser::Stylesheet;
    use crate::div::div;

    #[test]
    fn none_keeps_native_scrollbars_hidden_during_scroll() {
        let mut tree = RenderTree::from_element(
            &div()
                .class("numbers")
                .w(100.0)
                .h(100.0)
                .overflow_y_scroll()
                .child(div().h(1000.0)),
        );
        tree.set_stylesheet(Stylesheet::parse(".numbers { scrollbar-width: none; }").unwrap());
        tree.apply_stylesheet_layout_overrides();
        tree.compute_layout(100.0, 100.0);
        let mut physics = tree.scroll_physics.values().next().unwrap().lock().unwrap();
        physics.on_scroll_activity();
        assert_eq!(
            physics.config.scrollbar.visibility,
            ScrollbarVisibility::Never
        );
        assert_eq!(physics.scrollbar_render_info().opacity, 0.0);
    }

    #[test]
    fn id_rule_can_override_class_scrollbar_width() {
        let mut tree = RenderTree::from_element(
            &div()
                .id("list")
                .class("numbers")
                .w(100.0)
                .h(100.0)
                .overflow_y_scroll()
                .child(div().h(1000.0)),
        );
        tree.set_stylesheet(
            Stylesheet::parse(
                ".numbers { scrollbar-width: none; } #list { scrollbar-width: thin; }",
            )
            .unwrap(),
        );
        tree.apply_stylesheet_layout_overrides();
        let physics = tree.scroll_physics.values().next().unwrap().lock().unwrap();
        assert_eq!(
            physics.config.scrollbar.visibility,
            ScrollbarVisibility::Auto
        );
        assert_eq!(physics.config.scrollbar.size, ScrollbarSize::Thin);
    }
}
